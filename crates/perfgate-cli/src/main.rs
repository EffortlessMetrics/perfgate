//! perfgate CLI - entry point for all workflows.

use anyhow::Context;
use clap::{Args, Parser, Subcommand, ValueEnum};
use glob::glob;
use object_store::{ObjectStore, path::Path as ObjectPath};
use perfgate_adapters::{StdHostProbe, StdProcessRunner};
use perfgate_app::baseline_resolve::{is_remote_storage_uri, resolve_baseline_path};
use perfgate_app::comparison_logic::{build_budgets, build_metric_statistics, verdict_from_counts};
use perfgate_app::{
    BenchOutcome, BisectRequest, BisectUseCase, BlameRequest, BlameUseCase, CheckOutcome,
    CheckRequest, CheckUseCase, Clock, CompareRequest, CompareUseCase, ExplainRequest,
    ExplainUseCase, ExportFormat, ExportUseCase, PairedRunRequest, PairedRunUseCase,
    PromoteRequest, PromoteUseCase, ReportRequest, ReportUseCase, RunBenchRequest, RunBenchUseCase,
    SensorReportBuilder, SystemClock, classify_error, github_annotations, render_markdown,
    render_markdown_template,
};
use perfgate_client::{
    ListBaselinesQuery, ListVerdictsQuery, SubmitVerdictRequest, UploadBaselineRequest,
};
use perfgate_config::{ResolvedServerConfig, load_config_file, resolve_server_config};
use perfgate_domain::{DependencyChangeType, DomainError, SignificancePolicy};
use perfgate_error::{ConfigValidationError, IoError, PerfgateError};
use perfgate_summary::{SummaryRequest, SummaryUseCase};
use perfgate_types::{
    BASELINE_REASON_NO_BASELINE, BaselineServerConfig, CompareReceipt, CompareRef, ConfigFile,
    HostMismatchPolicy, RunReceipt, SensorVerdictStatus, ToolInfo, VerdictStatus,
};
use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};
use url::Url;

const BASELINE_SERVER_NOT_CONFIGURED: &str = "baseline server is not configured; set `--baseline-server`, `PERFGATE_SERVER_URL`, or `[baseline_server].url` in `perfgate.toml`";
const DEFAULT_FALLBACK_BASELINE_DIR: &str = "baselines";

/// Output mode for the check command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum OutputMode {
    /// Standard mode: exit codes reflect verdict (0=pass, 2=fail, 3=warn with --fail-on-warn)
    #[default]
    Standard,
    /// Cockpit mode: always write receipt, exit 0 unless catastrophic failure
    Cockpit,
}

/// Global flags for baseline server connection.
#[derive(Debug, Clone, Args, Default)]
#[command(next_help_heading = "Global Options")]
pub struct ServerFlags {
    /// URL of the baseline server (e.g., http://localhost:3000/api/v1)
    /// Can also be set via PERFGATE_SERVER_URL environment variable.
    #[arg(long, global = true)]
    pub baseline_server: Option<String>,

    /// API key for authentication with the baseline server.
    /// Can also be set via PERFGATE_API_KEY environment variable.
    #[arg(long, global = true)]
    pub api_key: Option<String>,

    /// Project name for multi-tenancy.
    /// Can also be set via PERFGATE_PROJECT environment variable.
    #[arg(long, global = true)]
    pub project: Option<String>,
}

impl ServerFlags {
    /// Resolves server configuration from CLI flags, environment variables, and config file.
    pub fn resolve(&self, config: &BaselineServerConfig) -> ResolvedServerConfig {
        resolve_server_config(
            self.baseline_server.clone(),
            self.api_key.clone(),
            self.project.clone(),
            config,
        )
    }
}

enum BaselineSelector {
    Local(PathBuf),
    Server { benchmark: String },
}

fn parse_baseline_selector(
    baseline: &str,
    server_config: &ResolvedServerConfig,
) -> anyhow::Result<BaselineSelector> {
    if let Some(server_ref) = baseline.strip_prefix("@server:") {
        if server_ref.is_empty() {
            anyhow::bail!("--baseline requires a benchmark name after @server:");
        }

        if !server_config.is_configured() {
            return Err(anyhow::anyhow!(BASELINE_SERVER_NOT_CONFIGURED));
        }

        return Ok(BaselineSelector::Server {
            benchmark: server_ref.to_string(),
        });
    }

    let path = Path::new(baseline);
    if !server_config.is_configured()
        || path.exists()
        || baseline.contains(std::path::MAIN_SEPARATOR)
        || baseline.contains('/')
        || baseline.contains('\\')
        || baseline.ends_with(".json")
    {
        return Ok(BaselineSelector::Local(path.to_path_buf()));
    }

    Ok(BaselineSelector::Server {
        benchmark: baseline.to_string(),
    })
}

#[derive(Debug, Parser)]
#[command(
    name = "perfgate",
    version,
    about = "Perf budgets and baseline diffs for CI / PR bots"
)]
struct Cli {
    #[command(flatten)]
    server: ServerFlags,

    #[command(subcommand)]
    cmd: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run a command repeatedly and emit a run receipt (JSON).
    Run(Box<RunArgs>),

    /// Compare a current receipt against a baseline and emit a compare receipt (JSON).
    Compare(Box<CompareArgs>),

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

    /// Export a run or compare receipt to CSV, JSONL, HTML, Prometheus, or JUnit format.
    Export {
        /// Path to a run receipt (mutually exclusive with --compare)
        #[arg(long, conflicts_with = "compare")]
        run: Option<PathBuf>,

        /// Path to a compare receipt (mutually exclusive with --run)
        #[arg(long, conflicts_with = "run")]
        compare: Option<PathBuf>,

        /// Output format: csv, jsonl, html, prometheus, or junit
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
    Promote(Box<PromoteArgs>),

    /// Generate a cockpit-compatible report from a compare receipt.
    ///
    /// Wraps a CompareReceipt into a `perfgate.report.v1` envelope with
    /// verdict, findings, and summary counts.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Report(Box<ReportArgs>),

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
    Check(Box<CheckArgs>),

    /// Run paired benchmark: interleave baseline and current commands for reduced noise.
    ///
    /// Executes baseline-1, current-1, baseline-2, current-2, etc. to minimize
    /// environmental variation between measurements.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Paired(Box<PairedArgs>),

    /// Manage baselines on the baseline server.
    Baseline {
        #[command(subcommand)]
        action: BaselineAction,
    },

    /// Summarize one or more compare receipts in a terminal table.
    Summary {
        /// Paths to compare receipts (glob patterns supported)
        #[arg(required = true, num_args = 1..)]
        files: Vec<String>,

        /// If true, do not exit with a non-zero status code when a fail verdict is encountered
        #[arg(long)]
        allow_nonzero: bool,
    },

    /// Aggregate multiple run receipts (e.g. from a fleet) into a single run receipt.
    Aggregate {
        /// Paths to run receipts (glob patterns supported)
        #[arg(required = true, num_args = 1..)]
        files: Vec<String>,

        /// Output file path
        #[arg(long, default_value = "perfgate-aggregated.json")]
        out: PathBuf,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,
    },

    /// Automatically find the commit that introduced a performance regression.
    ///
    /// This is a wrapper around `git bisect` that uses `perfgate paired`
    /// to determine if a commit is good or bad.
    Bisect(Box<BisectArgs>),

    /// Analyze changes in Cargo.lock to identify dependency updates causing binary size regressions.
    Blame(Box<BlameArgs>),

    /// Provide AI-ready prompts and automated playbooks for diagnosing performance regressions.
    Explain {
        /// Path to a compare receipt
        #[arg(long)]
        compare: PathBuf,

        /// Path to baseline Cargo.lock for binary blame analysis
        #[arg(long)]
        baseline_lock: Option<PathBuf>,

        /// Path to current Cargo.lock for binary blame analysis
        #[arg(long)]
        current_lock: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
pub struct BlameArgs {
    /// Path to baseline Cargo.lock
    #[arg(long)]
    pub baseline: PathBuf,

    /// Path to current Cargo.lock
    #[arg(long)]
    pub current: PathBuf,

    /// Output format (text|json)
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Debug, Args)]
pub struct BisectArgs {
    /// The known good commit
    #[arg(long)]
    pub good: String,

    /// The known bad commit
    #[arg(long, default_value = "HEAD")]
    pub bad: String,

    /// Shell command to build the project
    #[arg(long, default_value = "cargo build --release")]
    pub build_cmd: String,

    /// Path to the executable to benchmark
    #[arg(long)]
    pub executable: PathBuf,

    /// Fail the command if a regression exceeds this percentage (e.g., 5.0 for 5%).
    #[arg(long, default_value = "5.0")]
    pub threshold: f64,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    /// Bench identifier (used for baselines and reporting)
    #[arg(long)]
    pub name: String,

    /// Number of measured samples
    #[arg(long, default_value_t = 5)]
    pub repeat: u32,

    /// Warmup samples (excluded from stats)
    #[arg(long, default_value_t = 0)]
    pub warmup: u32,

    /// Units of work completed per run (enables throughput_per_s)
    #[arg(long)]
    pub work: Option<u64>,

    /// Working directory
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Per-run timeout (e.g. "2s")
    #[arg(long)]
    pub timeout: Option<String>,

    /// Environment variable (KEY=VALUE). Repeatable.
    #[arg(long, value_parser = parse_key_val_string)]
    pub env: Vec<(String, String)>,

    /// Max bytes captured from stdout/stderr per run
    #[arg(long, default_value_t = 8192)]
    pub output_cap_bytes: usize,

    /// Do not fail the tool when the command returns nonzero.
    #[arg(long, default_value_t = false)]
    pub allow_nonzero: bool,

    /// Include a hashed hostname in the host fingerprint for noise mitigation.
    #[arg(long, default_value_t = false)]
    pub include_hostname_hash: bool,

    /// Output file path
    #[arg(long, default_value = "perfgate.json")]
    pub out: PathBuf,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,

    /// Upload the run result to the baseline server.
    #[arg(long, default_value_t = false)]
    pub upload: bool,

    /// Project name for upload (overrides global --project flag).
    #[arg(long)]
    pub upload_project: Option<String>,

    /// Command to run (argv) after `--`
    #[arg(last = true, required = true)]
    pub command: Vec<String>,
}

#[derive(Debug, Args)]
pub struct CompareArgs {
    /// Path to baseline receipt, or "@server:benchmark_name" to fetch from server.
    #[arg(long)]
    pub baseline: String,

    #[arg(long)]
    pub current: PathBuf,

    /// Global regression threshold (0.20 = 20%)
    #[arg(long, default_value_t = 0.20)]
    pub threshold: f64,

    /// Global warn factor (warn_threshold = threshold * warn_factor)
    #[arg(long, default_value_t = 0.90)]
    pub warn_factor: f64,

    /// Global noise threshold (coefficient of variation).
    /// If CV exceeds this, the metric is considered flaky/noisy.
    #[arg(long)]
    pub noise_threshold: Option<f64>,

    /// Global noise policy (warn|skip|ignore)
    #[arg(long, value_parser = parse_noise_policy)]
    pub noise_policy: Option<perfgate_types::NoisePolicy>,

    /// Override per-metric threshold, e.g. wall_ms=0.10
    #[arg(long, value_parser = parse_key_val_f64)]
    pub metric_threshold: Vec<(String, f64)>,

    /// Override per-metric noise threshold, e.g. wall_ms=0.05
    #[arg(long, value_parser = parse_key_val_f64)]
    pub metric_noise_threshold: Vec<(String, f64)>,

    /// Override per-metric direction, e.g. throughput_per_s=higher
    #[arg(long, value_parser = parse_key_val_string)]
    pub direction: Vec<(String, String)>,

    /// Override per-metric statistic, e.g. wall_ms=p95
    #[arg(long, value_parser = parse_key_val_string)]
    pub metric_stat: Vec<(String, String)>,

    /// Compute per-metric significance metadata using Welch's t-test (p <= alpha).
    #[arg(long, value_parser = parse_significance_alpha)]
    pub significance_alpha: Option<f64>,

    /// Minimum samples required in each run before significance is computed.
    #[arg(long, default_value_t = 8)]
    pub significance_min_samples: u32,

    /// When set with --significance-alpha, warn/fail statuses require significance.
    #[arg(long, default_value_t = false)]
    pub require_significance: bool,

    /// Treat WARN verdict as a failing exit code
    #[arg(long, default_value_t = false)]
    pub fail_on_warn: bool,

    /// Policy for handling host mismatches between baseline and current runs.
    #[arg(long, default_value = "warn", value_parser = parse_host_mismatch_policy)]
    pub host_mismatch: HostMismatchPolicy,

    /// Output compare receipt
    #[arg(long, default_value = "perfgate-compare.json")]
    pub out: PathBuf,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct PromoteArgs {
    /// Path or cloud URI to the current run receipt to promote.
    #[arg(long)]
    pub current: PathBuf,

    /// Path or cloud URI where the baseline should be written.
    #[arg(long, conflicts_with = "to_server")]
    pub to: Option<PathBuf>,

    /// Promote to the baseline server instead of a local file.
    #[arg(long, conflicts_with = "to")]
    pub to_server: bool,

    /// Benchmark name for server promotion (required with --to-server).
    #[arg(long, requires = "to_server")]
    pub benchmark: Option<String>,

    /// Project name for server promotion (overrides global --project flag).
    #[arg(long, requires = "to_server")]
    pub promote_project: Option<String>,

    /// Version identifier for the promoted baseline (server only).
    #[arg(long, requires = "to_server")]
    pub version: Option<String>,

    /// Strip run-specific fields (run_id, timestamps) for stable baselines
    #[arg(long, default_value_t = false)]
    pub normalize: bool,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct ReportArgs {
    /// Path to the compare receipt
    #[arg(long)]
    pub compare: PathBuf,

    /// Output report JSON path
    #[arg(long, default_value = "perfgate-report.json")]
    pub out: PathBuf,

    /// Also write markdown summary to this path
    #[arg(long)]
    pub md: Option<PathBuf>,

    /// Render markdown with a Handlebars template file (requires --md).
    #[arg(long, requires = "md")]
    pub md_template: Option<PathBuf>,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Path to the config file (TOML or JSON)
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Name of the benchmark to run (must match a [[bench]] in config)
    #[arg(long, conflicts_with = "all")]
    pub bench: Option<String>,

    /// Run all benchmarks defined in the config file
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// Regex to filter benchmark names when used with --all
    #[arg(long, requires = "all")]
    pub bench_regex: Option<String>,

    /// Output directory for artifacts
    #[arg(long, default_value = "artifacts/perfgate")]
    pub out_dir: PathBuf,

    /// Path or cloud URI to the baseline file.
    #[arg(long, conflicts_with = "all")]
    pub baseline: Option<PathBuf>,

    /// Fail if baseline is missing (default: warn and continue)
    #[arg(long, default_value_t = false)]
    pub require_baseline: bool,

    /// Treat WARN verdict as a failing exit code
    #[arg(long, default_value_t = false)]
    pub fail_on_warn: bool,

    /// Global noise threshold (coefficient of variation).
    #[arg(long)]
    pub noise_threshold: Option<f64>,

    /// Global noise policy (warn|skip|ignore)
    #[arg(long, value_parser = parse_noise_policy)]
    pub noise_policy: Option<perfgate_types::NoisePolicy>,

    /// Environment variable (KEY=VALUE). Repeatable.
    #[arg(long, value_parser = parse_key_val_string)]
    pub env: Vec<(String, String)>,

    /// Max bytes captured from stdout/stderr per run
    #[arg(long, default_value_t = 8192)]
    pub output_cap_bytes: usize,

    /// Do not fail the tool when the command returns nonzero.
    #[arg(long, default_value_t = false)]
    pub allow_nonzero: bool,

    /// Policy for handling host mismatches between baseline and current runs.
    #[arg(long, default_value = "warn", value_parser = parse_host_mismatch_policy)]
    pub host_mismatch: HostMismatchPolicy,

    /// Compute per-metric significance metadata using Welch's t-test (p <= alpha).
    #[arg(long, value_parser = parse_significance_alpha)]
    pub significance_alpha: Option<f64>,

    /// Minimum samples required in each run before significance is computed.
    #[arg(long, default_value_t = 8)]
    pub significance_min_samples: u32,

    /// When set with --significance-alpha, warn/fail statuses require significance.
    #[arg(long, default_value_t = false)]
    pub require_significance: bool,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,

    /// Output mode (standard or cockpit).
    #[arg(long, default_value = "standard", value_enum)]
    pub mode: OutputMode,

    /// Render markdown using a Handlebars template file.
    #[arg(long)]
    pub md_template: Option<PathBuf>,

    /// Write GitHub Actions step outputs (verdict/counts) to $GITHUB_OUTPUT.
    #[arg(long, default_value_t = false)]
    pub output_github: bool,
}

#[derive(Debug, Args)]
pub struct PairedArgs {
    /// Bench identifier (used for baselines and reporting)
    #[arg(long)]
    pub name: String,

    /// Baseline command as a shell string (parsed using shell-words)
    #[arg(long, conflicts_with = "baseline_cmd")]
    pub baseline: Option<String>,

    /// Current command as a shell string (parsed using shell-words)
    #[arg(long, conflicts_with = "current_cmd")]
    pub current: Option<String>,

    /// Baseline command.
    #[arg(long, num_args = 1.., conflicts_with = "baseline")]
    pub baseline_cmd: Option<Vec<String>>,

    /// Current command.
    #[arg(long, num_args = 1.., conflicts_with = "current")]
    pub current_cmd: Option<Vec<String>>,

    /// Number of measured pairs
    #[arg(long, default_value_t = 5)]
    pub repeat: u32,

    /// Warmup pairs (excluded from stats)
    #[arg(long, default_value_t = 0)]
    pub warmup: u32,

    /// Units of work completed per run (enables throughput_per_s)
    #[arg(long)]
    pub work: Option<u64>,

    /// Working directory
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Per-run timeout (e.g. "2s")
    #[arg(long)]
    pub timeout: Option<String>,

    /// Environment variable (KEY=VALUE). Repeatable.
    #[arg(long, value_parser = parse_key_val_string)]
    pub env: Vec<(String, String)>,

    /// Max bytes captured from stdout/stderr per run
    #[arg(long, default_value_t = 8192)]
    pub output_cap_bytes: usize,

    /// Do not fail the tool when the command returns nonzero.
    #[arg(long, default_value_t = false)]
    pub allow_nonzero: bool,

    /// Include a hashed hostname in the host fingerprint for noise mitigation.
    #[arg(long, default_value_t = false)]
    pub include_hostname_hash: bool,

    /// Require statistical significance for wall time difference.
    #[arg(long, default_value_t = false)]
    pub require_significance: bool,

    /// Statistical significance level (alpha).
    #[arg(long)]
    pub significance_alpha: Option<f64>,

    /// Minimum samples required for significance testing.
    #[arg(long)]
    pub significance_min_samples: Option<u32>,

    /// Maximum number of additional pairs to run if significance is not reached.
    #[arg(long, default_value_t = 0)]
    pub max_retries: u32,

    /// Fail the command (exit code 2) if a regression exceeds this percentage (e.g., 5.0 for 5%).
    #[arg(long)]
    pub fail_on_regression: Option<f64>,

    /// Output file path
    #[arg(long, default_value = "perfgate-paired.json")]
    pub out: PathBuf,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

/// Subcommands for baseline management.
#[derive(Debug, Subcommand)]
enum BaselineAction {
    /// List baselines for a project.
    List {
        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Filter by benchmark name prefix
        #[arg(long)]
        prefix: Option<String>,

        /// Maximum number of results (default: 50, max: 200)
        #[arg(long, default_value_t = 50)]
        limit: u32,

        /// Include full receipts in output
        #[arg(long, default_value_t = false)]
        include_receipts: bool,
    },

    /// Download a baseline from the server.
    Download {
        /// Benchmark name to download
        #[arg(long)]
        benchmark: String,

        /// Output file path
        #[arg(long)]
        output: PathBuf,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Specific version to download (default: latest)
        #[arg(long)]
        version: Option<String>,
    },

    /// Upload a baseline to the server.
    Upload {
        /// Path to the run receipt file
        #[arg(long)]
        file: PathBuf,

        /// Benchmark name (uses the name from the receipt if not specified)
        #[arg(long)]
        benchmark: Option<String>,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Version identifier for the baseline
        #[arg(long)]
        version: Option<String>,

        /// Normalize the receipt before uploading (strip run_id, timestamps)
        #[arg(long, default_value_t = false)]
        normalize: bool,
    },

    /// Delete a baseline from the server.
    Delete {
        /// Benchmark name to delete
        #[arg(long)]
        benchmark: String,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Specific version to delete (default: latest)
        #[arg(long)]
        version: Option<String>,

        /// Confirm deletion without prompting
        #[arg(long, default_value_t = false)]
        force: bool,
    },

    /// Show version history for a baseline.
    History {
        /// Benchmark name
        #[arg(long)]
        benchmark: String,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Maximum number of versions to show
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },

    /// Show execution verdict history.
    Verdicts {
        /// Optional benchmark name to filter by
        #[arg(long)]
        benchmark: Option<String>,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Maximum number of results
        #[arg(long, default_value_t = 50)]
        limit: u32,

        /// Optional status to filter by (pass|warn|fail|skip)
        #[arg(long, value_parser = parse_verdict_status)]
        status: Option<VerdictStatus>,
    },

    /// Submit a benchmark verdict to the server.
    SubmitVerdict {
        /// Path to a compare receipt
        #[arg(long)]
        compare: PathBuf,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Git reference (e.g. branch name or tag)
        #[arg(long)]
        git_ref: Option<String>,

        /// Git commit SHA
        #[arg(long)]
        git_sha: Option<String>,
    },

    /// Migrate local baselines to the server.
    Migrate {
        /// Directory containing baseline JSON files
        #[arg(long, default_value = "baselines")]
        dir: PathBuf,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Recursively search for JSON files
        #[arg(long, default_value_t = false)]
        recursive: bool,

        /// Do not actually upload, just show what would be done
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
}

fn render_markdown_with_optional_template(
    compare: &CompareReceipt,
    template_path: Option<&Path>,
) -> anyhow::Result<String> {
    if let Some(path) = template_path {
        let template = fs::read_to_string(path)
            .with_context(|| format!("read template {}", path.display()))?;
        render_markdown_template(compare, &template)
    } else {
        Ok(render_markdown(compare))
    }
}

fn resolve_server_config_from_path(
    flags: &ServerFlags,
    config_path: Option<&Path>,
) -> anyhow::Result<(ResolvedServerConfig, ConfigFile)> {
    let path = config_path.unwrap_or_else(|| Path::new("perfgate.toml"));
    let config_file = load_config_file(path)?;
    let resolved = flags.resolve(&config_file.baseline_server);
    Ok((resolved, config_file))
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

fn main() -> ExitCode {
    match real_main() {
        Ok(_) => ExitCode::from(0),
        Err(err) => {
            if let Some(clap_err) = err.downcast_ref::<clap::Error>() {
                clap_err.exit();
            }
            eprintln!("error: {:#}", err);
            ExitCode::from(1)
        }
    }
}

fn real_main() -> anyhow::Result<()> {
    let cli = Cli::try_parse()?;
    run_command(cli.cmd, cli.server)
}

fn run_command(cmd: Command, server_flags: ServerFlags) -> anyhow::Result<()> {
    match cmd {
        Command::Run(args) => {
            let RunArgs {
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
                upload,
                upload_project,
                command,
            } = *args;

            let timeout = timeout.as_deref().map(parse_duration).transpose()?;

            let tool = tool_info();
            let runner = StdProcessRunner;
            let host_probe = StdHostProbe;
            let clock = SystemClock;
            let usecase = RunBenchUseCase::new(runner, host_probe, clock, tool);

            let outcome = usecase.execute(RunBenchRequest {
                name: name.clone(),
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

            // Upload to server if requested
            if upload {
                let (server_config, _config_file) =
                    resolve_server_config_from_path(&server_flags, None)?;
                let client = server_config.require_fallback_client(
                    Some(Path::new(DEFAULT_FALLBACK_BASELINE_DIR)),
                    BASELINE_SERVER_NOT_CONFIGURED,
                )?;
                let project = server_config.resolve_project(upload_project)?;

                let request = UploadBaselineRequest {
                    benchmark: name,
                    version: None,
                    git_ref: None,
                    git_sha: None,
                    receipt: outcome.receipt.clone(),
                    metadata: BTreeMap::new(),
                    tags: Vec::new(),
                    normalize: false,
                };

                with_tokio_runtime(async {
                    let response: perfgate_client::types::UploadBaselineResponse = client
                        .upload_baseline(&project, &request)
                        .await
                        .with_context(|| {
                            format!("Failed to upload baseline to server (project: {})", project)
                        })?;
                    eprintln!(
                        "Uploaded baseline {} version {} to server",
                        response.benchmark, response.version
                    );
                    Ok::<(), anyhow::Error>(())
                })?;
            }

            if outcome.failed && !allow_nonzero {
                // Measurement did complete, but the target command misbehaved.
                // Exit 1 to signal failure while still leaving a receipt artifact.
                anyhow::bail!("benchmark command failed: {}", outcome.reasons.join(", "));
            }

            Ok(())
        }

        Command::Compare(args) => {
            let CompareArgs {
                baseline,
                current,
                threshold,
                warn_factor,
                noise_threshold,
                noise_policy,
                metric_threshold,
                metric_noise_threshold,
                direction,
                metric_stat,
                significance_alpha,
                significance_min_samples,
                require_significance,
                fail_on_warn,
                host_mismatch,
                out,
                pretty,
            } = *args;

            let (server_config, config_file) =
                resolve_server_config_from_path(&server_flags, None)?;
            let baseline_selector = parse_baseline_selector(&baseline, &server_config)?;
            let (baseline_receipt, baseline_ref) = match baseline_selector {
                BaselineSelector::Server { benchmark } => {
                    let client = server_config.require_fallback_client(
                        Some(Path::new(DEFAULT_FALLBACK_BASELINE_DIR)),
                        BASELINE_SERVER_NOT_CONFIGURED,
                    )?;
                    let project = server_config.resolve_project(None)?;
                    let record = with_tokio_runtime(async {
                        let record: perfgate_api::BaselineRecord = client
                            .get_latest_baseline(&project, &benchmark)
                            .await
                            .with_context(|| {
                                format!("Failed to fetch baseline '{benchmark}' from server")
                            })?;
                        Ok::<perfgate_api::BaselineRecord, anyhow::Error>(record)
                    })?;

                    let receipt = record.receipt;
                    let ref_info = CompareRef {
                        path: Some(format!("@server:{benchmark}")),
                        run_id: Some(receipt.run.id.clone()),
                    };
                    (receipt, ref_info)
                }
                BaselineSelector::Local(path) => {
                    let receipt: RunReceipt = read_json_from_location(&path)?;
                    let ref_info = CompareRef {
                        path: Some(path.display().to_string()),
                        run_id: Some(receipt.run.id.clone()),
                    };
                    (receipt, ref_info)
                }
            };

            let current_receipt: RunReceipt = read_json_from_location(&current)?;

            let budgets = build_budgets(
                &baseline_receipt,
                &current_receipt,
                threshold,
                warn_factor,
                noise_threshold,
                noise_policy,
                metric_threshold,
                metric_noise_threshold,
                direction,
            )?;

            let metric_statistics = build_metric_statistics(&budgets, metric_stat)?;

            let significance = significance_alpha
                .map(|alpha| {
                    SignificancePolicy::new(
                        alpha,
                        significance_min_samples as usize,
                        require_significance,
                    )
                })
                .transpose()?;

            let compare_result = CompareUseCase::execute(CompareRequest {
                baseline: baseline_receipt.clone(),
                current: current_receipt.clone(),
                budgets,
                metric_statistics,
                significance,
                baseline_ref,
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

            // Submit verdict to server if configured
            submit_verdict_if_possible(&server_flags, &config_file, &compare_result.receipt);

            write_json(&out, &compare_result.receipt, pretty)?;

            match compare_result.receipt.verdict.status {
                perfgate_types::VerdictStatus::Pass | perfgate_types::VerdictStatus::Skip => Ok(()),
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
            let md = render_markdown_with_optional_template(&compare_receipt, template.as_deref())?;

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

        Command::Promote(args) => {
            let PromoteArgs {
                current,
                to,
                to_server,
                benchmark,
                promote_project,
                version,
                normalize,
                pretty,
            } = *args;

            let receipt: RunReceipt = read_json_from_location(&current)?;

            if to_server {
                let (server_config, _config_file) =
                    resolve_server_config_from_path(&server_flags, None)?;
                // Promote to server
                let client = server_config.require_fallback_client(
                    Some(Path::new(DEFAULT_FALLBACK_BASELINE_DIR)),
                    BASELINE_SERVER_NOT_CONFIGURED,
                )?;
                let project = server_config.resolve_project(promote_project)?;

                let benchmark_name = benchmark.ok_or_else(|| {
                    anyhow::anyhow!("--to-server requires --benchmark to be specified")
                })?;

                let request = perfgate_client::types::UploadBaselineRequest {
                    benchmark: benchmark_name.clone(),
                    version,
                    git_ref: None,
                    git_sha: None,
                    receipt: receipt.clone(),
                    metadata: BTreeMap::new(),
                    tags: Vec::new(),
                    normalize,
                };

                with_tokio_runtime(async {
                    let response: perfgate_client::types::UploadBaselineResponse = client
                        .upload_baseline(&project, &request)
                        .await
                        .with_context(|| {
                            format!(
                                "Failed to promote baseline to server (project: {project}, benchmark: {benchmark_name})"
                            )
                        })?;
                    eprintln!(
                        "Promoted baseline {} version {} to server",
                        response.benchmark, response.version
                    );
                    Ok::<(), anyhow::Error>(())
                })?;
            } else {
                // Promote to local file
                let to_path = to.ok_or_else(|| {
                    anyhow::anyhow!("--to is required when not using --to-server")
                })?;

                let result = PromoteUseCase::execute(PromoteRequest { receipt, normalize });
                write_json_to_location(&to_path, &result.receipt, pretty)?;
            }

            Ok(())
        }

        Command::Report(args) => {
            let ReportArgs {
                compare,
                out,
                md,
                md_template,
                pretty,
            } = *args;

            let compare_receipt: CompareReceipt = read_json(&compare)?;

            let result = ReportUseCase::execute(ReportRequest {
                compare: compare_receipt.clone(),
            });

            write_json(&out, &result.report, pretty)?;

            // Optionally write markdown summary
            if let Some(md_path) = md {
                let md_content = render_markdown_with_optional_template(
                    &compare_receipt,
                    md_template.as_deref(),
                )?;
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

        Command::Check(args) => {
            let CheckArgs {
                config,
                bench,
                all,
                bench_regex,
                out_dir,
                baseline,
                require_baseline,
                fail_on_warn,
                noise_threshold,
                noise_policy,
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
            } = *args;

            let req = CheckConfig {
                config_path: config,
                bench,
                all,
                bench_regex,
                out_dir,
                baseline,
                require_baseline,
                fail_on_warn,
                noise_threshold,
                noise_policy,
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
                server_flags,
            };
            match mode {
                OutputMode::Standard => run_check_standard(req),
                OutputMode::Cockpit => run_check_cockpit(req),
            }
        }

        Command::Paired(args) => {
            let PairedArgs {
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
                require_significance,
                significance_alpha,
                significance_min_samples,
                max_retries,
                fail_on_regression,
                out,
                pretty,
            } = *args;

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
                significance_alpha,
                significance_min_samples,
                require_significance,
                max_retries,
                fail_on_regression,
            })?;

            write_json(&out, &outcome.receipt, pretty)?;

            if outcome.failed && !allow_nonzero {
                anyhow::bail!("paired benchmark failed: {}", outcome.reasons.join(", "));
            }

            Ok(())
        }

        Command::Baseline { action } => execute_baseline_action(action, &server_flags),

        Command::Summary {
            files,
            allow_nonzero,
        } => {
            let usecase = SummaryUseCase;
            let outcome = usecase.execute(SummaryRequest { files })?;
            println!("{}", usecase.render_markdown(&outcome));

            if outcome.failed && !allow_nonzero {
                anyhow::bail!("Matrix gating failed: at least one benchmark regression detected.");
            }

            Ok(())
        }

        Command::Aggregate { files, out, pretty } => {
            let usecase = perfgate_app::AggregateUseCase;
            let mut resolved_files = Vec::new();
            for pattern in files {
                for entry in glob(&pattern).map_err(|e| anyhow::anyhow!("invalid glob: {}", e))? {
                    resolved_files.push(entry?);
                }
            }
            let outcome = usecase.execute(perfgate_app::AggregateRequest {
                files: resolved_files,
            })?;
            write_json(&out, &outcome.receipt, pretty)?;
            Ok(())
        }

        Command::Bisect(args) => {
            let usecase = BisectUseCase::default();
            usecase.execute(BisectRequest {
                good: args.good.clone(),
                bad: args.bad.clone(),
                build_cmd: args.build_cmd.clone(),
                executable: args.executable.clone(),
                threshold: args.threshold,
            })?;
            Ok(())
        }

        Command::Blame(args) => execute_blame(*args),

        Command::Explain {
            compare,
            baseline_lock,
            current_lock,
        } => {
            let usecase = ExplainUseCase;
            let outcome = usecase.execute(ExplainRequest {
                compare,
                baseline_lock,
                current_lock,
            })?;
            println!("{}", outcome.markdown);
            Ok(())
        }
    }
}

fn execute_blame(args: BlameArgs) -> anyhow::Result<()> {
    let usecase = BlameUseCase;
    let outcome = usecase.execute(BlameRequest {
        baseline_lock: args.baseline,
        current_lock: args.current,
    })?;

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&outcome.blame)?);
    } else {
        println!("# Binary Blame: Dependency Changes\n");
        if outcome.blame.changes.is_empty() {
            println!("No dependency changes detected.");
        } else {
            for change in outcome.blame.changes {
                match change.change_type {
                    DependencyChangeType::Added => {
                        println!(
                            "- Added: {} v{}",
                            change.name,
                            change.new_version.as_deref().unwrap_or("?")
                        );
                    }
                    DependencyChangeType::Removed => {
                        println!(
                            "- Removed: {} v{}",
                            change.name,
                            change.old_version.as_deref().unwrap_or("?")
                        );
                    }
                    DependencyChangeType::Updated => {
                        println!(
                            "- Updated: {} ({} -> {})",
                            change.name,
                            change.old_version.as_deref().unwrap_or("?"),
                            change.new_version.as_deref().unwrap_or("?")
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

/// Execute baseline management actions.
fn execute_baseline_action(
    action: BaselineAction,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;

    let rt = tokio::runtime::Runtime::new()?;

    match action {
        BaselineAction::List {
            project,
            prefix,
            limit,
            include_receipts,
        } => {
            let project = server_config.resolve_project(project)?;

            let mut query = ListBaselinesQuery::new().with_limit(limit);
            if let Some(prefix) = prefix {
                query = query.with_benchmark_prefix(prefix);
            }
            if include_receipts {
                query = query.with_receipts();
            }

            rt.block_on(async {
                let response =
                    client
                        .list_baselines(&project, &query)
                        .await
                        .with_context(|| {
                            format!("Failed to list baselines for project '{}'", project)
                        })?;

                if response.baselines.is_empty() {
                    println!("No baselines found.");
                } else {
                    println!(
                        "Baselines ({} of {}):",
                        response.baselines.len(),
                        response.pagination.total
                    );
                    for baseline in &response.baselines {
                        println!(
                            "  {} - version {} ({})",
                            baseline.benchmark, baseline.version, baseline.created_at
                        );
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::Download {
            benchmark,
            output,
            project,
            version,
        } => {
            let project = server_config.resolve_project(project)?;

            rt.block_on(async {
                let record = if let Some(version) = version {
                    client
                        .get_baseline_version(&project, &benchmark, &version)
                        .await
                        .with_context(|| {
                            format!("Failed to get baseline {} version {}", benchmark, version)
                        })?
                } else {
                    client
                        .get_latest_baseline(&project, &benchmark)
                        .await
                        .with_context(|| {
                            format!("Failed to get latest baseline for {}", benchmark)
                        })?
                };

                let receipt = record.receipt;
                write_json(&output, &receipt, true)
                    .with_context(|| format!("Failed to write baseline to {}", output.display()))?;

                eprintln!(
                    "Downloaded baseline {} version {} to {}",
                    benchmark,
                    record.version,
                    output.display()
                );

                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::Upload {
            file,
            benchmark,
            project,
            version,
            normalize,
        } => {
            let project = server_config.resolve_project(project)?;

            let receipt: RunReceipt = read_json(&file)
                .with_context(|| format!("Failed to read run receipt from {}", file.display()))?;

            let benchmark_name = benchmark.unwrap_or_else(|| receipt.bench.name.clone());

            let request = UploadBaselineRequest {
                benchmark: benchmark_name.clone(),
                version,
                git_ref: None,
                git_sha: None,
                receipt,
                metadata: BTreeMap::new(),
                tags: Vec::new(),
                normalize,
            };

            rt.block_on(async {
                let response = client
                    .upload_baseline(&project, &request)
                    .await
                    .with_context(|| {
                        format!("Failed to upload baseline to server (project: {})", project)
                    })?;

                eprintln!(
                    "Uploaded baseline {} version {} to server",
                    response.benchmark, response.version
                );

                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::Delete {
            benchmark,
            project,
            version,
            force,
        } => {
            let project = server_config.resolve_project(project)?;

            if !force {
                eprintln!(
                    "Warning: This will delete baseline '{}' from project '{}'.",
                    benchmark, project
                );
                eprintln!("Use --force to confirm deletion.");
                anyhow::bail!("Deletion not confirmed. Use --force to proceed.");
            }

            let version_str = version.as_deref().unwrap_or("latest");

            rt.block_on(async {
                client
                    .delete_baseline(&project, &benchmark, version_str)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to delete baseline {} version {}",
                            benchmark, version_str
                        )
                    })?;

                eprintln!(
                    "Deleted baseline {} version {} from server",
                    benchmark, version_str
                );

                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::History {
            benchmark,
            project,
            limit,
        } => {
            let project = server_config.resolve_project(project)?;

            let query = ListBaselinesQuery::new()
                .with_benchmark(benchmark.clone())
                .with_limit(limit);

            rt.block_on(async {
                let response = client
                    .list_baselines(&project, &query)
                    .await
                    .with_context(|| format!("Failed to get history for baseline {}", benchmark))?;

                if response.baselines.is_empty() {
                    println!("No versions found for baseline '{}'.", benchmark);
                } else {
                    println!(
                        "Version history for {} ({} versions):",
                        benchmark,
                        response.baselines.len()
                    );
                    for baseline in &response.baselines {
                        let git_ref = baseline.git_ref.as_deref().unwrap_or("unknown");
                        println!(
                            "  {} - {} ({})",
                            baseline.version, baseline.created_at, git_ref
                        );
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::Verdicts {
            benchmark,
            project,
            limit,
            status,
        } => {
            let project = server_config.resolve_project(project)?;

            let mut query = ListVerdictsQuery::new().with_limit(limit);
            if let Some(bench) = benchmark {
                query = query.with_benchmark(bench);
            }
            if let Some(s) = status {
                query = query.with_status(s);
            }

            rt.block_on(async {
                let response = client
                    .list_verdicts(&project, &query)
                    .await
                    .with_context(|| {
                        format!("Failed to get verdict history for project {}", project)
                    })?;

                if response.verdicts.is_empty() {
                    println!("No verdicts found for project '{}'.", project);
                } else {
                    println!(
                        "Verdict history for {} ({} results):",
                        project,
                        response.verdicts.len()
                    );
                    for record in &response.verdicts {
                        let git_ref = record.git_ref.as_deref().unwrap_or("unknown");
                        println!(
                            "  [{:?}] {} - {} ({})",
                            record.status, record.benchmark, record.created_at, git_ref
                        );
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::SubmitVerdict {
            compare,
            project,
            git_ref,
            git_sha,
        } => {
            let project = server_config.resolve_project(project)?;
            let compare_receipt: CompareReceipt = read_json(&compare)?;

            let request = SubmitVerdictRequest {
                benchmark: compare_receipt.bench.name.clone(),
                run_id: compare_receipt
                    .current_ref
                    .run_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
                status: compare_receipt.verdict.status,
                counts: compare_receipt.verdict.counts.clone(),
                reasons: compare_receipt.verdict.reasons.clone(),
                git_ref,
                git_sha,
            };

            rt.block_on(async {
                client
                    .submit_verdict(&project, &request)
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to submit verdict for benchmark '{}'",
                            request.benchmark
                        )
                    })?;
                println!("Verdict submitted for benchmark '{}'", request.benchmark);
                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::Migrate {
            dir,
            project,
            recursive,
            dry_run,
        } => {
            let project = server_config.resolve_project(project)?;

            if !dir.exists() {
                anyhow::bail!("Directory does not exist: {}", dir.display());
            }

            let pattern = if recursive {
                format!("{}/**/*.json", dir.display())
            } else {
                format!("{}/*.json", dir.display())
            };

            let paths: Vec<PathBuf> = glob(&pattern)
                .with_context(|| format!("Invalid glob pattern: {}", pattern))?
                .filter_map(|e| e.ok())
                .filter(|p| p.is_file())
                .collect();

            if paths.is_empty() {
                println!("No baseline files found in {}.", dir.display());
                return Ok(());
            }

            println!(
                "Migrating {} baselines to project '{}'...",
                paths.len(),
                project
            );

            if dry_run {
                println!("Dry run enabled. No files will be uploaded.");
            }

            let mut success_count = 0;
            let mut error_count = 0;

            for path in paths {
                let res: anyhow::Result<()> = (|| {
                    let receipt: RunReceipt = read_json(&path).with_context(|| {
                        format!("Failed to read run receipt from {}", path.display())
                    })?;

                    if dry_run {
                        println!("Would upload: {}", path.display());
                        return Ok(());
                    }

                    let benchmark_name = receipt.bench.name.clone();
                    let request = UploadBaselineRequest {
                        benchmark: benchmark_name.clone(),
                        version: None,
                        git_ref: None,
                        git_sha: None,
                        receipt,
                        metadata: BTreeMap::new(),
                        tags: vec!["migration".to_string()],
                        normalize: true,
                    };

                    rt.block_on(async {
                        client
                            .upload_baseline(&project, &request)
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to upload baseline {} from {}",
                                    benchmark_name,
                                    path.display()
                                )
                            })?;
                        Ok::<_, anyhow::Error>(())
                    })?;

                    println!("Migrated: {}", benchmark_name);
                    Ok(())
                })();

                if let Err(err) = res {
                    eprintln!("Error migrating {}: {:#}", path.display(), err);
                    error_count += 1;
                } else {
                    success_count += 1;
                }
            }

            println!(
                "\nMigration complete: {} succeeded, {} failed.",
                success_count, error_count
            );

            if error_count > 0 {
                anyhow::bail!("Migration finished with errors.");
            }
        }
    }

    Ok(())
}

#[cfg(not(test))]
fn exit_with_code(code: i32) -> ! {
    std::process::exit(code);
}

#[cfg(test)]
fn exit_with_code(code: i32) -> ! {
    panic!("exit {code}");
}

/// Configuration for the check command.
#[derive(Debug, Clone)]
struct CheckConfig {
    config_path: PathBuf,
    bench: Option<String>,
    all: bool,
    bench_regex: Option<String>,
    out_dir: PathBuf,
    baseline: Option<PathBuf>,
    require_baseline: bool,
    fail_on_warn: bool,
    noise_threshold: Option<f64>,
    noise_policy: Option<perfgate_types::NoisePolicy>,
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
    server_flags: ServerFlags,
}

fn submit_verdict_if_possible(
    server_flags: &ServerFlags,
    config_file: &ConfigFile,
    compare_receipt: &CompareReceipt,
) {
    let server_config = resolve_server_config(
        server_flags.baseline_server.clone(),
        server_flags.api_key.clone(),
        server_flags.project.clone(),
        &config_file.baseline_server,
    );

    if server_config.url.is_some()
        && let Ok(client) = server_config.require_fallback_client(
            Some(Path::new(DEFAULT_FALLBACK_BASELINE_DIR)),
            BASELINE_SERVER_NOT_CONFIGURED,
        )
        && let Ok(project) = server_config.resolve_project(None)
    {
        let request = SubmitVerdictRequest {
            benchmark: compare_receipt.bench.name.clone(),
            run_id: compare_receipt
                .current_ref
                .run_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            status: compare_receipt.verdict.status,
            counts: compare_receipt.verdict.counts.clone(),
            reasons: compare_receipt.verdict.reasons.clone(),
            git_ref: None, // Could be extracted if needed
            git_sha: None,
        };

        let _ = with_tokio_runtime(async {
            let _ = client.submit_verdict(&project, &request).await;
            Ok::<(), anyhow::Error>(())
        });
    }
}

/// Run check in standard mode (exit codes reflect verdict).
fn run_check_standard(req: CheckConfig) -> anyhow::Result<()> {
    // Load config file
    let config_content = fs::read_to_string(&req.config_path)
        .with_context(|| format!("read {}", req.config_path.display()))?;

    let config_file: ConfigFile = if req
        .config_path
        .extension()
        .map(|e| e == "json")
        .unwrap_or(false)
    {
        serde_json::from_str(&config_content)
            .with_context(|| format!("parse JSON config {}", req.config_path.display()))?
    } else {
        toml::from_str(&config_content)
            .with_context(|| format!("parse TOML config {}", req.config_path.display()))?
    };

    config_file
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;

    // Determine which benches to run
    let bench_names = resolve_bench_names(
        &config_file,
        req.bench.as_deref(),
        req.all,
        req.bench_regex.as_deref(),
    )?;
    let bench_count = bench_names.len() as u32;

    let markdown_template_path = req.md_template.or_else(|| {
        config_file
            .defaults
            .markdown_template
            .as_ref()
            .map(PathBuf::from)
    });
    let _markdown_template = load_template(markdown_template_path.as_deref())?;
    let github_output_path = resolve_github_output_path(req.output_github)?;

    // Track aggregate exit code: fail (2) > warn-as-fail (3) > pass (0)
    let mut max_exit_code: i32 = 0;
    let mut all_warnings: Vec<String> = Vec::new();
    let mut total_pass: u32 = 0;
    let mut total_warn: u32 = 0;
    let mut total_fail: u32 = 0;

    for bench_name in &bench_names {
        // For --all mode, use per-bench subdirectories
        let bench_out_dir = if req.all {
            req.out_dir.join(bench_name)
        } else {
            req.out_dir.clone()
        };

        // Resolve baseline path (--baseline flag only valid for single bench mode)
        let baseline_path = resolve_baseline_path(&req.baseline, bench_name, &config_file);
        let baseline_receipt = load_optional_baseline_receipt(&baseline_path)
            .map_err(|e| PerfgateError::Io(IoError::BaselineResolve(e.to_string())))?;

        // Create output directory
        fs::create_dir_all(&bench_out_dir).map_err(|e| {
            PerfgateError::Io(IoError::ArtifactWrite(format!(
                "create output dir {}: {}",
                bench_out_dir.display(),
                e
            )))
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
            require_baseline: req.require_baseline,
            fail_on_warn: req.fail_on_warn,
            noise_threshold: req.noise_threshold,
            noise_policy: req.noise_policy,
            tool: tool_info(),
            env: req.env.clone(),
            output_cap_bytes: req.output_cap_bytes,
            allow_nonzero: req.allow_nonzero,
            host_mismatch_policy: req.host_mismatch,
            significance_alpha: req.significance_alpha,
            significance_min_samples: req.significance_min_samples,
            require_significance: req.require_significance,
        })?;

        // Submit verdict to server if configured
        if let Some(compare) = &outcome.compare_receipt {
            submit_verdict_if_possible(&req.server_flags, &config_file, compare);
        }

        // Write artifacts
        write_check_artifacts(&outcome, req.pretty)
            .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;

        if let Some(compare) = &outcome.compare_receipt {
            let markdown =
                render_markdown_with_optional_template(compare, markdown_template_path.as_deref())?;
            atomic_write(&outcome.markdown_path, markdown.as_bytes())
                .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;
        } else {
            let msg = "markdown template ignored for no-baseline bench".to_string();
            if req.all {
                all_warnings.push(format!("[{}] {}", bench_name, msg));
            } else {
                all_warnings.push(msg);
            }
        }
        for warning in &outcome.warnings {
            if req.all {
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

/// Run check in cockpit mode (always write receipt, exit 0 unless catastrophic).
fn run_check_cockpit(req: CheckConfig) -> anyhow::Result<()> {
    eprintln!("DEBUG: run_check_cockpit entered");
    let clock = SystemClock;
    let started_at = clock.now_rfc3339();
    let start_instant = Instant::now();
    let github_output_path = resolve_github_output_path(req.output_github)?;

    // Ensure base output directory exists (catastrophic failure if we can't)
    fs::create_dir_all(&req.out_dir)
        .with_context(|| format!("create output dir {}", req.out_dir.display()))?;

    // Try to run the check; capture errors
    let result = run_check_cockpit_inner(
        &req,
        &started_at,
        start_instant,
        github_output_path.as_deref(),
    );

    match result {
        Ok(()) => Ok(()),
        Err(err) => {
            // Error during execution - still try to write an error report
            let ended_at = clock.now_rfc3339();
            let duration_ms = start_instant.elapsed().as_millis() as u64;

            let baseline_available = req
                .baseline
                .as_ref()
                .and_then(|p| location_exists(p).ok())
                .unwrap_or(false);

            let (stage, error_kind) = classify_error(&err);

            let builder = SensorReportBuilder::new(tool_info(), started_at)
                .ended_at(ended_at, duration_ms)
                .baseline(baseline_available, None);

            let error_report = builder.build_error(&err.to_string(), stage, error_kind);

            // Try to write the error report
            let report_path = req.out_dir.join("report.json");
            if write_json(&report_path, &error_report, req.pretty).is_ok() {
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
                eprintln!("error: {}", err);
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
fn run_check_cockpit_inner(
    req: &CheckConfig,
    started_at: &str,
    start_instant: Instant,
    github_output_path: Option<&Path>,
) -> anyhow::Result<()> {
    let clock = SystemClock;

    // Load config file
    let config_content = fs::read_to_string(&req.config_path)
        .with_context(|| format!("read {}", req.config_path.display()))?;

    let config_file: ConfigFile = if req
        .config_path
        .extension()
        .map(|e| e == "json")
        .unwrap_or(false)
    {
        serde_json::from_str(&config_content)
            .with_context(|| format!("parse JSON config {}", req.config_path.display()))?
    } else {
        toml::from_str(&config_content)
            .with_context(|| format!("parse TOML config {}", req.config_path.display()))?
    };

    config_file
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;

    // Determine which benches to run
    let bench_names = resolve_bench_names(
        &config_file,
        req.bench.as_deref(),
        req.all,
        req.bench_regex.as_deref(),
    )?;
    let markdown_template_path = req.md_template.clone().or_else(|| {
        config_file
            .defaults
            .markdown_template
            .as_ref()
            .map(PathBuf::from)
    });
    let _markdown_template = load_template(markdown_template_path.as_deref())?;

    let multi_bench = bench_names.len() > 1;

    // Collect per-bench outcomes
    let mut bench_outcomes: Vec<BenchOutcome> = Vec::new();

    for bench_name in &bench_names {
        let outcome: BenchOutcome = (|| -> anyhow::Result<BenchOutcome> {
            // Create extras directory for native artifacts
            let extras_dir = if multi_bench {
                req.out_dir.join("extras").join(bench_name)
            } else {
                req.out_dir.join("extras")
            };
            fs::create_dir_all(&extras_dir).map_err(|e| {
                PerfgateError::Io(IoError::ArtifactWrite(format!(
                    "create extras dir {}: {}",
                    extras_dir.display(),
                    e
                )))
            })?;

            // Resolve baseline path
            let baseline_path = resolve_baseline_path(&req.baseline, bench_name, &config_file);
            let baseline_receipt = load_optional_baseline_receipt(&baseline_path)
                .map_err(|e| PerfgateError::Io(IoError::BaselineResolve(e.to_string())))?;

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
                require_baseline: req.require_baseline,
                fail_on_warn: req.fail_on_warn,
                noise_threshold: req.noise_threshold,
                noise_policy: req.noise_policy,
                tool: tool_info(),
                env: req.env.clone(),
                output_cap_bytes: req.output_cap_bytes,
                allow_nonzero: req.allow_nonzero,
                host_mismatch_policy: req.host_mismatch,
                significance_alpha: req.significance_alpha,
                significance_min_samples: req.significance_min_samples,
                require_significance: req.require_significance,
            })?;

            // Submit verdict to server if configured
            if let Some(compare) = &check_outcome.compare_receipt {
                submit_verdict_if_possible(&req.server_flags, &config_file, compare);
            }

            // Write native artifacts to extras/
            write_check_artifacts(&check_outcome, req.pretty)
                .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;

            let final_markdown = if let Some(compare) = &check_outcome.compare_receipt {
                let rendered = render_markdown_with_optional_template(
                    compare,
                    markdown_template_path.as_deref(),
                )?;
                atomic_write(&check_outcome.markdown_path, rendered.as_bytes())
                    .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;
                rendered
            } else {
                eprintln!(
                    "warning: [{}] markdown template ignored for no-baseline bench",
                    bench_name
                );
                check_outcome.markdown.clone()
            };

            // Rename extras files to versioned names
            rename_extras_to_versioned(&extras_dir)
                .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;

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
                markdown: final_markdown,
                extras_prefix: Some(extras_prefix),
                report: Box::new(check_outcome.report),
            })
        })()
        .unwrap_or_else(|err| {
            let (stage, error_kind) = classify_error(&err);
            eprintln!("error: bench '{}': {:#}", bench_name, err);
            BenchOutcome::Error {
                bench_name: bench_name.clone(),
                error: err.to_string(),
                stage: stage.to_string(),
                kind: error_kind.to_string(),
            }
        });
        bench_outcomes.push(outcome);
    }

    // Build aggregated sensor report
    let ended_at = clock.now_rfc3339();
    let duration_ms = start_instant.elapsed().as_millis() as u64;

    let any_baseline_available = bench_outcomes.iter().any(|o| match o {
        BenchOutcome::Success { report, .. } => report.compare.is_some(),
        _ => false,
    });

    let baseline_reason = if !any_baseline_available {
        Some(BASELINE_REASON_NO_BASELINE.to_string())
    } else {
        None
    };

    let all_baseline_available = bench_outcomes.iter().all(|o| match o {
        BenchOutcome::Success { report, .. } => report.compare.is_some(),
        _ => false,
    });

    let builder = SensorReportBuilder::new(tool_info(), started_at.to_string())
        .ended_at(ended_at, duration_ms)
        .baseline(all_baseline_available, baseline_reason);

    let (sensor_report, combined_markdown) = builder.build_aggregated(&bench_outcomes);

    // Write sensor report to out_dir/report.json
    let report_path = req.out_dir.join("report.json");
    write_json(&report_path, &sensor_report, req.pretty)?;

    // Write combined markdown to out_dir root
    let md_dest = req.out_dir.join("comment.md");
    fs::write(&md_dest, &combined_markdown)
        .with_context(|| format!("write {}", md_dest.display()))?;

    if let Some(path) = github_output_path {
        let summary = GitHubOutputSummary {
            verdict: verdict_from_sensor(&sensor_report.verdict.status),
            pass_count: sensor_report.verdict.counts.info,
            warn_count: sensor_report.verdict.counts.warn,
            fail_count: sensor_report.verdict.counts.error,
            bench_count: bench_names.len() as u32,
            exit_code: 0,
        };
        // Cockpit: if write fails, warn but don't fail tool
        if let Err(e) = write_github_outputs(path, &summary) {
            eprintln!("warning: failed to write GITHUB_OUTPUT: {}", e);
        }
    }

    // Cockpit mode: always exit 0 if we got here
    Ok(())
}

fn update_max_exit_code(max_exit_code: &mut i32, outcome_exit_code: i32) {
    debug_assert!(
        (0..=3).contains(&outcome_exit_code),
        "outcome_exit_code {} out of bounds",
        outcome_exit_code
    );
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
            let compare_receipt: CompareReceipt = read_json(&compare_path)?;
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

fn parse_noise_policy(s: &str) -> Result<perfgate_types::NoisePolicy, String> {
    match s.to_lowercase().as_str() {
        "warn" => Ok(perfgate_types::NoisePolicy::Warn),
        "skip" => Ok(perfgate_types::NoisePolicy::Skip),
        "ignore" => Ok(perfgate_types::NoisePolicy::Ignore),
        _ => Err(format!(
            "invalid noise policy: {s} (expected warn|skip|ignore)"
        )),
    }
}

fn parse_verdict_status(s: &str) -> Result<VerdictStatus, String> {
    match s.to_lowercase().as_str() {
        "pass" => Ok(VerdictStatus::Pass),
        "warn" => Ok(VerdictStatus::Warn),
        "fail" => Ok(VerdictStatus::Fail),
        "skip" => Ok(VerdictStatus::Skip),
        _ => Err(format!(
            "invalid verdict status: {s} (expected pass|warn|fail|skip)"
        )),
    }
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

fn parse_significance_alpha(s: &str) -> Result<f64, String> {
    let alpha: f64 = s.parse().map_err(|_| format!("invalid float value: {s}"))?;
    if !(0.0..=1.0).contains(&alpha) {
        return Err(format!(
            "significance alpha must be between 0.0 and 1.0, got {alpha}"
        ));
    }
    Ok(alpha)
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
    store: Arc<dyn ObjectStore>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, HostInfo, PerfgateReport, RUN_SCHEMA_V1, ReportSummary, RunMeta, RunReceipt,
        Stats, U64Summary, Verdict, VerdictCounts, VerdictStatus,
    };

    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;
    use uuid::Uuid;

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
            wall_ms: U64Summary::new(wall_ms, wall_ms, wall_ms),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        }
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
  "verdict": {{"status": "{}", "counts": {{"pass": 0, "warn": 0, "fail": 1, "skip": 0}}, "reasons": ["wall_ms_fail"]}}
}}"#,
            metric_status, verdict_status
        )
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
    fn parse_significance_alpha_accepts_valid_values() {
        assert!((parse_significance_alpha("0.0").unwrap() - 0.0).abs() < f64::EPSILON);
        assert!((parse_significance_alpha("0.05").unwrap() - 0.05).abs() < f64::EPSILON);
        assert!((parse_significance_alpha("0.5").unwrap() - 0.5).abs() < f64::EPSILON);
        assert!((parse_significance_alpha("1.0").unwrap() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_significance_alpha_rejects_out_of_range() {
        let err = parse_significance_alpha("-0.1").unwrap_err();
        assert!(
            err.contains("significance alpha must be between 0.0 and 1.0"),
            "unexpected error: {}",
            err
        );

        let err = parse_significance_alpha("1.1").unwrap_err();
        assert!(
            err.contains("significance alpha must be between 0.0 and 1.0"),
            "unexpected error: {}",
            err
        );

        let err = parse_significance_alpha("2.0").unwrap_err();
        assert!(
            err.contains("significance alpha must be between 0.0 and 1.0"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn parse_significance_alpha_rejects_non_numeric() {
        let err = parse_significance_alpha("abc").unwrap_err();
        assert!(
            err.contains("invalid float value"),
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
                    skip: 0,
                },
                reasons: Vec::new(),
            },
            compare: None,
            findings: Vec::new(),
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 0,
                skip_count: 0,
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
                    skip: 0,
                },
                reasons: Vec::new(),
            },
            compare: None,
            findings: Vec::new(),
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 0,
                skip_count: 0,
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
    fn print_cli_size() {
        println!("Size of Cli: {}", std::mem::size_of::<Cli>());
        println!("Size of Command: {}", std::mem::size_of::<Command>());
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
