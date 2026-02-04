use anyhow::Context;
use clap::{Parser, Subcommand};
use perfgate_adapters::{StdHostProbe, StdProcessRunner};
use perfgate_app::{
    github_annotations, render_markdown, CheckOutcome, CheckRequest, CheckUseCase, CompareRequest,
    CompareUseCase, ExportFormat, ExportUseCase, PromoteRequest, PromoteUseCase, ReportRequest,
    ReportUseCase, RunBenchRequest, RunBenchUseCase, SystemClock,
};
use perfgate_domain::DomainError;
use perfgate_types::{
    Budget, CompareReceipt, CompareRef, ConfigFile, Metric, RunReceipt, ToolInfo,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

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
        #[arg(long)]
        bench: String,

        /// Output directory for artifacts
        #[arg(long, default_value = "artifacts/perfgate")]
        out_dir: PathBuf,

        /// Path to the baseline file. If not specified, looks in baseline_dir/{bench}.json
        #[arg(long)]
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

            let compare = CompareUseCase::execute(CompareRequest {
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
            })
            .map_err(map_domain_err)?;

            write_json(&out, &compare, pretty)?;

            match compare.verdict.status {
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
            out_dir,
            baseline,
            require_baseline,
            fail_on_warn,
            env,
            output_cap_bytes,
            allow_nonzero,
            pretty,
        } => {
            // Load config file
            let config_content = fs::read_to_string(&config)
                .with_context(|| format!("read {}", config.display()))?;

            let config_file: ConfigFile =
                if config.extension().map(|e| e == "json").unwrap_or(false) {
                    serde_json::from_str(&config_content)
                        .with_context(|| format!("parse JSON config {}", config.display()))?
                } else {
                    toml::from_str(&config_content)
                        .with_context(|| format!("parse TOML config {}", config.display()))?
                };

            // Resolve baseline path
            let baseline_path = resolve_baseline_path(&baseline, &bench, &config_file);
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
            fs::create_dir_all(&out_dir)
                .with_context(|| format!("create output dir {}", out_dir.display()))?;

            // Execute check
            let runner = StdProcessRunner;
            let host_probe = StdHostProbe;
            let clock = SystemClock;
            let usecase = CheckUseCase::new(runner, host_probe, clock);

            let outcome = usecase.execute(CheckRequest {
                config: config_file,
                bench_name: bench,
                out_dir: out_dir.clone(),
                baseline: baseline_receipt,
                baseline_path,
                require_baseline,
                fail_on_warn,
                tool: tool_info(),
                env,
                output_cap_bytes,
                allow_nonzero,
            })?;

            // Write artifacts
            write_check_artifacts(&outcome, pretty)?;

            // Print warnings
            for warning in &outcome.warnings {
                eprintln!("warning: {}", warning);
            }

            // Exit with appropriate code
            if outcome.exit_code != 0 {
                std::process::exit(outcome.exit_code);
            }

            Ok(())
        }
    }
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
    match metric {
        Metric::WallMs => "wall_ms",
        Metric::MaxRssKb => "max_rss_kb",
        Metric::ThroughputPerS => "throughput_per_s",
    }
}
