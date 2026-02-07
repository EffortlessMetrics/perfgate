use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use perfgate_adapters::{StdHostProbe, StdProcessRunner};
use perfgate_app::{
    classify_error, github_annotations, render_markdown, sensor_fingerprint, CheckOutcome,
    CheckRequest, CheckUseCase, Clock, CompareRequest, CompareUseCase, ExportFormat, ExportUseCase,
    PairedRunRequest, PairedRunUseCase, PromoteRequest, PromoteUseCase, ReportRequest,
    ReportUseCase, RunBenchRequest, RunBenchUseCase, SensorReportBuilder, SystemClock,
};
use perfgate_domain::DomainError;
use perfgate_types::{
    Budget, CompareReceipt, CompareRef, ConfigFile, HostMismatchPolicy, Metric, RunReceipt,
    ToolInfo, BASELINE_REASON_NO_BASELINE, CHECK_ID_TOOL_TRUNCATION, FINDING_CODE_TRUNCATED,
    MAX_FINDINGS_DEFAULT, VERDICT_REASON_TRUNCATED,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

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
    },

    /// Emit GitHub Actions annotations from a compare receipt.
    GithubAnnotations {
        #[arg(long)]
        compare: PathBuf,
    },

    /// Export a run or compare receipt to CSV or JSONL format.
    Export {
        /// Path to a run receipt (mutually exclusive with --compare)
        #[arg(long, conflicts_with = "compare")]
        run: Option<PathBuf>,

        /// Path to a compare receipt (mutually exclusive with --run)
        #[arg(long, conflicts_with = "run")]
        compare: Option<PathBuf>,

        /// Output format: csv or jsonl
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
        /// Path to the current run receipt to promote
        #[arg(long)]
        current: PathBuf,

        /// Path where the baseline should be written
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

        /// Output directory for artifacts
        #[arg(long, default_value = "artifacts/perfgate")]
        out_dir: PathBuf,

        /// Path to the baseline file. If not specified, looks in baseline_dir/{bench}.json
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

        /// Baseline command (argv)
        #[arg(long, required = true, num_args = 1..)]
        baseline_cmd: Vec<String>,

        /// Current command (argv)
        #[arg(long, required = true, num_args = 1..)]
        current_cmd: Vec<String>,

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

    match cli.cmd {
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

            let compare_result = CompareUseCase::execute(CompareRequest {
                baseline: baseline_receipt.clone(),
                current: current_receipt.clone(),
                budgets,
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
            if let Some(ref mismatch) = compare_result.host_mismatch {
                for reason in &mismatch.reasons {
                    eprintln!("warning: host mismatch: {}", reason);
                }
            }

            write_json(&out, &compare_result.receipt, pretty)?;

            match compare_result.receipt.verdict.status {
                perfgate_types::VerdictStatus::Pass => Ok(()),
                perfgate_types::VerdictStatus::Warn => {
                    if fail_on_warn {
                        std::process::exit(3)
                    } else {
                        Ok(())
                    }
                }
                perfgate_types::VerdictStatus::Fail => std::process::exit(2),
            }
        }

        Command::Md { compare, out } => {
            let compare_receipt: perfgate_types::CompareReceipt = read_json(&compare)?;
            let md = render_markdown(&compare_receipt);

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
        } => {
            let export_format = ExportFormat::from_str(&format).ok_or_else(|| {
                anyhow::anyhow!("invalid format: {} (expected csv or jsonl)", format)
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
                    unreachable!("clap prevents both --run and --compare");
                }
            };

            atomic_write(&out, content.as_bytes())?;
            Ok(())
        }

        Command::Promote {
            current,
            to,
            normalize,
            pretty,
        } => {
            let receipt: RunReceipt = read_json(&current)?;

            let result = PromoteUseCase::execute(PromoteRequest { receipt, normalize });

            write_json(&to, &result.receipt, pretty)?;
            Ok(())
        }

        Command::Report {
            compare,
            out,
            md,
            pretty,
        } => {
            let compare_receipt: CompareReceipt = read_json(&compare)?;

            let result = ReportUseCase::execute(ReportRequest {
                compare: compare_receipt.clone(),
            });

            write_json(&out, &result.report, pretty)?;

            // Optionally write markdown summary
            if let Some(md_path) = md {
                let md_content = render_markdown(&compare_receipt);
                if let Some(parent) = md_path.parent() {
                    if !parent.as_os_str().is_empty() {
                        fs::create_dir_all(parent)
                            .with_context(|| format!("create dir {}", parent.display()))?;
                    }
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
            out_dir,
            baseline,
            require_baseline,
            fail_on_warn,
            env,
            output_cap_bytes,
            allow_nonzero,
            host_mismatch,
            pretty,
            mode,
        } => match mode {
            OutputMode::Standard => run_check_standard(
                config,
                bench,
                all,
                out_dir,
                baseline,
                require_baseline,
                fail_on_warn,
                env,
                output_cap_bytes,
                allow_nonzero,
                host_mismatch,
                pretty,
            ),
            OutputMode::Cockpit => run_check_cockpit(
                config,
                bench,
                all,
                out_dir,
                baseline,
                require_baseline,
                fail_on_warn,
                env,
                output_cap_bytes,
                allow_nonzero,
                host_mismatch,
                pretty,
            ),
        },

        Command::Paired {
            name,
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

            let tool = tool_info();
            let runner = StdProcessRunner;
            let host_probe = StdHostProbe;
            let clock = SystemClock;
            let usecase = PairedRunUseCase::new(runner, host_probe, clock, tool);

            let outcome = usecase.execute(PairedRunRequest {
                name,
                cwd,
                baseline_command: baseline_cmd,
                current_command: current_cmd,
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

/// Run check in standard mode (exit codes reflect verdict).
#[allow(clippy::too_many_arguments)]
fn run_check_standard(
    config: PathBuf,
    bench: Option<String>,
    all: bool,
    out_dir: PathBuf,
    baseline: Option<PathBuf>,
    require_baseline: bool,
    fail_on_warn: bool,
    env: Vec<(String, String)>,
    output_cap_bytes: usize,
    allow_nonzero: bool,
    host_mismatch: HostMismatchPolicy,
    pretty: bool,
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
        .map_err(|e| anyhow::anyhow!("config validation: {}", e))?;

    // Determine which benches to run
    let bench_names: Vec<String> = if all {
        if config_file.benches.is_empty() {
            anyhow::bail!("no benchmarks defined in config file");
        }
        config_file.benches.iter().map(|b| b.name.clone()).collect()
    } else if let Some(ref name) = bench {
        vec![name.clone()]
    } else {
        anyhow::bail!("either --bench or --all must be specified");
    };

    // Track aggregate exit code: fail (2) > warn-as-fail (3) > pass (0)
    let mut max_exit_code: i32 = 0;
    let mut all_warnings: Vec<String> = Vec::new();

    for bench_name in &bench_names {
        // For --all mode, use per-bench subdirectories
        let bench_out_dir = if all {
            out_dir.join(bench_name)
        } else {
            out_dir.clone()
        };

        // Resolve baseline path (--baseline flag only valid for single bench mode)
        let baseline_path = resolve_baseline_path(&baseline, bench_name, &config_file);
        let baseline_receipt: Option<RunReceipt> = if let Some(ref path) = baseline_path {
            if path.exists() {
                Some(read_json(path)?)
            } else {
                None
            }
        } else {
            None
        };

        // Create output directory
        fs::create_dir_all(&bench_out_dir)
            .with_context(|| format!("create output dir {}", bench_out_dir.display()))?;

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
            baseline_path,
            require_baseline,
            fail_on_warn,
            tool: tool_info(),
            env: env.clone(),
            output_cap_bytes,
            allow_nonzero,
            host_mismatch_policy: host_mismatch,
        })?;

        // Write artifacts
        write_check_artifacts(&outcome, pretty)?;

        // Collect warnings (prefix with bench name in --all mode)
        for warning in &outcome.warnings {
            if all {
                all_warnings.push(format!("[{}] {}", bench_name, warning));
            } else {
                all_warnings.push(warning.clone());
            }
        }

        // Update aggregate exit code (worst wins)
        // Priority: 2 (fail) > 3 (warn-as-fail) > 0 (pass)
        if outcome.exit_code == 2 {
            max_exit_code = 2;
        } else if outcome.exit_code == 3 && max_exit_code != 2 {
            max_exit_code = 3;
        }
    }

    // Print all warnings
    for warning in &all_warnings {
        eprintln!("warning: {}", warning);
    }

    // Exit with aggregate code
    if max_exit_code != 0 {
        std::process::exit(max_exit_code);
    }

    Ok(())
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
    out_dir: PathBuf,
    baseline: Option<PathBuf>,
    require_baseline: bool,
    fail_on_warn: bool,
    env: Vec<(String, String)>,
    output_cap_bytes: usize,
    allow_nonzero: bool,
    host_mismatch: HostMismatchPolicy,
    pretty: bool,
) -> anyhow::Result<()> {
    let clock = SystemClock;
    let started_at = clock.now_rfc3339();
    let start_instant = Instant::now();

    // Ensure base output directory exists (catastrophic failure if we can't)
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("create output dir {}", out_dir.display()))?;

    // Try to run the check; capture errors
    let result = run_check_cockpit_inner(
        &config,
        &bench,
        all,
        &out_dir,
        &baseline,
        require_baseline,
        fail_on_warn,
        &env,
        output_cap_bytes,
        allow_nonzero,
        host_mismatch,
        pretty,
        &started_at,
        start_instant,
    );

    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            // Error during execution - still try to write an error report
            let ended_at = clock.now_rfc3339();
            let duration_ms = start_instant.elapsed().as_millis() as u64;

            let baseline_available = baseline.as_ref().map(|p| p.exists()).unwrap_or(false);

            let (stage, error_kind) = classify_error(&err);

            let builder = SensorReportBuilder::new(tool_info(), started_at)
                .ended_at(ended_at, duration_ms)
                .baseline(baseline_available, None);

            let error_report = builder.build_error(&format!("{:#}", err), stage, error_kind);

            // Try to write the error report
            let report_path = out_dir.join("report.json");
            if write_json(&report_path, &error_report, pretty).is_ok() {
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
    out_dir: &PathBuf,
    baseline: &Option<PathBuf>,
    require_baseline: bool,
    fail_on_warn: bool,
    env: &[(String, String)],
    output_cap_bytes: usize,
    allow_nonzero: bool,
    host_mismatch: HostMismatchPolicy,
    pretty: bool,
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
        .map_err(|e| anyhow::anyhow!("config validation: {}", e))?;

    // Determine which benches to run
    let bench_names: Vec<String> = if all {
        if config_file.benches.is_empty() {
            anyhow::bail!("no benchmarks defined in config file");
        }
        config_file.benches.iter().map(|b| b.name.clone()).collect()
    } else if let Some(ref name) = bench {
        vec![name.clone()]
    } else {
        anyhow::bail!("either --bench or --all must be specified");
    };

    let multi_bench = bench_names.len() > 1;

    // Collect per-bench outcomes
    let mut all_outcomes: Vec<(String, CheckOutcome, bool)> = Vec::new(); // (bench_name, outcome, baseline_available)

    for bench_name in &bench_names {
        // Create extras directory for native artifacts
        let extras_dir = if multi_bench {
            out_dir.join("extras").join(bench_name)
        } else {
            out_dir.join("extras")
        };
        fs::create_dir_all(&extras_dir)
            .with_context(|| format!("create extras dir {}", extras_dir.display()))?;

        // Resolve baseline path
        let baseline_path = resolve_baseline_path(baseline, bench_name, &config_file);
        let baseline_receipt: Option<RunReceipt> = if let Some(ref path) = baseline_path {
            if path.exists() {
                Some(read_json(path)?)
            } else {
                None
            }
        } else {
            None
        };
        let baseline_available = baseline_receipt.is_some();

        // Execute check
        let runner = StdProcessRunner;
        let host_probe = StdHostProbe;
        let usecase = CheckUseCase::new(runner, host_probe, clock.clone());

        let outcome = usecase.execute(CheckRequest {
            config: config_file.clone(),
            bench_name: bench_name.clone(),
            out_dir: extras_dir.clone(),
            baseline: baseline_receipt,
            baseline_path: baseline_path.clone(),
            require_baseline,
            fail_on_warn,
            tool: tool_info(),
            env: env.to_vec(),
            output_cap_bytes,
            allow_nonzero,
            host_mismatch_policy: host_mismatch,
        })?;

        // Write native artifacts to extras/
        write_check_artifacts(&outcome, pretty)?;

        // Rename extras files to versioned names
        rename_extras_to_versioned(&extras_dir)?;

        all_outcomes.push((bench_name.clone(), outcome, baseline_available));
    }

    // Build sensor report
    let ended_at = clock.now_rfc3339();
    let duration_ms = start_instant.elapsed().as_millis() as u64;

    // Aggregate across all benches
    let any_baseline_available = all_outcomes.iter().any(|(_, _, avail)| *avail);
    let all_baseline_available = all_outcomes.iter().all(|(_, _, avail)| *avail);

    let baseline_reason = if !any_baseline_available {
        Some(BASELINE_REASON_NO_BASELINE.to_string())
    } else {
        None
    };

    let mut builder = SensorReportBuilder::new(tool_info(), started_at.to_string())
        .ended_at(ended_at.clone(), duration_ms)
        .baseline(all_baseline_available, baseline_reason);

    // Aggregate findings, verdict, counts, reasons, and artifacts
    use perfgate_types::{SensorFinding, SensorSeverity, SensorVerdictCounts, SensorVerdictStatus};
    let mut aggregated_findings: Vec<SensorFinding> = Vec::new();
    let mut total_info = 0u32;
    let mut total_warn = 0u32;
    let mut total_error = 0u32;
    let mut worst_status = SensorVerdictStatus::Pass;
    let mut all_reasons: Vec<String> = Vec::new();
    let mut combined_markdown = String::new();

    for (bench_name, outcome, baseline_available) in &all_outcomes {
        // Map findings from this bench's report
        for f in &outcome.report.findings {
            let severity = match f.severity {
                perfgate_types::Severity::Warn => SensorSeverity::Warn,
                perfgate_types::Severity::Fail => SensorSeverity::Error,
            };
            let mut finding_data = f.data.as_ref().and_then(|d| serde_json::to_value(d).ok());
            // In multi-bench mode, include bench name in finding data
            if multi_bench {
                if let Some(ref mut val) = finding_data {
                    if let Some(obj) = val.as_object_mut() {
                        obj.insert(
                            "bench_name".to_string(),
                            serde_json::Value::String(bench_name.clone()),
                        );
                    }
                } else {
                    finding_data = Some(serde_json::json!({ "bench_name": bench_name }));
                }
            }
            let metric_name = f
                .data
                .as_ref()
                .map(|d| d.metric_name.as_str())
                .unwrap_or("");
            let fingerprint = if multi_bench {
                Some(sensor_fingerprint(&[
                    "perfgate",
                    bench_name,
                    &f.check_id,
                    &f.code,
                    metric_name,
                ]))
            } else {
                Some(sensor_fingerprint(&[
                    "perfgate",
                    &f.check_id,
                    &f.code,
                    metric_name,
                ]))
            };
            aggregated_findings.push(SensorFinding {
                check_id: f.check_id.clone(),
                code: f.code.clone(),
                severity,
                message: if multi_bench {
                    format!("[{}] {}", bench_name, f.message)
                } else {
                    f.message.clone()
                },
                fingerprint,
                data: finding_data,
            });
        }

        // Aggregate counts
        total_info += outcome.report.summary.pass_count;
        total_warn += outcome.report.summary.warn_count;
        total_error += outcome.report.summary.fail_count;

        // Aggregate worst verdict status
        match outcome.report.verdict.status {
            perfgate_types::VerdictStatus::Fail => {
                worst_status = SensorVerdictStatus::Fail;
            }
            perfgate_types::VerdictStatus::Warn => {
                if worst_status != SensorVerdictStatus::Fail {
                    worst_status = SensorVerdictStatus::Warn;
                }
            }
            perfgate_types::VerdictStatus::Pass => {}
        }

        // Aggregate reasons (union)
        for reason in &outcome.report.verdict.reasons {
            if !all_reasons.contains(reason) {
                all_reasons.push(reason.clone());
            }
        }

        // Add per-bench artifacts
        let extras_prefix = if multi_bench {
            format!("extras/{}", bench_name)
        } else {
            "extras".to_string()
        };

        builder = builder.artifact(
            format!("{}/perfgate.run.v1.json", extras_prefix),
            "run_receipt".to_string(),
        );
        builder = builder.artifact(
            format!("{}/perfgate.report.v1.json", extras_prefix),
            "perfgate_report".to_string(),
        );

        if outcome.compare_receipt.is_some() {
            builder = builder.artifact(
                format!("{}/perfgate.compare.v1.json", extras_prefix),
                "compare_receipt".to_string(),
            );
        }

        // Collect markdown
        if multi_bench {
            if !combined_markdown.is_empty() {
                combined_markdown.push_str("\n---\n\n");
            }
        }
        combined_markdown.push_str(&outcome.markdown);

        // Baseline reason per bench
        if !baseline_available && !all_reasons.contains(&BASELINE_REASON_NO_BASELINE.to_string()) {
            all_reasons.push(BASELINE_REASON_NO_BASELINE.to_string());
        }
    }

    builder = builder.artifact("comment.md".to_string(), "markdown".to_string());

    // Apply truncation to aggregated findings
    // Truncation invariants:
    // - When applied: findings.len() == findings_emitted + 1
    //   (findings_emitted real findings + 1 truncation meta-finding)
    // - findings_total = original real finding count (before truncation)
    // - When NOT applied: findings_total / findings_emitted are absent from data
    let limit = MAX_FINDINGS_DEFAULT;
    let mut truncation_totals: Option<(usize, usize)> = None;
    if aggregated_findings.len() > limit {
        let total = aggregated_findings.len();
        let shown = limit.saturating_sub(1);
        aggregated_findings.truncate(shown);
        aggregated_findings.push(SensorFinding {
            check_id: CHECK_ID_TOOL_TRUNCATION.to_string(),
            code: FINDING_CODE_TRUNCATED.to_string(),
            severity: SensorSeverity::Info,
            message: format!(
                "Showing {} of {} findings; {} omitted",
                shown,
                total,
                total - shown
            ),
            fingerprint: Some(sensor_fingerprint(&[
                "perfgate",
                CHECK_ID_TOOL_TRUNCATION,
                FINDING_CODE_TRUNCATED,
            ])),
            data: Some(serde_json::json!({
                "total_findings": total,
                "shown_findings": shown,
            })),
        });
        if !all_reasons.contains(&VERDICT_REASON_TRUNCATED.to_string()) {
            all_reasons.push(VERDICT_REASON_TRUNCATED.to_string());
        }
        truncation_totals = Some((total, shown));
    }

    // Build aggregated data section
    let mut data = serde_json::json!({
        "summary": {
            "pass_count": total_info,
            "warn_count": total_warn,
            "fail_count": total_error,
            "total_count": total_info + total_warn + total_error,
        }
    });

    if let Some((total, emitted)) = truncation_totals {
        data["findings_total"] = serde_json::json!(total);
        data["findings_emitted"] = serde_json::json!(emitted);
    }

    // Build the report manually since we have aggregated data
    let sensor_report = perfgate_types::SensorReport {
        schema: perfgate_types::SENSOR_REPORT_SCHEMA_V1.to_string(),
        tool: tool_info(),
        run: perfgate_types::SensorRunMeta {
            started_at: started_at.to_string(),
            ended_at: Some(ended_at),
            duration_ms: Some(duration_ms),
            capabilities: perfgate_types::SensorCapabilities {
                baseline: perfgate_types::Capability {
                    status: if all_baseline_available {
                        perfgate_types::CapabilityStatus::Available
                    } else {
                        perfgate_types::CapabilityStatus::Unavailable
                    },
                    reason: if !any_baseline_available {
                        Some(BASELINE_REASON_NO_BASELINE.to_string())
                    } else {
                        None
                    },
                },
            },
        },
        verdict: perfgate_types::SensorVerdict {
            status: worst_status,
            counts: SensorVerdictCounts {
                info: total_info,
                warn: total_warn,
                error: total_error,
            },
            reasons: all_reasons,
        },
        findings: aggregated_findings,
        artifacts: {
            let mut arts = builder.take_artifacts();
            arts.sort_by(|a, b| (&a.artifact_type, &a.path).cmp(&(&b.artifact_type, &b.path)));
            arts
        },
        data,
    };

    // Write sensor report to out_dir/report.json
    let report_path = out_dir.join("report.json");
    write_json(&report_path, &sensor_report, pretty)?;

    // Write combined markdown to out_dir root
    let md_dest = out_dir.join("comment.md");
    fs::write(&md_dest, &combined_markdown)
        .with_context(|| format!("write {}", md_dest.display()))?;

    // Print warnings but don't affect exit code
    for (_, outcome, _) in &all_outcomes {
        for warning in &outcome.warnings {
            eprintln!("warning: {}", warning);
        }
    }

    // Cockpit mode: always exit 0 if we got here
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
        if old_path.exists() {
            fs::rename(&old_path, &new_path).with_context(|| {
                format!("rename {} -> {}", old_path.display(), new_path.display())
            })?;
        }
    }

    // Clean up stale files that might exist from previous runs
    let stale_files = ["run.json", "compare.json", "report.json"];
    for name in &stale_files {
        let stale = extras_dir.join(name);
        if stale.exists() {
            match fs::remove_file(&stale) {
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
    }

    Ok(())
}

/// Resolve the baseline path from CLI args or config defaults.
fn resolve_baseline_path(
    cli_baseline: &Option<PathBuf>,
    bench_name: &str,
    config: &ConfigFile,
) -> Option<PathBuf> {
    // 1. CLI takes precedence
    if let Some(path) = cli_baseline {
        return Some(path.clone());
    }

    // 2. Fall back to baseline_dir from config defaults
    if let Some(ref baseline_dir) = config.defaults.baseline_dir {
        let path = PathBuf::from(baseline_dir).join(format!("{}.json", bench_name));
        return Some(path);
    }

    // 3. Default convention: baselines/{bench_name}.json
    Some(PathBuf::from("baselines").join(format!("{}.json", bench_name)))
}

/// Write all artifacts from a check outcome.
fn write_check_artifacts(outcome: &CheckOutcome, pretty: bool) -> anyhow::Result<()> {
    // Write run receipt
    write_json(&outcome.run_path, &outcome.run_receipt, pretty)?;

    // Write compare receipt if present
    if let (Some(ref compare), Some(ref path)) = (&outcome.compare_receipt, &outcome.compare_path) {
        write_json(path, compare, pretty)?;
    } else if outcome.compare_receipt.is_none() {
        // Ensure compare.json is absent when no baseline is available.
        if let Some(parent) = outcome.run_path.parent() {
            let stale = parent.join("compare.json");
            match fs::remove_file(&stale) {
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => {
                    return Err(anyhow::anyhow!(
                        "failed to remove stale compare.json {}: {}",
                        stale.display(),
                        err
                    ));
                }
            }
        }
    }

    // Write report (always present for cockpit integration)
    write_json(&outcome.report_path, &outcome.report, pretty)?;

    // Write markdown
    fs::write(&outcome.markdown_path, &outcome.markdown)
        .with_context(|| format!("write {}", outcome.markdown_path.display()))?;

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
        "error" => Ok(HostMismatchPolicy::Error),
        "ignore" => Ok(HostMismatchPolicy::Ignore),
        _ => Err(format!(
            "invalid host mismatch policy: {} (expected warn, error, or ignore)",
            s
        )),
    }
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let v =
        serde_json::from_slice(&bytes).with_context(|| format!("parse json {}", path.display()))?;
    Ok(v)
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T, pretty: bool) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create dir {}", parent.display()))?;
        }
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
    if baseline.stats.cpu_ms.is_some() && current.stats.cpu_ms.is_some() {
        candidates.push(Metric::CpuMs);
    }
    if baseline.stats.max_rss_kb.is_some() && current.stats.max_rss_kb.is_some() {
        candidates.push(Metric::MaxRssKb);
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

fn metric_key(metric: Metric) -> &'static str {
    metric.as_str()
}
