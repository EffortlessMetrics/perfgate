use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use perfgate_adapters::{StdHostProbe, StdProcessRunner};
use perfgate_app::{
    BenchOutcome, CheckOutcome, CheckRequest, CheckUseCase, Clock, CompareRequest, CompareUseCase,
    ExportFormat, ExportUseCase, PairedRunRequest, PairedRunUseCase, PromoteRequest,
    PromoteUseCase, ReportRequest, ReportUseCase, RunBenchRequest, RunBenchUseCase,
    SensorReportBuilder, SystemClock, classify_error, github_annotations, render_markdown,
};
use perfgate_domain::DomainError;
use perfgate_types::{
    BASELINE_REASON_NO_BASELINE, Budget, CompareReceipt, CompareRef, ConfigFile,
    ConfigValidationError, HostMismatchPolicy, Metric, PerfgateError, RunReceipt, ToolInfo,
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
        } => execute_export(run, compare, &format, &out),

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
        .map_err(|e| ConfigValidationError::ConfigFile(e))?;

    // Determine which benches to run
    let bench_names: Vec<String> = if all {
        if config_file.benches.is_empty() {
            anyhow::bail!("no benchmarks defined in config file");
        }
        config_file.benches.iter().map(|b| b.name.clone()).collect()
    } else if let Some(name) = bench {
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
        let baseline_receipt: Option<RunReceipt> = if baseline_path.exists() {
            Some(
                read_json(&baseline_path)
                    .map_err(|e| PerfgateError::BaselineResolve(format!("{:#}", e)))?,
            )
        } else {
            None
        };

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
        })?;

        // Write artifacts
        write_check_artifacts(&outcome, pretty)
            .map_err(|e| PerfgateError::ArtifactWrite(format!("{:#}", e)))?;

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
        update_max_exit_code(&mut max_exit_code, outcome.exit_code);
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
        .map_err(|e| ConfigValidationError::ConfigFile(e))?;

    // Determine which benches to run
    let bench_names: Vec<String> = if all {
        if config_file.benches.is_empty() {
            anyhow::bail!("no benchmarks defined in config file");
        }
        config_file.benches.iter().map(|b| b.name.clone()).collect()
    } else if let Some(name) = bench {
        vec![name.clone()]
    } else {
        anyhow::bail!("either --bench or --all must be specified");
    };

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
            let baseline_receipt: Option<RunReceipt> = if baseline_path.exists() {
                Some(
                    read_json(&baseline_path)
                        .map_err(|e| PerfgateError::BaselineResolve(format!("{:#}", e)))?,
                )
            } else {
                None
            };
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
            })?;

            // Write native artifacts to extras/
            write_check_artifacts(&check_outcome, pretty)
                .map_err(|e| PerfgateError::ArtifactWrite(format!("{:#}", e)))?;

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
                markdown: check_outcome.markdown,
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

    // 2. Fall back to baseline_dir from config defaults
    if let Some(baseline_dir) = &config.defaults.baseline_dir {
        return PathBuf::from(baseline_dir).join(format!("{}.json", bench_name));
    }

    // 3. Default convention: baselines/{bench_name}.json
    PathBuf::from("baselines").join(format!("{}.json", bench_name))
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
    let export_format = ExportFormat::from_str(format)
        .ok_or_else(|| anyhow::anyhow!("invalid format: {} (expected csv or jsonl)", format))?;

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
            max_rss_kb: rss.then_some(U64Summary {
                median: 2048,
                min: 1024,
                max: 4096,
            }),
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
            max_rss_kb: None,
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
    fn resolve_baseline_path_prefers_cli_then_config_then_default() {
        let config = ConfigFile {
            defaults: DefaultsConfig {
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
            PathBuf::from("bases").join("bench.json")
        );

        let config_no_default = ConfigFile::default();
        assert_eq!(
            resolve_baseline_path(&no_cli, "bench", &config_no_default),
            PathBuf::from("baselines").join("bench.json")
        );
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
                out_dir,
                Some(baseline_path),
                false,
                false,
                Vec::new(),
                8192,
                false,
                HostMismatchPolicy::Warn,
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
            out_dir,
            None,
            false,
            false,
            Vec::new(),
            8192,
            false,
            HostMismatchPolicy::Warn,
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
