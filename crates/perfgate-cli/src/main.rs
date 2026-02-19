use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use object_store::{ObjectStore, path::Path as ObjectPath};
use perfgate_adapters::{StdHostProbe, StdProcessRunner};
use perfgate_app::{
    BenchOutcome, CheckOutcome, CheckRequest, CheckUseCase, Clock, CompareRequest, CompareUseCase,
    ExportFormat, ExportUseCase, PairedRunRequest, PairedRunUseCase, PromoteRequest,
    PromoteUseCase, ReportRequest, ReportUseCase, RunBenchRequest, RunBenchUseCase,
    SensorReportBuilder, SystemClock, classify_error, github_annotations, render_markdown,
    render_markdown_template,
};
use perfgate_domain::{DomainError, SignificancePolicy};
use perfgate_types::{
    BASELINE_REASON_NO_BASELINE, Budget, CompareReceipt, CompareRef, ConfigFile,
    ConfigValidationError, HostMismatchPolicy, Metric, MetricStatistic, PerfgateError, RunReceipt,
    SensorVerdictStatus, ToolInfo,
};
use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;

/// Output mode for the check command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum OutputMode {
    /// Standard mode: exit codes reflect verdict (0=pass, 2=fail, 3=warn with --fail-on-warn)
    #[default]
    Standard,
    /// Cockpit mode: always write receipt, exit 0 unless catastrophic failure
    Cockpit,
}

#[derive(Debug, Parser)]
#[command(
    name = "perfgate",
    version,
    about = "Perf budgets and baseline diffs for CI / PR bots"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a command repeatedly and emit a run receipt (JSON).
    Run {
        /// Bench identifier (used for baselines and reporting)
        #[arg(long)]
        name: String,

        /// Number of measured samples
        #[arg(long, default_value_t = 5)]
        repeat: u32,

        /// Warmup samples (excluded from stats)
        #[arg(long, default_value_t = 0)]
        warmup: u32,

        /// Units of work completed per run (enables throughput_per_s)
        #[arg(long)]
        work: Option<u64>,

        /// Working directory
        #[arg(long)]
        cwd: Option<PathBuf>,

        /// Per-run timeout (e.g. "2s")
        #[arg(long)]
        timeout: Option<String>,

        /// Environment variable (KEY=VALUE). Repeatable.
        #[arg(long, value_parser = parse_key_val_string)]
        env: Vec<(String, String)>,

        /// Max bytes captured from stdout/stderr per run
        #[arg(long, default_value_t = 8192)]
        output_cap_bytes: usize,

        /// Do not fail the tool when the command returns nonzero.
        #[arg(long, default_value_t = false)]
        allow_nonzero: bool,

        /// Include a hashed hostname in the host fingerprint for noise mitigation.
        /// The hostname is SHA-256 hashed for privacy.
        #[arg(long, default_value_t = false)]
        include_hostname_hash: bool,

        /// Output file path
        #[arg(long, default_value = "perfgate.json")]
        out: PathBuf,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,

        /// Command to run (argv) after `--`
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },

    /// Compare a current receipt against a baseline and emit a compare receipt (JSON).
    Compare {
        #[arg(long)]
        baseline: PathBuf,

        #[arg(long)]
        current: PathBuf,

        /// Global regression threshold (0.20 = 20%)
        #[arg(long, default_value_t = 0.20)]
        threshold: f64,

        /// Global warn factor (warn_threshold = threshold * warn_factor)
        #[arg(long, default_value_t = 0.90)]
        warn_factor: f64,

        /// Override per-metric threshold, e.g. wall_ms=0.10
        #[arg(long, value_parser = parse_key_val_f64)]
        metric_threshold: Vec<(String, f64)>,

        /// Override per-metric direction, e.g. throughput_per_s=higher
        #[arg(long, value_parser = parse_key_val_string)]
        direction: Vec<(String, String)>,

        /// Override per-metric statistic, e.g. wall_ms=p95
        #[arg(long, value_parser = parse_key_val_string)]
        metric_stat: Vec<(String, String)>,

        /// Compute per-metric significance metadata using Welch's t-test (p <= alpha).
        #[arg(long)]
        significance_alpha: Option<f64>,

        /// Minimum samples required in each run before significance is computed.
        #[arg(long, default_value_t = 8)]
        significance_min_samples: u32,

        /// When set with --significance-alpha, warn/fail statuses require significance.
        #[arg(long, default_value_t = false)]
        require_significance: bool,

        /// Treat WARN verdict as a failing exit code
        #[arg(long, default_value_t = false)]
        fail_on_warn: bool,

        /// Policy for handling host mismatches between baseline and current runs.
        /// - warn (default): Warn but continue with comparison
        /// - error: Exit 1 on mismatch
        /// - ignore: Suppress warnings
        #[arg(long, default_value = "warn", value_parser = parse_host_mismatch_policy)]
        host_mismatch: HostMismatchPolicy,

        /// Output compare receipt
        #[arg(long, default_value = "perfgate-compare.json")]
        out: PathBuf,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,
    },

    /// Render a Markdown summary from a compare receipt.
    Md {
        #[arg(long)]
        compare: PathBuf,

        /// Output markdown path (default: stdout)
        #[arg(long)]
        out: Option<PathBuf>,

        /// Render markdown using a Handlebars template file.
        #[arg(long)]
        template: Option<PathBuf>,
    },

    /// Emit GitHub Actions annotations from a compare receipt.
    GithubAnnotations {
        #[arg(long)]
        compare: PathBuf,
    },

    /// Export a run or compare receipt to CSV, JSONL, HTML, or Prometheus format.
    Export {
        /// Path to a run receipt (mutually exclusive with --compare)
        #[arg(long, conflicts_with = "compare")]
        run: Option<PathBuf>,

        /// Path to a compare receipt (mutually exclusive with --run)
        #[arg(long, conflicts_with = "run")]
        compare: Option<PathBuf>,

        /// Output format: csv, jsonl, html, or prometheus
        #[arg(long, default_value = "csv")]
        format: String,

        /// Output file path
        #[arg(long)]
        out: PathBuf,
    },

    /// Promote a run receipt to become the new baseline.
    ///
    /// This command copies a run receipt to a baseline location, optionally
    /// normalizing run-specific fields (run_id, timestamps) to make baselines
    /// more stable across runs. Typically used on trusted branches (e.g., main)
    /// after successful benchmark runs.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Promote {
        /// Path or cloud URI (`s3://...`, `gs://...`) to the current run receipt to promote.
        #[arg(long)]
        current: PathBuf,

        /// Path or cloud URI (`s3://...`, `gs://...`) where the baseline should be written.
        #[arg(long)]
        to: PathBuf,

        /// Strip run-specific fields (run_id, timestamps) for stable baselines
        #[arg(long, default_value_t = false)]
        normalize: bool,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,
    },

    /// Generate a cockpit-compatible report from a compare receipt.
    ///
    /// Wraps a CompareReceipt into a `perfgate.report.v1` envelope with
    /// verdict, findings, and summary counts.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Report {
        /// Path to the compare receipt
        #[arg(long)]
        compare: PathBuf,

        /// Output report JSON path
        #[arg(long, default_value = "perfgate-report.json")]
        out: PathBuf,

        /// Also write markdown summary to this path
        #[arg(long)]
        md: Option<PathBuf>,

        /// Render markdown with a Handlebars template file (requires --md).
        #[arg(long, requires = "md")]
        md_template: Option<PathBuf>,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,
    },

    /// Config-driven one-command workflow.
    ///
    /// Reads a config file, runs a benchmark, compares against baseline,
    /// and produces all artifacts (run.json, compare.json, report.json, comment.md).
    ///
    /// This is the main adoption lever for perfgate in CI pipelines.
    ///
    /// Exit codes:
    /// - 0: pass (or warn without --fail-on-warn, or no baseline without --require-baseline)
    /// - 1: tool error (I/O, parse, spawn failures)
    /// - 2: fail (budget violated)
    /// - 3: warn treated as failure (with --fail-on-warn)
    Check {
        /// Path to the config file (TOML or JSON)
        #[arg(long, default_value = "perfgate.toml")]
        config: PathBuf,

        /// Name of the benchmark to run (must match a [[bench]] in config)
        #[arg(long, conflicts_with = "all")]
        bench: Option<String>,

        /// Run all benchmarks defined in the config file
        #[arg(long, default_value_t = false)]
        all: bool,

        /// Regex to filter benchmark names when used with --all
        #[arg(long, requires = "all")]
        bench_regex: Option<String>,

        /// Output directory for artifacts
        #[arg(long, default_value = "artifacts/perfgate")]
        out_dir: PathBuf,

        /// Path or cloud URI to the baseline file.
        /// If not specified, looks in baseline_dir/{bench}.json
        /// (only valid when --bench is specified, not with --all)
        #[arg(long, conflicts_with = "all")]
        baseline: Option<PathBuf>,

        /// Fail if baseline is missing (default: warn and continue)
        #[arg(long, default_value_t = false)]
        require_baseline: bool,

        /// Treat WARN verdict as a failing exit code
        #[arg(long, default_value_t = false)]
        fail_on_warn: bool,

        /// Environment variable (KEY=VALUE). Repeatable.
        #[arg(long, value_parser = parse_key_val_string)]
        env: Vec<(String, String)>,

        /// Max bytes captured from stdout/stderr per run
        #[arg(long, default_value_t = 8192)]
        output_cap_bytes: usize,

        /// Do not fail the tool when the command returns nonzero.
        #[arg(long, default_value_t = false)]
        allow_nonzero: bool,

        /// Policy for handling host mismatches between baseline and current runs.
        /// - warn (default): Warn but continue with comparison
        /// - error: Exit 1 on mismatch
        /// - ignore: Suppress warnings
        #[arg(long, default_value = "warn", value_parser = parse_host_mismatch_policy)]
        host_mismatch: HostMismatchPolicy,

        /// Compute per-metric significance metadata using Welch's t-test (p <= alpha).
        #[arg(long)]
        significance_alpha: Option<f64>,

        /// Minimum samples required in each run before significance is computed.
        #[arg(long, default_value_t = 8)]
        significance_min_samples: u32,

        /// When set with --significance-alpha, warn/fail statuses require significance.
        #[arg(long, default_value_t = false)]
        require_significance: bool,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,

        /// Output mode (standard or cockpit).
        ///
        /// In cockpit mode:
        /// - Always writes sensor.report.v1 envelope to report.json
        /// - Writes native artifacts to extras/ subdirectory
        /// - Exits 0 unless catastrophic failure (cannot write receipt)
        /// - Errors are captured in the report rather than causing exit 1
        #[arg(long, default_value = "standard", value_enum)]
        mode: OutputMode,

        /// Render markdown using a Handlebars template file.
        /// If omitted, falls back to defaults.markdown_template from config.
        #[arg(long)]
        md_template: Option<PathBuf>,

        /// Write GitHub Actions step outputs (verdict/counts) to $GITHUB_OUTPUT.
        #[arg(long, default_value_t = false)]
        output_github: bool,
    },

    /// Run paired benchmark: interleave baseline and current commands for reduced noise.
    ///
    /// Executes baseline-1, current-1, baseline-2, current-2, etc. to minimize
    /// environmental variation between measurements.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Paired {
        /// Bench identifier (used for baselines and reporting)
        #[arg(long)]
        name: String,

        /// Baseline command as a shell string (parsed using shell-words)
        #[arg(long, conflicts_with = "baseline_cmd")]
        baseline: Option<String>,

        /// Current command as a shell string (parsed using shell-words)
        #[arg(long, conflicts_with = "current_cmd")]
        current: Option<String>,

        /// Baseline command.
        /// Accepts either a quoted shell string or raw argv tokens.
        #[arg(long, num_args = 1.., conflicts_with = "baseline")]
        baseline_cmd: Option<Vec<String>>,

        /// Current command.
        /// Accepts either a quoted shell string or raw argv tokens.
        #[arg(long, num_args = 1.., conflicts_with = "current")]
        current_cmd: Option<Vec<String>>,

        /// Number of measured pairs
        #[arg(long, default_value_t = 5)]
        repeat: u32,

        /// Warmup pairs (excluded from stats)
        #[arg(long, default_value_t = 0)]
        warmup: u32,

        /// Units of work completed per run (enables throughput_per_s)
        #[arg(long)]
        work: Option<u64>,

        /// Working directory
        #[arg(long)]
        cwd: Option<PathBuf>,

        /// Per-run timeout (e.g. "2s")
        #[arg(long)]
        timeout: Option<String>,

        /// Environment variable (KEY=VALUE). Repeatable.
        #[arg(long, value_parser = parse_key_val_string)]
        env: Vec<(String, String)>,

        /// Max bytes captured from stdout/stderr per run
        #[arg(long, default_value_t = 8192)]
        output_cap_bytes: usize,

        /// Do not fail the tool when the command returns nonzero.
        #[arg(long, default_value_t = false)]
        allow_nonzero: bool,

        /// Include a hashed hostname in the host fingerprint for noise mitigation.
        #[arg(long, default_value_t = false)]
        include_hostname_hash: bool,

        /// Output file path
        #[arg(long, default_value = "perfgate-paired.json")]
        out: PathBuf,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,
    },
}

fn main() -> ExitCode {
    if let Err(err) = real_main() {
        eprintln!("{err:#}");
        return ExitCode::from(1);
    }
    ExitCode::from(0)
}

fn real_main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run_command(cli.cmd)
}

fn run_command(cmd: Command) -> anyhow::Result<()> {
    match cmd {
        Command::Run {
            name,
            repeat,
            warmup,
            work,
            cwd,
            timeout,
            env,
            output_cap_bytes,
            allow_nonzero,
            include_hostname_hash,
            out,
            pretty,
            command,
        } => {
            let timeout = timeout.as_deref().map(parse_duration).transpose()?;

            let tool = tool_info();
            let runner = StdProcessRunner;
            let host_probe = StdHostProbe;
            let clock = SystemClock;
            let usecase = RunBenchUseCase::new(runner, host_probe, clock, tool);

            let outcome = usecase.execute(RunBenchRequest {
                name,
                cwd,
                command,
                repeat,
                warmup,
                work_units: work,
                timeout,
                env,
                output_cap_bytes,
                allow_nonzero,
                include_hostname_hash,
            })?;

            write_json(&out, &outcome.receipt, pretty)?;

            if outcome.failed && !allow_nonzero {
                // Measurement did complete, but the target command misbehaved.
                // Exit 1 to signal failure while still leaving a receipt artifact.
                anyhow::bail!("benchmark command failed: {}", outcome.reasons.join(", "));
            }

            Ok(())
        }

        Command::Compare {
            baseline,
            current,
            threshold,
            warn_factor,
            metric_threshold,
            direction,
            metric_stat,
            significance_alpha,
            significance_min_samples,
            require_significance,
            fail_on_warn,
            host_mismatch,
            out,
            pretty,
        } => {
            let baseline_receipt: RunReceipt = read_json(&baseline)?;
            let current_receipt: RunReceipt = read_json(&current)?;

            let budgets = build_budgets(
                &baseline_receipt,
                &current_receipt,
                threshold,
                warn_factor,
                metric_threshold,
                direction,
            )?;

            let metric_statistics = build_metric_statistics(&budgets, metric_stat)?;

            let significance = significance_alpha.map(|alpha| SignificancePolicy {
                alpha,
                min_samples: significance_min_samples as usize,
                require_significance,
            });

            let compare_result = CompareUseCase::execute(CompareRequest {
                baseline: baseline_receipt.clone(),
                current: current_receipt.clone(),
                budgets,
                metric_statistics,
                significance,
                baseline_ref: CompareRef {
                    path: Some(baseline.display().to_string()),
                    run_id: Some(baseline_receipt.run.id.clone()),
                },
                current_ref: CompareRef {
                    path: Some(current.display().to_string()),
                    run_id: Some(current_receipt.run.id.clone()),
                },
                tool: tool_info(),
                host_mismatch_policy: host_mismatch,
            })
            .map_err(map_domain_err)?;

            // Print host mismatch warnings if detected (for Warn policy)
            if let Some(mismatch) = &compare_result.host_mismatch {
                for reason in &mismatch.reasons {
                    eprintln!("warning: host mismatch: {}", reason);
                }
            }

            write_json(&out, &compare_result.receipt, pretty)?;

            match compare_result.receipt.verdict.status {
                perfgate_types::VerdictStatus::Pass => Ok(()),
                perfgate_types::VerdictStatus::Warn => {
                    if fail_on_warn {
                        exit_with_code(3)
                    } else {
                        Ok(())
                    }
                }
                perfgate_types::VerdictStatus::Fail => exit_with_code(2),
            }
        }

        Command::Md {
            compare,
            out,
            template,
        } => {
            let compare_receipt: perfgate_types::CompareReceipt = read_json(&compare)?;
            let md = if let Some(template_path) = template {
                let template = fs::read_to_string(&template_path)
                    .with_context(|| format!("read {}", template_path.display()))?;
                render_markdown_template(&compare_receipt, &template)?
            } else {
                render_markdown(&compare_receipt)
            };

            match out {
                Some(path) => {
                    fs::write(&path, md).with_context(|| format!("write {}", path.display()))?;
                }
                None => {
                    print!("{md}");
                }
            }

            Ok(())
        }

        Command::GithubAnnotations { compare } => {
            let compare_receipt: perfgate_types::CompareReceipt = read_json(&compare)?;
            for line in github_annotations(&compare_receipt) {
                println!("{line}");
            }
            Ok(())
        }

        Command::Export {
            run,
            compare,
            format,
            out,
        } => execute_export(run, compare, &format, &out),

        Command::Promote {
            current,
            to,
            normalize,
            pretty,
        } => {
            let receipt: RunReceipt = read_json_from_location(&current)?;

            let result = PromoteUseCase::execute(PromoteRequest { receipt, normalize });

            write_json_to_location(&to, &result.receipt, pretty)?;
            Ok(())
        }

        Command::Report {
            compare,
            out,
            md,
            md_template,
            pretty,
        } => {
            let compare_receipt: CompareReceipt = read_json(&compare)?;

            let result = ReportUseCase::execute(ReportRequest {
                compare: compare_receipt.clone(),
            });

            write_json(&out, &result.report, pretty)?;

            // Optionally write markdown summary
            if let Some(md_path) = md {
                let md_content = if let Some(template_path) = md_template {
                    let template = fs::read_to_string(&template_path)
                        .with_context(|| format!("read {}", template_path.display()))?;
                    render_markdown_template(&compare_receipt, &template)?
                } else {
                    render_markdown(&compare_receipt)
                };
                if let Some(parent) = md_path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("create dir {}", parent.display()))?;
                }
                fs::write(&md_path, md_content)
                    .with_context(|| format!("write {}", md_path.display()))?;
            }

            Ok(())
        }

        Command::Check {
            config,
            bench,
            all,
            bench_regex,
            out_dir,
            baseline,
            require_baseline,
            fail_on_warn,
            env,
            output_cap_bytes,
            allow_nonzero,
            host_mismatch,
            significance_alpha,
            significance_min_samples,
            require_significance,
            pretty,
            mode,
            md_template,
            output_github,
        } => match mode {
            OutputMode::Standard => run_check_standard(
                config,
                bench,
                all,
                bench_regex,
                out_dir,
                baseline,
                require_baseline,
                fail_on_warn,
                env,
                output_cap_bytes,
                allow_nonzero,
                host_mismatch,
                significance_alpha,
                significance_min_samples,
                require_significance,
                pretty,
                md_template,
                output_github,
            ),
            OutputMode::Cockpit => run_check_cockpit(
                config,
                bench,
                all,
                bench_regex,
                out_dir,
                baseline,
                require_baseline,
                fail_on_warn,
                env,
                output_cap_bytes,
                allow_nonzero,
                host_mismatch,
                significance_alpha,
                significance_min_samples,
                require_significance,
                pretty,
                md_template,
                output_github,
            ),
        },

        Command::Paired {
            name,
            baseline,
            current,
            baseline_cmd,
            current_cmd,
            repeat,
            warmup,
            work,
            cwd,
            timeout,
            env,
            output_cap_bytes,
            allow_nonzero,
            include_hostname_hash,
            out,
            pretty,
        } => {
            let timeout = timeout.as_deref().map(parse_duration).transpose()?;

            let baseline_command = match (baseline, baseline_cmd) {
                (Some(s), None) => shell_words::split(&s)
                    .with_context(|| format!("failed to parse baseline command: {}", s))?,
                (None, Some(argv)) => normalize_paired_cli_command(argv, "--baseline-cmd")?,
                _ => anyhow::bail!("either --baseline or --baseline-cmd must be specified"),
            };

            let current_command = match (current, current_cmd) {
                (Some(s), None) => shell_words::split(&s)
                    .with_context(|| format!("failed to parse current command: {}", s))?,
                (None, Some(argv)) => normalize_paired_cli_command(argv, "--current-cmd")?,
                _ => anyhow::bail!("either --current or --current-cmd must be specified"),
            };

            let tool = tool_info();
            let runner = StdProcessRunner;
            let host_probe = StdHostProbe;
            let clock = SystemClock;
            let usecase = PairedRunUseCase::new(runner, host_probe, clock, tool);

            let outcome = usecase.execute(PairedRunRequest {
                name,
                cwd,
                baseline_command,
                current_command,
                repeat,
                warmup,
                work_units: work,
                timeout,
                env,
                output_cap_bytes,
                allow_nonzero,
                include_hostname_hash,
            })?;

            write_json(&out, &outcome.receipt, pretty)?;

            if outcome.failed && !allow_nonzero {
                anyhow::bail!("paired benchmark failed: {}", outcome.reasons.join(", "));
            }

            Ok(())
        }
    }
}

#[cfg(not(test))]
fn exit_with_code(code: i32) -> ! {
    std::process::exit(code);
}

#[cfg(test)]
fn exit_with_code(code: i32) -> ! {
    panic!("exit {code}");
}

/// Run check in standard mode (exit codes reflect verdict).
#[allow(clippy::too_many_arguments)]
fn run_check_standard(
    config: PathBuf,
    bench: Option<String>,
    all: bool,
    bench_regex: Option<String>,
    out_dir: PathBuf,
    baseline: Option<PathBuf>,
    require_baseline: bool,
    fail_on_warn: bool,
    env: Vec<(String, String)>,
    output_cap_bytes: usize,
    allow_nonzero: bool,
    host_mismatch: HostMismatchPolicy,
    significance_alpha: Option<f64>,
    significance_min_samples: u32,
    require_significance: bool,
    pretty: bool,
    md_template: Option<PathBuf>,
    output_github: bool,
) -> anyhow::Result<()> {
    // Load config file
    let config_content =
        fs::read_to_string(&config).with_context(|| format!("read {}", config.display()))?;

    let config_file: ConfigFile = if config.extension().map(|e| e == "json").unwrap_or(false) {
        serde_json::from_str(&config_content)
            .with_context(|| format!("parse JSON config {}", config.display()))?
    } else {
        toml::from_str(&config_content)
            .with_context(|| format!("parse TOML config {}", config.display()))?
    };

    config_file
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;

    // Determine which benches to run
    let bench_names =
        resolve_bench_names(&config_file, bench.as_deref(), all, bench_regex.as_deref())?;
    let bench_count = bench_names.len() as u32;

    let markdown_template_path = md_template.or_else(|| {
        config_file
            .defaults
            .markdown_template
            .as_ref()
            .map(PathBuf::from)
    });
    let markdown_template = load_template(markdown_template_path.as_deref())?;
    let github_output_path = resolve_github_output_path(output_github)?;

    // Track aggregate exit code: fail (2) > warn-as-fail (3) > pass (0)
    let mut max_exit_code: i32 = 0;
    let mut all_warnings: Vec<String> = Vec::new();
    let mut total_pass: u32 = 0;
    let mut total_warn: u32 = 0;
    let mut total_fail: u32 = 0;

    for bench_name in &bench_names {
        // For --all mode, use per-bench subdirectories
        let bench_out_dir = if all {
            out_dir.join(bench_name)
        } else {
            out_dir.clone()
        };

        // Resolve baseline path (--baseline flag only valid for single bench mode)
        let baseline_path = resolve_baseline_path(&baseline, bench_name, &config_file);
        let baseline_receipt = load_optional_baseline_receipt(&baseline_path)
            .map_err(|e| PerfgateError::BaselineResolve(format!("{:#}", e)))?;

        // Create output directory
        fs::create_dir_all(&bench_out_dir).map_err(|e| {
            PerfgateError::ArtifactWrite(format!(
                "create output dir {}: {}",
                bench_out_dir.display(),
                e
            ))
        })?;

        // Execute check
        let runner = StdProcessRunner;
        let host_probe = StdHostProbe;
        let clock = SystemClock;
        let usecase = CheckUseCase::new(runner, host_probe, clock);

        let outcome = usecase.execute(CheckRequest {
            config: config_file.clone(),
            bench_name: bench_name.clone(),
            out_dir: bench_out_dir.clone(),
            baseline: baseline_receipt,
            baseline_path: Some(baseline_path.clone()),
            require_baseline,
            fail_on_warn,
            tool: tool_info(),
            env: env.clone(),
            output_cap_bytes,
            allow_nonzero,
            host_mismatch_policy: host_mismatch,
            significance_alpha,
            significance_min_samples,
            require_significance,
        })?;

        // Write artifacts
        write_check_artifacts(&outcome, pretty)
            .map_err(|e| PerfgateError::ArtifactWrite(format!("{:#}", e)))?;

        if let Some(template) = markdown_template.as_deref() {
            if let Some(compare) = &outcome.compare_receipt {
                let markdown = render_markdown_template(compare, template).with_context(|| {
                    format!("render markdown template for bench '{}'", bench_name)
                })?;
                atomic_write(&outcome.markdown_path, markdown.as_bytes())
                    .map_err(|e| PerfgateError::ArtifactWrite(format!("{:#}", e)))?;
            } else {
                let msg = "markdown template ignored for no-baseline bench".to_string();
                if all {
                    all_warnings.push(format!("[{}] {}", bench_name, msg));
                } else {
                    all_warnings.push(msg);
                }
            }
        }

        // Collect warnings (prefix with bench name in --all mode)
        for warning in &outcome.warnings {
            if all {
                all_warnings.push(format!("[{}] {}", bench_name, warning));
            } else {
                all_warnings.push(warning.clone());
            }
        }

        total_pass += outcome.report.summary.pass_count;
        total_warn += outcome.report.summary.warn_count;
        total_fail += outcome.report.summary.fail_count;

        // Update aggregate exit code (worst wins)
        // Priority: 2 (fail) > 3 (warn-as-fail) > 0 (pass)
        update_max_exit_code(&mut max_exit_code, outcome.exit_code);
    }

    if let Some(path) = github_output_path.as_deref() {
        write_github_outputs(
            path,
            &GitHubOutputSummary {
                verdict: verdict_from_counts(total_pass, total_warn, total_fail),
                pass_count: total_pass,
                warn_count: total_warn,
                fail_count: total_fail,
                bench_count,
                exit_code: max_exit_code,
            },
        )?;
    }

    // Print all warnings
    for warning in &all_warnings {
        eprintln!("warning: {}", warning);
    }

    // Exit with aggregate code
    if max_exit_code != 0 {
        exit_with_code(max_exit_code);
    }

    Ok(())
}

fn resolve_bench_names(
    config_file: &ConfigFile,
    bench: Option<&str>,
    all: bool,
    bench_regex: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if all {
        if config_file.benches.is_empty() {
            anyhow::bail!("no benchmarks defined in config file");
        }

        let mut names: Vec<String> = config_file.benches.iter().map(|b| b.name.clone()).collect();

        if let Some(pattern) = bench_regex {
            let regex = Regex::new(pattern)
                .with_context(|| format!("invalid --bench-regex pattern: {}", pattern))?;
            names.retain(|name| regex.is_match(name));

            if names.is_empty() {
                anyhow::bail!(
                    "--bench-regex '{}' did not match any benchmark names in config",
                    pattern
                );
            }
        }

        return Ok(names);
    }

    if bench_regex.is_some() {
        anyhow::bail!("--bench-regex can only be used with --all");
    }

    if let Some(name) = bench {
        return Ok(vec![name.to_string()]);
    }

    anyhow::bail!("either --bench or --all must be specified")
}

/// Run check in cockpit mode (always write receipt, exit 0 unless catastrophic).
///
/// In cockpit mode:
/// - Captures start time at beginning
/// - Wraps execution in error recovery
/// - Always writes sensor.report.v1 envelope to report.json
/// - Writes native artifacts to extras/ subdirectory
/// - Exits 0 if receipt written, 1 only on catastrophic failure
#[allow(clippy::too_many_arguments)]
fn run_check_cockpit(
    config: PathBuf,
    bench: Option<String>,
    all: bool,
    bench_regex: Option<String>,
    out_dir: PathBuf,
    baseline: Option<PathBuf>,
    require_baseline: bool,
    fail_on_warn: bool,
    env: Vec<(String, String)>,
    output_cap_bytes: usize,
    allow_nonzero: bool,
    host_mismatch: HostMismatchPolicy,
    significance_alpha: Option<f64>,
    significance_min_samples: u32,
    require_significance: bool,
    pretty: bool,
    md_template: Option<PathBuf>,
    output_github: bool,
) -> anyhow::Result<()> {
    let clock = SystemClock;
    let started_at = clock.now_rfc3339();
    let start_instant = Instant::now();
    let github_output_path = resolve_github_output_path(output_github)?;

    // Ensure base output directory exists (catastrophic failure if we can't)
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("create output dir {}", out_dir.display()))?;

    // Try to run the check; capture errors
    let result = run_check_cockpit_inner(
        &config,
        &bench,
        all,
        &bench_regex,
        &out_dir,
        &baseline,
        require_baseline,
        fail_on_warn,
        &env,
        output_cap_bytes,
        allow_nonzero,
        host_mismatch,
        significance_alpha,
        significance_min_samples,
        require_significance,
        pretty,
        &md_template,
        github_output_path.as_deref(),
        &started_at,
        start_instant,
    );

    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            // Error during execution - still try to write an error report
            let ended_at = clock.now_rfc3339();
            let duration_ms = start_instant.elapsed().as_millis() as u64;

            let baseline_available = baseline
                .as_ref()
                .and_then(|p| location_exists(p).ok())
                .unwrap_or(false);

            let (stage, error_kind) = classify_error(&err);

            let builder = SensorReportBuilder::new(tool_info(), started_at)
                .ended_at(ended_at, duration_ms)
                .baseline(baseline_available, None);

            let error_report = builder.build_error(&format!("{:#}", err), stage, error_kind);

            // Try to write the error report
            let report_path = out_dir.join("report.json");
            if write_json(&report_path, &error_report, pretty).is_ok() {
                if let Some(path) = github_output_path.as_deref() {
                    write_github_outputs(
                        path,
                        &GitHubOutputSummary {
                            verdict: verdict_from_sensor(&error_report.verdict.status),
                            pass_count: error_report.verdict.counts.info,
                            warn_count: error_report.verdict.counts.warn,
                            fail_count: error_report.verdict.counts.error,
                            bench_count: 1,
                            exit_code: 0,
                        },
                    )?;
                }

                // Report written successfully - exit 0 per cockpit contract
                eprintln!("error: {:#}", err);
                eprintln!("note: error recorded in {}", report_path.display());
                Ok(())
            } else {
                // Catastrophic: can't even write the report
                Err(err)
            }
        }
    }
}

/// Inner implementation of cockpit mode that may return errors.
#[allow(clippy::too_many_arguments)]
fn run_check_cockpit_inner(
    config: &PathBuf,
    bench: &Option<String>,
    all: bool,
    bench_regex: &Option<String>,
    out_dir: &Path,
    baseline: &Option<PathBuf>,
    require_baseline: bool,
    fail_on_warn: bool,
    env: &[(String, String)],
    output_cap_bytes: usize,
    allow_nonzero: bool,
    host_mismatch: HostMismatchPolicy,
    significance_alpha: Option<f64>,
    significance_min_samples: u32,
    require_significance: bool,
    pretty: bool,
    md_template: &Option<PathBuf>,
    github_output_path: Option<&Path>,
    started_at: &str,
    start_instant: Instant,
) -> anyhow::Result<()> {
    let clock = SystemClock;

    // Load config file
    let config_content =
        fs::read_to_string(config).with_context(|| format!("read {}", config.display()))?;

    let config_file: ConfigFile = if config.extension().map(|e| e == "json").unwrap_or(false) {
        serde_json::from_str(&config_content)
            .with_context(|| format!("parse JSON config {}", config.display()))?
    } else {
        toml::from_str(&config_content)
            .with_context(|| format!("parse TOML config {}", config.display()))?
    };

    config_file
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;

    // Determine which benches to run
    let bench_names =
        resolve_bench_names(&config_file, bench.as_deref(), all, bench_regex.as_deref())?;
    let markdown_template_path = md_template.clone().or_else(|| {
        config_file
            .defaults
            .markdown_template
            .as_ref()
            .map(PathBuf::from)
    });
    let markdown_template = load_template(markdown_template_path.as_deref())?;

    let multi_bench = bench_names.len() > 1;

    // Collect per-bench outcomes
    let mut bench_outcomes: Vec<BenchOutcome> = Vec::new();

    for bench_name in &bench_names {
        let outcome: BenchOutcome = (|| -> anyhow::Result<BenchOutcome> {
            // Create extras directory for native artifacts
            let extras_dir = if multi_bench {
                out_dir.join("extras").join(bench_name)
            } else {
                out_dir.join("extras")
            };
            fs::create_dir_all(&extras_dir).map_err(|e| {
                PerfgateError::ArtifactWrite(format!(
                    "create extras dir {}: {}",
                    extras_dir.display(),
                    e
                ))
            })?;

            // Resolve baseline path
            let baseline_path = resolve_baseline_path(baseline, bench_name, &config_file);
            let baseline_receipt = load_optional_baseline_receipt(&baseline_path)
                .map_err(|e| PerfgateError::BaselineResolve(format!("{:#}", e)))?;
            let baseline_available = baseline_receipt.is_some();

            // Execute check
            let runner = StdProcessRunner;
            let host_probe = StdHostProbe;
            let usecase = CheckUseCase::new(runner, host_probe, clock.clone());

            let check_outcome = usecase.execute(CheckRequest {
                config: config_file.clone(),
                bench_name: bench_name.clone(),
                out_dir: extras_dir.clone(),
                baseline: baseline_receipt,
                baseline_path: Some(baseline_path.clone()),
                require_baseline,
                fail_on_warn,
                tool: tool_info(),
                env: env.to_vec(),
                output_cap_bytes,
                allow_nonzero,
                host_mismatch_policy: host_mismatch,
                significance_alpha,
                significance_min_samples,
                require_significance,
            })?;

            // Write native artifacts to extras/
            write_check_artifacts(&check_outcome, pretty)
                .map_err(|e| PerfgateError::ArtifactWrite(format!("{:#}", e)))?;

            let final_markdown = if let Some(template) = markdown_template.as_deref() {
                if let Some(compare) = &check_outcome.compare_receipt {
                    let rendered =
                        render_markdown_template(compare, template).with_context(|| {
                            format!("render markdown template for bench '{}'", bench_name)
                        })?;
                    atomic_write(&check_outcome.markdown_path, rendered.as_bytes())
                        .map_err(|e| PerfgateError::ArtifactWrite(format!("{:#}", e)))?;
                    rendered
                } else {
                    eprintln!(
                        "warning: [{}] markdown template ignored for no-baseline bench",
                        bench_name
                    );
                    check_outcome.markdown.clone()
                }
            } else {
                check_outcome.markdown.clone()
            };

            // Rename extras files to versioned names
            rename_extras_to_versioned(&extras_dir)
                .map_err(|e| PerfgateError::ArtifactWrite(format!("{:#}", e)))?;

            // Print warnings (CLI concern, not part of aggregation)
            for warning in &check_outcome.warnings {
                eprintln!("warning: {}", warning);
            }

            let extras_prefix = if multi_bench {
                format!("extras/{}", bench_name)
            } else {
                "extras".to_string()
            };

            Ok(BenchOutcome::Success {
                bench_name: bench_name.clone(),
                has_compare: check_outcome.compare_receipt.is_some(),
                baseline_available,
                markdown: final_markdown,
                extras_prefix,
                report: check_outcome.report,
            })
        })()
        .unwrap_or_else(|err| {
            let (stage, error_kind) = classify_error(&err);
            eprintln!("error: bench '{}': {:#}", bench_name, err);
            BenchOutcome::Error {
                bench_name: bench_name.clone(),
                error_message: format!("{:#}", err),
                stage,
                error_kind,
            }
        });
        bench_outcomes.push(outcome);
    }

    // Build aggregated sensor report
    let ended_at = clock.now_rfc3339();
    let duration_ms = start_instant.elapsed().as_millis() as u64;

    let any_baseline_available = bench_outcomes.iter().any(|o| {
        matches!(
            o,
            BenchOutcome::Success {
                baseline_available: true,
                ..
            }
        )
    });

    let baseline_reason = if !any_baseline_available {
        Some(BASELINE_REASON_NO_BASELINE.to_string())
    } else {
        None
    };

    let all_baseline_available = bench_outcomes.iter().all(|o| {
        matches!(
            o,
            BenchOutcome::Success {
                baseline_available: true,
                ..
            }
        )
    });

    let builder = SensorReportBuilder::new(tool_info(), started_at.to_string())
        .ended_at(ended_at, duration_ms)
        .baseline(all_baseline_available, baseline_reason);

    let (sensor_report, combined_markdown) = builder.build_aggregated(&bench_outcomes);

    // Write sensor report to out_dir/report.json
    let report_path = out_dir.join("report.json");
    write_json(&report_path, &sensor_report, pretty)?;

    // Write combined markdown to out_dir root
    let md_dest = out_dir.join("comment.md");
    fs::write(&md_dest, &combined_markdown)
        .with_context(|| format!("write {}", md_dest.display()))?;

    if let Some(path) = github_output_path {
        write_github_outputs(
            path,
            &GitHubOutputSummary {
                verdict: verdict_from_sensor(&sensor_report.verdict.status),
                pass_count: sensor_report.verdict.counts.info,
                warn_count: sensor_report.verdict.counts.warn,
                fail_count: sensor_report.verdict.counts.error,
                bench_count: bench_names.len() as u32,
                exit_code: 0,
            },
        )?;
    }

    // Cockpit mode: always exit 0 if we got here
    Ok(())
}

fn update_max_exit_code(max_exit_code: &mut i32, outcome_exit_code: i32) {
    // Priority: 2 (fail) > 3 (warn-as-fail) > 0 (pass)
    if outcome_exit_code == 2 {
        *max_exit_code = 2;
    } else if outcome_exit_code == 3 && *max_exit_code != 2 {
        *max_exit_code = 3;
    }
}

#[derive(Debug, Clone)]
struct GitHubOutputSummary {
    verdict: &'static str,
    pass_count: u32,
    warn_count: u32,
    fail_count: u32,
    bench_count: u32,
    exit_code: i32,
}

fn verdict_from_counts(pass_count: u32, warn_count: u32, fail_count: u32) -> &'static str {
    if fail_count > 0 {
        "fail"
    } else if warn_count > 0 {
        "warn"
    } else if pass_count > 0 {
        "pass"
    } else {
        "skip"
    }
}

fn verdict_from_sensor(status: &SensorVerdictStatus) -> &'static str {
    match status {
        SensorVerdictStatus::Pass => "pass",
        SensorVerdictStatus::Warn => "warn",
        SensorVerdictStatus::Fail => "fail",
        SensorVerdictStatus::Skip => "skip",
    }
}

fn resolve_github_output_path(output_github: bool) -> anyhow::Result<Option<PathBuf>> {
    if !output_github {
        return Ok(None);
    }

    let path = std::env::var_os("GITHUB_OUTPUT")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("--output-github requires GITHUB_OUTPUT to be set"))?;
    Ok(Some(path))
}

fn write_github_outputs(path: &Path, summary: &GitHubOutputSummary) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;

    writeln!(file, "verdict={}", summary.verdict)?;
    writeln!(file, "pass_count={}", summary.pass_count)?;
    writeln!(file, "warn_count={}", summary.warn_count)?;
    writeln!(file, "fail_count={}", summary.fail_count)?;
    writeln!(file, "bench_count={}", summary.bench_count)?;
    writeln!(file, "exit_code={}", summary.exit_code)?;

    Ok(())
}

fn load_template(path: Option<&Path>) -> anyhow::Result<Option<String>> {
    path.map(|p| fs::read_to_string(p).with_context(|| format!("read {}", p.display())))
        .transpose()
}

fn rename_if_exists(old_path: &Path, new_path: &Path) -> anyhow::Result<()> {
    if old_path.exists() {
        fs::rename(old_path, new_path)
            .with_context(|| format!("rename {} -> {}", old_path.display(), new_path.display()))?;
    }
    Ok(())
}

fn remove_stale_file(stale: &Path) -> anyhow::Result<()> {
    if stale.exists() {
        match fs::remove_file(stale) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "failed to remove stale {}: {}",
                    stale.display(),
                    e
                ));
            }
        }
    }
    Ok(())
}

fn remove_stale_compare_file(stale: &Path) -> anyhow::Result<()> {
    if stale.exists() {
        match fs::remove_file(stale) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "failed to remove stale compare.json {}: {}",
                    stale.display(),
                    e
                ));
            }
        }
    }
    Ok(())
}

/// Rename extras files to versioned names.
fn rename_extras_to_versioned(extras_dir: &Path) -> anyhow::Result<()> {
    let renames = [
        ("run.json", "perfgate.run.v1.json"),
        ("compare.json", "perfgate.compare.v1.json"),
        ("report.json", "perfgate.report.v1.json"),
    ];

    for (old_name, new_name) in &renames {
        let old_path = extras_dir.join(old_name);
        let new_path = extras_dir.join(new_name);
        rename_if_exists(&old_path, &new_path)?;
    }

    // Clean up stale files that might exist from previous runs
    let stale_files = ["run.json", "compare.json", "report.json"];
    for name in &stale_files {
        let stale = extras_dir.join(name);
        remove_stale_file(&stale)?;
    }

    Ok(())
}

/// Resolve the baseline path from CLI args or config defaults.
fn resolve_baseline_path(
    cli_baseline: &Option<PathBuf>,
    bench_name: &str,
    config: &ConfigFile,
) -> PathBuf {
    // 1. CLI takes precedence
    if let Some(path) = cli_baseline {
        return path.clone();
    }

    // 2. Fall back to baseline_pattern from config defaults.
    if let Some(pattern) = &config.defaults.baseline_pattern {
        return render_baseline_pattern(pattern, bench_name);
    }

    // 3. Fall back to baseline_dir from config defaults
    if let Some(baseline_dir) = &config.defaults.baseline_dir {
        if is_remote_storage_uri(baseline_dir) {
            return PathBuf::from(format!(
                "{}/{}.json",
                baseline_dir.trim_end_matches('/'),
                bench_name
            ));
        }
        return PathBuf::from(baseline_dir).join(format!("{}.json", bench_name));
    }

    // 4. Default convention: baselines/{bench_name}.json
    PathBuf::from("baselines").join(format!("{}.json", bench_name))
}

fn render_baseline_pattern(pattern: &str, bench_name: &str) -> PathBuf {
    PathBuf::from(pattern.replace("{bench}", bench_name))
}

fn is_remote_storage_uri(path: &str) -> bool {
    path.starts_with("s3://") || path.starts_with("gs://")
}

/// Write all artifacts from a check outcome.
fn write_check_artifacts(outcome: &CheckOutcome, pretty: bool) -> anyhow::Result<()> {
    // Write run receipt
    write_json(&outcome.run_path, &outcome.run_receipt, pretty)?;

    // Write compare receipt if present
    if let (Some(compare), Some(path)) = (&outcome.compare_receipt, &outcome.compare_path) {
        write_json(path, compare, pretty)?;
    } else if outcome.compare_receipt.is_none() {
        // Ensure compare.json is absent when no baseline is available.
        let parent = outcome.run_path.parent().unwrap_or_else(|| Path::new(""));
        let stale = parent.join("compare.json");
        remove_stale_compare_file(&stale)?;
    }

    // Write report (always present for cockpit integration)
    write_json(&outcome.report_path, &outcome.report, pretty)?;

    // Write markdown
    fs::write(&outcome.markdown_path, &outcome.markdown)
        .with_context(|| format!("write {}", outcome.markdown_path.display()))?;

    Ok(())
}

fn execute_export(
    run: Option<PathBuf>,
    compare: Option<PathBuf>,
    format: &str,
    out: &Path,
) -> anyhow::Result<()> {
    let export_format = ExportFormat::parse(format).ok_or_else(|| {
        anyhow::anyhow!(
            "invalid format: {} (expected csv, jsonl, html, or prometheus)",
            format
        )
    })?;

    let content = match (run, compare) {
        (Some(run_path), None) => {
            let run_receipt: RunReceipt = read_json(&run_path)?;
            ExportUseCase::export_run(&run_receipt, export_format)?
        }
        (None, Some(compare_path)) => {
            let compare_receipt: perfgate_types::CompareReceipt = read_json(&compare_path)?;
            ExportUseCase::export_compare(&compare_receipt, export_format)?
        }
        (None, None) => {
            anyhow::bail!("either --run or --compare must be specified");
        }
        (Some(_), Some(_)) => {
            anyhow::bail!("--run and --compare are mutually exclusive");
        }
    };

    atomic_write(out, content.as_bytes())?;
    Ok(())
}

fn tool_info() -> ToolInfo {
    ToolInfo {
        name: "perfgate".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn map_domain_err(err: anyhow::Error) -> anyhow::Error {
    // Keep it easy to read in CI logs.
    if let Some(DomainError::InvalidBaseline(m)) = err.downcast_ref::<DomainError>() {
        return anyhow::anyhow!("invalid baseline for {m:?}");
    }
    err
}

fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let d = humantime::parse_duration(s).with_context(|| format!("invalid duration: {s}"))?;
    Ok(d)
}

fn parse_key_val_string(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| "expected KEY=VALUE".to_string())?;
    Ok((k.to_string(), v.to_string()))
}

fn parse_key_val_f64(s: &str) -> Result<(String, f64), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| "expected KEY=VALUE".to_string())?;
    let f: f64 = v.parse().map_err(|_| format!("invalid float value: {v}"))?;
    Ok((k.to_string(), f))
}

fn parse_host_mismatch_policy(s: &str) -> Result<HostMismatchPolicy, String> {
    match s {
        "warn" => Ok(HostMismatchPolicy::Warn),
        "error" | "fail" => Ok(HostMismatchPolicy::Error),
        "ignore" => Ok(HostMismatchPolicy::Ignore),
        _ => Err(format!(
            "invalid host mismatch policy: {} (expected warn, error, or ignore)",
            s
        )),
    }
}

fn normalize_paired_cli_command(args: Vec<String>, flag_name: &str) -> anyhow::Result<Vec<String>> {
    if args.is_empty() {
        anyhow::bail!("{} requires at least one argument", flag_name);
    }

    if args.len() == 1 && args[0].chars().any(char::is_whitespace) {
        let raw = &args[0];
        let parsed = shell_words::split(raw)
            .with_context(|| format!("failed to parse {} shell string: {}", flag_name, raw))?;
        if parsed.is_empty() {
            anyhow::bail!("{} parsed to an empty command", flag_name);
        }
        return Ok(parsed);
    }

    Ok(args)
}

struct RemoteLocation {
    store: Arc<dyn object_store::ObjectStore>,
    object_path: ObjectPath,
}

fn parse_remote_location(path: &Path) -> anyhow::Result<Option<RemoteLocation>> {
    let uri = path.to_string_lossy().to_string();
    if !is_remote_storage_uri(&uri) {
        return Ok(None);
    }

    let url = Url::parse(&uri).with_context(|| format!("invalid remote URI {}", uri))?;
    let (store, object_path) =
        object_store::parse_url(&url).with_context(|| format!("parse remote URI {}", uri))?;

    Ok(Some(RemoteLocation {
        store: store.into(),
        object_path,
    }))
}

fn with_tokio_runtime<T, F>(f: F) -> anyhow::Result<T>
where
    F: std::future::Future<Output = anyhow::Result<T>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("initialize async runtime")?;
    rt.block_on(f)
}

fn is_object_not_found(err: &object_store::Error) -> bool {
    matches!(err, object_store::Error::NotFound { .. })
        || err.to_string().to_ascii_lowercase().contains("not found")
}

fn location_exists(path: &Path) -> anyhow::Result<bool> {
    if let Some(remote) = parse_remote_location(path)? {
        let head = with_tokio_runtime(async move {
            remote
                .store
                .head(&remote.object_path)
                .await
                .map_err(anyhow::Error::from)
        });
        return match head {
            Ok(_) => Ok(true),
            Err(err) => {
                if err
                    .downcast_ref::<object_store::Error>()
                    .is_some_and(is_object_not_found)
                {
                    Ok(false)
                } else {
                    Err(err).with_context(|| format!("check existence {}", path.display()))
                }
            }
        };
    }
    Ok(path.exists())
}

fn read_json_from_location<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    if let Some(remote) = parse_remote_location(path)? {
        let bytes = with_tokio_runtime(async move {
            let result = remote
                .store
                .get(&remote.object_path)
                .await
                .map_err(anyhow::Error::from)?;
            result.bytes().await.map_err(anyhow::Error::from)
        })
        .with_context(|| format!("read {}", path.display()))?;

        return serde_json::from_slice(&bytes)
            .with_context(|| format!("parse json {}", path.display()));
    }

    read_json(path)
}

fn write_json_to_location<T: serde::Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> anyhow::Result<()> {
    if let Some(remote) = parse_remote_location(path)? {
        let bytes = if pretty {
            serde_json::to_vec_pretty(value)?
        } else {
            serde_json::to_vec(value)?
        };

        with_tokio_runtime(async move {
            remote
                .store
                .put(&remote.object_path, bytes.into())
                .await
                .map(|_| ())
                .map_err(anyhow::Error::from)
        })
        .with_context(|| format!("write {}", path.display()))?;
        return Ok(());
    }

    write_json(path, value, pretty)
}

fn load_optional_baseline_receipt(path: &Path) -> anyhow::Result<Option<RunReceipt>> {
    if location_exists(path)? {
        Ok(Some(read_json_from_location(path)?))
    } else {
        Ok(None)
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let v =
        serde_json::from_slice(&bytes).with_context(|| format!("parse json {}", path.display()))?;
    Ok(v)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T, pretty: bool) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }

    let bytes = if pretty {
        serde_json::to_vec_pretty(value)?
    } else {
        serde_json::to_vec(value)?
    };

    atomic_write(path, &bytes)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = parent.to_path_buf();
    tmp.push(format!(".{}.tmp", uuid::Uuid::new_v4()));

    {
        let mut f =
            fs::File::create(&tmp).with_context(|| format!("create temp {}", tmp.display()))?;
        f.write_all(bytes)
            .with_context(|| format!("write temp {}", tmp.display()))?;
        f.sync_all().ok();
    }

    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

fn build_budgets(
    baseline: &RunReceipt,
    current: &RunReceipt,
    global_threshold: f64,
    global_warn_factor: f64,
    metric_thresholds: Vec<(String, f64)>,
    direction_overrides: Vec<(String, String)>,
) -> anyhow::Result<BTreeMap<Metric, Budget>> {
    // Determine candidate metrics: those present in both baseline+current.
    let mut candidates = Vec::new();
    candidates.push(Metric::WallMs);
    if baseline.stats.binary_bytes.is_some() && current.stats.binary_bytes.is_some() {
        candidates.push(Metric::BinaryBytes);
    }
    if baseline.stats.cpu_ms.is_some() && current.stats.cpu_ms.is_some() {
        candidates.push(Metric::CpuMs);
    }
    if baseline.stats.ctx_switches.is_some() && current.stats.ctx_switches.is_some() {
        candidates.push(Metric::CtxSwitches);
    }
    if baseline.stats.max_rss_kb.is_some() && current.stats.max_rss_kb.is_some() {
        candidates.push(Metric::MaxRssKb);
    }
    if baseline.stats.page_faults.is_some() && current.stats.page_faults.is_some() {
        candidates.push(Metric::PageFaults);
    }
    if baseline.stats.throughput_per_s.is_some() && current.stats.throughput_per_s.is_some() {
        candidates.push(Metric::ThroughputPerS);
    }

    let mut thresholds: BTreeMap<String, f64> = metric_thresholds.into_iter().collect();
    let mut dirs: BTreeMap<String, String> = direction_overrides.into_iter().collect();

    let mut budgets = BTreeMap::new();

    for metric in candidates {
        let key = metric_key(metric);
        let threshold = thresholds.remove(key).unwrap_or(global_threshold);
        let warn_threshold = threshold * global_warn_factor;
        let dir = match dirs.remove(key).as_deref() {
            Some("lower") => perfgate_types::Direction::Lower,
            Some("higher") => perfgate_types::Direction::Higher,
            Some(other) => {
                anyhow::bail!("invalid direction for {key}: {other} (expected lower|higher)")
            }
            None => metric.default_direction(),
        };

        budgets.insert(
            metric,
            Budget {
                threshold,
                warn_threshold,
                direction: dir,
            },
        );
    }

    Ok(budgets)
}

fn build_metric_statistics(
    budgets: &BTreeMap<Metric, Budget>,
    overrides: Vec<(String, String)>,
) -> anyhow::Result<BTreeMap<Metric, MetricStatistic>> {
    let mut statistics = BTreeMap::new();

    for (key, value) in overrides {
        let metric = Metric::parse_key(&key)
            .ok_or_else(|| anyhow::anyhow!("unknown metric for --metric-stat: {}", key))?;
        if !budgets.contains_key(&metric) {
            anyhow::bail!(
                "metric-stat override for {} is not applicable (metric not present in both receipts)",
                key
            );
        }

        let statistic = parse_metric_statistic(&value).ok_or_else(|| {
            anyhow::anyhow!(
                "invalid statistic for {}: {} (expected median|p95)",
                key,
                value
            )
        })?;

        statistics.insert(metric, statistic);
    }

    Ok(statistics)
}

fn parse_metric_statistic(s: &str) -> Option<MetricStatistic> {
    match s {
        "median" => Some(MetricStatistic::Median),
        "p95" => Some(MetricStatistic::P95),
        _ => None,
    }
}

fn metric_key(metric: Metric) -> &'static str {
    metric.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, DefaultsConfig, Direction, F64Summary, HostInfo, PerfgateReport, RUN_SCHEMA_V1,
        ReportSummary, RunMeta, RunReceipt, Stats, U64Summary, Verdict, VerdictCounts,
        VerdictStatus,
    };
    use serde_json::json;
    use std::any::Any;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn make_stats(cpu: bool, rss: bool, throughput: bool) -> Stats {
        Stats {
            wall_ms: U64Summary {
                median: 100,
                min: 90,
                max: 110,
            },
            cpu_ms: cpu.then_some(U64Summary {
                median: 50,
                min: 40,
                max: 60,
            }),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: rss.then_some(U64Summary {
                median: 2048,
                min: 1024,
                max: 4096,
            }),
            binary_bytes: None,
            throughput_per_s: throughput.then_some(F64Summary {
                median: 200.0,
                min: 180.0,
                max: 220.0,
            }),
        }
    }

    fn make_receipt(stats: Stats) -> RunReceipt {
        RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: tool_info(),
            run: RunMeta {
                id: "run-id".to_string(),
                started_at: "2020-01-01T00:00:00Z".to_string(),
                ended_at: "2020-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(8),
                    memory_bytes: Some(8 * 1024 * 1024 * 1024),
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hi".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: Vec::new(),
            stats,
        }
    }

    fn make_stats_with_wall(wall_ms: u64) -> Stats {
        Stats {
            wall_ms: U64Summary {
                median: wall_ms,
                min: wall_ms,
                max: wall_ms,
            },
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            binary_bytes: None,
            throughput_per_s: None,
        }
    }

    #[cfg(unix)]
    fn slow_command() -> Vec<String> {
        vec!["sh".to_string(), "-c".to_string(), "sleep 0.05".to_string()]
    }

    #[cfg(windows)]
    fn slow_command() -> Vec<String> {
        vec![
            "powershell".to_string(),
            "-Command".to_string(),
            "Start-Sleep -Milliseconds 50".to_string(),
        ]
    }

    fn create_compare_receipt_json(verdict_status: &str, metric_status: &str) -> String {
        format!(
            r#"{{
  "schema": "perfgate.compare.v1",
  "tool": {{"name": "perfgate", "version": "0.1.0"}},
  "bench": {{"name": "test-bench", "cwd": null, "command": ["true"], "repeat": 1, "warmup": 0}},
  "baseline_ref": {{"path": "baseline.json", "run_id": "b123"}},
  "current_ref": {{"path": "current.json", "run_id": "c456"}},
  "budgets": {{"wall_ms": {{"threshold": 0.2, "warn_threshold": 0.18, "direction": "lower"}}}},
  "deltas": {{"wall_ms": {{"baseline": 100.0, "current": 150.0, "ratio": 1.5, "pct": 0.5, "regression": 0.5, "status": "{}"}}}},
  "verdict": {{"status": "{}", "counts": {{"pass": 0, "warn": 0, "fail": 1}}, "reasons": ["wall_ms_fail"]}}
}}"#,
            metric_status, verdict_status
        )
    }

    fn create_check_config_json(
        temp_dir: &std::path::Path,
        bench_name: &str,
    ) -> std::path::PathBuf {
        let config_path = temp_dir.join("perfgate.json");
        let cmd = slow_command();
        let config = json!({
            "defaults": {
                "repeat": 1,
                "warmup": 0,
                "threshold": 0.0,
                "warn_factor": 0.0
            },
            "bench": [{
                "name": bench_name,
                "command": cmd
            }]
        });

        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
            .expect("Failed to write config file");

        config_path
    }

    fn assert_exit_code(result: Result<(), Box<dyn Any + Send>>, expected: i32) {
        let err = result.expect_err("expected exit panic");
        let msg = if let Some(s) = err.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = err.downcast_ref::<&str>() {
            s.to_string()
        } else {
            "<non-string panic>".to_string()
        };
        assert!(
            msg.contains(&format!("exit {}", expected)),
            "unexpected panic: {}",
            msg
        );
    }

    #[test]
    fn parse_duration_accepts_humantime() {
        let d = parse_duration("150ms").unwrap();
        assert_eq!(d, Duration::from_millis(150));
    }

    #[test]
    fn parse_duration_rejects_invalid() {
        let err = parse_duration("not-a-duration").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid duration"),
            "unexpected error: {}",
            msg
        );
        assert!(msg.contains("not-a-duration"), "unexpected error: {}", msg);
    }

    #[test]
    fn parse_key_val_string_parses_and_errors() {
        let (k, v) = parse_key_val_string("KEY=VALUE").unwrap();
        assert_eq!(k, "KEY");
        assert_eq!(v, "VALUE");

        let err = parse_key_val_string("NOPE").unwrap_err();
        assert_eq!(err, "expected KEY=VALUE");
    }

    #[test]
    fn parse_key_val_f64_parses_and_errors() {
        let (k, v) = parse_key_val_f64("wall_ms=0.25").unwrap();
        assert_eq!(k, "wall_ms");
        assert!((v - 0.25).abs() < f64::EPSILON);

        let err = parse_key_val_f64("wall_ms=abc").unwrap_err();
        assert!(
            err.contains("invalid float value: abc"),
            "unexpected error: {}",
            err
        );

        let err = parse_key_val_f64("missing_equals").unwrap_err();
        assert_eq!(err, "expected KEY=VALUE");
    }

    #[test]
    fn parse_host_mismatch_policy_accepts_and_errors() {
        assert_eq!(
            parse_host_mismatch_policy("warn").unwrap(),
            HostMismatchPolicy::Warn
        );
        assert_eq!(
            parse_host_mismatch_policy("error").unwrap(),
            HostMismatchPolicy::Error
        );
        assert_eq!(
            parse_host_mismatch_policy("ignore").unwrap(),
            HostMismatchPolicy::Ignore
        );

        let err = parse_host_mismatch_policy("maybe").unwrap_err();
        assert!(
            err.contains("invalid host mismatch policy"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn normalize_paired_cli_command_splits_single_shell_string() {
        let parsed =
            normalize_paired_cli_command(vec!["cmd /c exit 0".to_string()], "--baseline-cmd")
                .expect("parse shell string");
        assert_eq!(parsed, vec!["cmd", "/c", "exit", "0"]);
    }

    #[test]
    fn normalize_paired_cli_command_keeps_argv_tokens() {
        let args = vec![
            "cmd".to_string(),
            "/c".to_string(),
            "exit".to_string(),
            "0".to_string(),
        ];
        let parsed =
            normalize_paired_cli_command(args.clone(), "--baseline-cmd").expect("parse argv");
        assert_eq!(parsed, args);
    }

    #[test]
    fn normalize_paired_cli_command_keeps_single_token() {
        let args = vec!["true".to_string()];
        let parsed =
            normalize_paired_cli_command(args.clone(), "--baseline-cmd").expect("single token");
        assert_eq!(parsed, args);
    }

    #[test]
    fn resolve_baseline_path_prefers_cli_then_pattern_then_config_then_default() {
        let config = ConfigFile {
            defaults: DefaultsConfig {
                baseline_pattern: Some("pattern/{bench}.receipt.json".to_string()),
                baseline_dir: Some("bases".to_string()),
                ..Default::default()
            },
            benches: Vec::new(),
        };

        let cli = Some(PathBuf::from("cli.json"));
        assert_eq!(
            resolve_baseline_path(&cli, "bench", &config),
            PathBuf::from("cli.json")
        );

        let no_cli = None;
        assert_eq!(
            resolve_baseline_path(&no_cli, "bench", &config),
            PathBuf::from("pattern").join("bench.receipt.json")
        );

        let config_dir_only = ConfigFile {
            defaults: DefaultsConfig {
                baseline_dir: Some("bases".to_string()),
                ..Default::default()
            },
            benches: Vec::new(),
        };
        assert_eq!(
            resolve_baseline_path(&no_cli, "bench", &config_dir_only),
            PathBuf::from("bases").join("bench.json")
        );

        let config_no_default = ConfigFile::default();
        assert_eq!(
            resolve_baseline_path(&no_cli, "bench", &config_no_default),
            PathBuf::from("baselines").join("bench.json")
        );
    }

    #[test]
    fn resolve_baseline_path_supports_remote_baseline_dir() {
        let config = ConfigFile {
            defaults: DefaultsConfig {
                baseline_dir: Some("s3://my-bucket/baselines".to_string()),
                ..Default::default()
            },
            benches: Vec::new(),
        };

        let no_cli = None;
        assert_eq!(
            resolve_baseline_path(&no_cli, "bench-a", &config),
            PathBuf::from("s3://my-bucket/baselines/bench-a.json")
        );
    }

    #[test]
    fn is_remote_storage_uri_accepts_s3_and_gs_only() {
        assert!(is_remote_storage_uri("s3://bucket/key.json"));
        assert!(is_remote_storage_uri("gs://bucket/key.json"));
        assert!(!is_remote_storage_uri("file:///tmp/key.json"));
        assert!(!is_remote_storage_uri("baselines/key.json"));
    }

    #[test]
    fn rename_extras_to_versioned_moves_files() {
        let dir = tempdir().unwrap();
        let extras = dir.path();
        fs::write(extras.join("run.json"), "run").unwrap();
        fs::write(extras.join("report.json"), "report").unwrap();

        rename_extras_to_versioned(extras).unwrap();

        assert!(extras.join("perfgate.run.v1.json").exists());
        assert!(extras.join("perfgate.report.v1.json").exists());
        assert!(!extras.join("run.json").exists());
        assert!(!extras.join("report.json").exists());
    }

    #[test]
    fn atomic_write_writes_and_cleans_temp() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.txt");
        atomic_write(&path, b"hello").unwrap();

        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "hello");

        let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn write_json_and_read_json_round_trip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested").join("value.json");
        let value = json!({ "hello": "world", "n": 1 });

        write_json(&path, &value, true).unwrap();
        let read: serde_json::Value = read_json(&path).unwrap();
        assert_eq!(read, value);
    }

    #[test]
    fn write_json_to_location_and_read_json_from_location_round_trip_local() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("value.json");
        let value = json!({ "hello": "location", "n": 2 });

        write_json_to_location(&path, &value, false).unwrap();
        let read: serde_json::Value = read_json_from_location(&path).unwrap();
        assert_eq!(read, value);
        assert!(location_exists(&path).unwrap());
    }

    #[test]
    fn read_json_reports_parse_error() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "not-json").unwrap();

        let err = read_json::<serde_json::Value>(&path).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("parse json"), "unexpected error: {}", msg);
    }

    #[test]
    fn map_domain_err_rewrites_invalid_baseline() {
        let err = anyhow::Error::new(DomainError::InvalidBaseline(Metric::WallMs));
        let mapped = map_domain_err(err);
        assert_eq!(mapped.to_string(), "invalid baseline for WallMs");
    }

    #[test]
    fn build_budgets_includes_matching_metrics_and_applies_overrides() {
        let baseline = make_receipt(make_stats(true, true, true));
        let current = make_receipt(make_stats(true, false, true));

        let budgets = build_budgets(
            &baseline,
            &current,
            0.20,
            0.90,
            vec![("cpu_ms".to_string(), 0.10)],
            vec![("throughput_per_s".to_string(), "lower".to_string())],
        )
        .unwrap();

        assert!(budgets.contains_key(&Metric::WallMs));
        assert!(budgets.contains_key(&Metric::CpuMs));
        assert!(budgets.contains_key(&Metric::ThroughputPerS));
        assert!(!budgets.contains_key(&Metric::MaxRssKb));

        let cpu = budgets.get(&Metric::CpuMs).unwrap();
        assert!((cpu.threshold - 0.10).abs() < f64::EPSILON);
        assert!((cpu.warn_threshold - 0.09).abs() < f64::EPSILON);

        let throughput = budgets.get(&Metric::ThroughputPerS).unwrap();
        assert_eq!(throughput.direction, Direction::Lower);
    }

    #[test]
    fn build_budgets_rejects_invalid_direction() {
        let baseline = make_receipt(make_stats(false, false, false));
        let current = make_receipt(make_stats(false, false, false));

        let err = build_budgets(
            &baseline,
            &current,
            0.20,
            0.90,
            Vec::new(),
            vec![("wall_ms".to_string(), "sideways".to_string())],
        )
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("invalid direction for wall_ms"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn compare_command_rejects_invalid_direction() {
        let dir = tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");
        let current_path = dir.path().join("current.json");
        let out_path = dir.path().join("compare.json");

        write_json(
            &baseline_path,
            &make_receipt(make_stats_with_wall(100)),
            false,
        )
        .unwrap();
        write_json(
            &current_path,
            &make_receipt(make_stats_with_wall(100)),
            false,
        )
        .unwrap();

        let err = run_command(Command::Compare {
            baseline: baseline_path,
            current: current_path,
            threshold: 0.20,
            warn_factor: 0.90,
            metric_threshold: Vec::new(),
            direction: vec![("wall_ms".to_string(), "sideways".to_string())],
            metric_stat: Vec::new(),
            significance_alpha: None,
            significance_min_samples: 8,
            require_significance: false,
            fail_on_warn: false,
            host_mismatch: HostMismatchPolicy::Warn,
            out: out_path,
            pretty: false,
        })
        .unwrap_err();

        assert!(
            err.to_string().contains("invalid direction"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn compare_command_rejects_invalid_metric_statistic() {
        let dir = tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");
        let current_path = dir.path().join("current.json");
        let out_path = dir.path().join("compare.json");

        write_json(
            &baseline_path,
            &make_receipt(make_stats_with_wall(100)),
            false,
        )
        .unwrap();
        write_json(
            &current_path,
            &make_receipt(make_stats_with_wall(100)),
            false,
        )
        .unwrap();

        let err = run_command(Command::Compare {
            baseline: baseline_path,
            current: current_path,
            threshold: 0.20,
            warn_factor: 0.90,
            metric_threshold: Vec::new(),
            direction: Vec::new(),
            metric_stat: vec![("wall_ms".to_string(), "p99".to_string())],
            significance_alpha: None,
            significance_min_samples: 8,
            require_significance: false,
            fail_on_warn: false,
            host_mismatch: HostMismatchPolicy::Warn,
            out: out_path,
            pretty: false,
        })
        .unwrap_err();

        assert!(
            err.to_string().contains("invalid statistic"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn compare_command_exits_with_fail_code() {
        let dir = tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");
        let current_path = dir.path().join("current.json");
        let out_path = dir.path().join("compare.json");

        write_json(
            &baseline_path,
            &make_receipt(make_stats_with_wall(100)),
            false,
        )
        .unwrap();
        write_json(
            &current_path,
            &make_receipt(make_stats_with_wall(150)),
            false,
        )
        .unwrap();

        let out_clone = out_path.clone();
        let result = std::panic::catch_unwind(|| {
            run_command(Command::Compare {
                baseline: baseline_path,
                current: current_path,
                threshold: 0.20,
                warn_factor: 0.90,
                metric_threshold: Vec::new(),
                direction: Vec::new(),
                metric_stat: Vec::new(),
                significance_alpha: None,
                significance_min_samples: 8,
                require_significance: false,
                fail_on_warn: false,
                host_mismatch: HostMismatchPolicy::Warn,
                out: out_clone,
                pretty: false,
            })
            .unwrap();
        });

        assert_exit_code(result, 2);
        assert!(out_path.exists());
    }

    #[test]
    fn compare_command_exits_with_warn_code() {
        let dir = tempdir().unwrap();
        let baseline_path = dir.path().join("baseline.json");
        let current_path = dir.path().join("current.json");
        let out_path = dir.path().join("compare.json");

        write_json(
            &baseline_path,
            &make_receipt(make_stats_with_wall(100)),
            false,
        )
        .unwrap();
        write_json(
            &current_path,
            &make_receipt(make_stats_with_wall(119)),
            false,
        )
        .unwrap();

        let out_clone = out_path.clone();
        let result = std::panic::catch_unwind(|| {
            run_command(Command::Compare {
                baseline: baseline_path,
                current: current_path,
                threshold: 0.20,
                warn_factor: 0.90,
                metric_threshold: Vec::new(),
                direction: Vec::new(),
                metric_stat: Vec::new(),
                significance_alpha: None,
                significance_min_samples: 8,
                require_significance: false,
                fail_on_warn: true,
                host_mismatch: HostMismatchPolicy::Warn,
                out: out_clone,
                pretty: false,
            })
            .unwrap();
        });

        assert_exit_code(result, 3);
        assert!(out_path.exists());
    }

    #[test]
    fn report_command_creates_parent_dirs_for_md() {
        let dir = tempdir().unwrap();
        let compare_path = dir.path().join("compare.json");
        let report_path = dir.path().join("report.json");
        let md_path = dir.path().join("nested").join("comment.md");

        fs::write(&compare_path, create_compare_receipt_json("pass", "pass")).unwrap();

        run_command(Command::Report {
            compare: compare_path,
            out: report_path,
            md: Some(md_path.clone()),
            md_template: None,
            pretty: false,
        })
        .unwrap();

        assert!(md_path.exists());
    }

    #[test]
    fn report_command_skips_parent_dir_for_relative_md() {
        let dir = tempdir().unwrap();
        let compare_path = dir.path().join("compare.json");
        let report_path = dir.path().join("report.json");
        let md_name = format!("comment_{}.md", Uuid::new_v4());
        let md_path = PathBuf::from(&md_name);

        fs::write(&compare_path, create_compare_receipt_json("pass", "pass")).unwrap();

        run_command(Command::Report {
            compare: compare_path,
            out: report_path,
            md: Some(md_path.clone()),
            md_template: None,
            pretty: false,
        })
        .unwrap();

        assert!(md_path.exists());
        let _ = fs::remove_file(&md_path);
    }

    #[test]
    fn report_command_errors_on_empty_md_path() {
        let dir = tempdir().unwrap();
        let compare_path = dir.path().join("compare.json");
        let report_path = dir.path().join("report.json");

        fs::write(&compare_path, create_compare_receipt_json("pass", "pass")).unwrap();

        let err = run_command(Command::Report {
            compare: compare_path,
            out: report_path,
            md: Some(PathBuf::from("")),
            md_template: None,
            pretty: false,
        })
        .unwrap_err();

        assert!(err.to_string().contains("write"));
    }

    #[test]
    fn run_check_standard_exits_with_fail_code() {
        let dir = tempdir().unwrap();
        let config_path = create_check_config_json(dir.path(), "bench");
        let baseline_path = dir.path().join("baseline.json");
        let out_dir = dir.path().join("out");

        write_json(
            &baseline_path,
            &make_receipt(make_stats_with_wall(1)),
            false,
        )
        .unwrap();

        let result = std::panic::catch_unwind(|| {
            run_check_standard(
                config_path,
                Some("bench".to_string()),
                false,
                None,
                out_dir,
                Some(baseline_path),
                false,
                false,
                Vec::new(),
                8192,
                false,
                HostMismatchPolicy::Warn,
                None,
                8,
                false,
                false,
                None,
                false,
            )
            .unwrap();
        });

        assert_exit_code(result, 2);
    }

    #[test]
    fn run_check_cockpit_returns_error_when_report_write_fails() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();
        fs::create_dir_all(out_dir.join("report.json")).unwrap();

        let config_path = dir.path().join("perfgate.json");
        let config = json!({
            "defaults": {},
            "bench": []
        });
        fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        let err = run_check_cockpit(
            config_path,
            None,
            false,
            None,
            out_dir,
            None,
            false,
            false,
            Vec::new(),
            8192,
            false,
            HostMismatchPolicy::Warn,
            None,
            8,
            false,
            false,
            None,
            false,
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("either --bench or --all must be specified"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn update_max_exit_code_prioritizes_fail_over_warn() {
        let mut max_exit_code = 0;
        update_max_exit_code(&mut max_exit_code, 3);
        assert_eq!(max_exit_code, 3);
        update_max_exit_code(&mut max_exit_code, 2);
        assert_eq!(max_exit_code, 2);
        update_max_exit_code(&mut max_exit_code, 3);
        assert_eq!(max_exit_code, 2);
    }

    #[test]
    fn execute_export_rejects_both_run_and_compare() {
        let dir = tempdir().unwrap();
        let run_path = dir.path().join("run.json");
        let compare_path = dir.path().join("compare.json");
        let out_path = dir.path().join("out.csv");

        let err = execute_export(Some(run_path), Some(compare_path), "csv", &out_path).unwrap_err();
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn rename_if_exists_reports_error_on_invalid_target() {
        let dir = tempdir().unwrap();
        let old_path = dir.path().join("run.json");
        let new_path = dir.path().join("perfgate.run.v1.json");
        fs::write(&old_path, "data").unwrap();
        fs::create_dir_all(&new_path).unwrap();

        let err = rename_if_exists(&old_path, &new_path).unwrap_err();
        assert!(
            err.to_string().contains("rename"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn remove_stale_file_removes_existing_file() {
        let dir = tempdir().unwrap();
        let stale = dir.path().join("run.json");
        fs::write(&stale, "data").unwrap();

        remove_stale_file(&stale).unwrap();

        assert!(!stale.exists());
    }

    #[test]
    fn remove_stale_file_reports_error_on_directory() {
        let dir = tempdir().unwrap();
        let stale_dir = dir.path().join("run.json");
        fs::create_dir_all(&stale_dir).unwrap();

        let err = remove_stale_file(&stale_dir).unwrap_err();
        assert!(
            err.to_string().contains("failed to remove stale"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn remove_stale_compare_file_reports_error_on_directory() {
        let dir = tempdir().unwrap();
        let stale_dir = dir.path().join("compare.json");
        fs::create_dir_all(&stale_dir).unwrap();

        let err = remove_stale_compare_file(&stale_dir).unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to remove stale compare.json"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn write_check_artifacts_removes_stale_compare_when_missing_baseline() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();

        let run_path = out_dir.join("run.json");
        let report_path = out_dir.join("report.json");
        let markdown_path = out_dir.join("comment.md");
        let stale_compare = out_dir.join("compare.json");

        fs::write(&stale_compare, "stale").unwrap();

        let report = PerfgateReport {
            report_type: "perfgate.report.v1".to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                },
                reasons: Vec::new(),
            },
            compare: None,
            findings: Vec::new(),
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 0,
                total_count: 0,
            },
        };

        let outcome = CheckOutcome {
            run_receipt: make_receipt(make_stats_with_wall(100)),
            run_path: run_path.clone(),
            compare_receipt: None,
            compare_path: None,
            report,
            report_path: report_path.clone(),
            markdown: "hello".to_string(),
            markdown_path: markdown_path.clone(),
            warnings: Vec::new(),
            failed: false,
            exit_code: 0,
        };

        write_check_artifacts(&outcome, false).unwrap();

        assert!(!stale_compare.exists());
        assert!(run_path.exists());
        assert!(report_path.exists());
        assert!(markdown_path.exists());
    }

    #[test]
    fn write_check_artifacts_skips_compare_when_path_missing() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();

        let run_path = out_dir.join("run.json");
        let report_path = out_dir.join("report.json");
        let markdown_path = out_dir.join("comment.md");

        let compare_receipt: CompareReceipt =
            serde_json::from_str(&create_compare_receipt_json("pass", "pass")).unwrap();

        let report = PerfgateReport {
            report_type: "perfgate.report.v1".to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                },
                reasons: Vec::new(),
            },
            compare: None,
            findings: Vec::new(),
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 0,
                total_count: 0,
            },
        };

        let outcome = CheckOutcome {
            run_receipt: make_receipt(make_stats_with_wall(100)),
            run_path: run_path.clone(),
            compare_receipt: Some(compare_receipt),
            compare_path: None,
            report,
            report_path: report_path.clone(),
            markdown: "hello".to_string(),
            markdown_path: markdown_path.clone(),
            warnings: Vec::new(),
            failed: false,
            exit_code: 0,
        };

        write_check_artifacts(&outcome, false).unwrap();

        assert!(run_path.exists());
        assert!(report_path.exists());
        assert!(markdown_path.exists());
    }

    #[test]
    fn write_json_skips_parent_dir_for_relative_path() {
        let name = format!("write_json_{}.json", Uuid::new_v4());
        let path = PathBuf::from(&name);
        let receipt = make_receipt(make_stats_with_wall(1));

        write_json(&path, &receipt, false).unwrap();

        assert!(path.exists());
        let _ = fs::remove_file(&path);
    }
}
