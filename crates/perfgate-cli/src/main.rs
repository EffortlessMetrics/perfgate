use anyhow::Context;
use clap::{Parser, Subcommand};
use perfgate_adapters::StdProcessRunner;
use perfgate_app::{
    github_annotations, render_markdown, CompareRequest, CompareUseCase, RunBenchRequest,
    RunBenchUseCase, SystemClock,
};
use perfgate_domain::DomainError;
use perfgate_types::{Budget, CompareRef, Metric, RunReceipt, ToolInfo};
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
            out,
            pretty,
            command,
        } => {
            let timeout = timeout.as_deref().map(parse_duration).transpose()?;

            let tool = tool_info();
            let runner = StdProcessRunner;
            let clock = SystemClock;
            let usecase = RunBenchUseCase::new(runner, clock, tool);

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
    }
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
