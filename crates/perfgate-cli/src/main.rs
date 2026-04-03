//! perfgate CLI - entry point for all workflows.

use anyhow::Context;
use clap::{Args, Parser, Subcommand, ValueEnum};
use glob::glob;
use object_store::{ObjectStore, path::Path as ObjectPath};
use perfgate_adapters::{HostProbe, StdHostProbe, StdProcessRunner};
use perfgate_app::baseline_resolve::{is_remote_storage_uri, resolve_baseline_path};
use perfgate_app::comparison_logic::{build_budgets, build_metric_statistics, verdict_from_counts};
use perfgate_app::init::{
    CiPlatform, Preset, ci_workflow_path, discover_benchmarks, generate_config, render_config_toml,
    scaffold_ci,
};
use perfgate_app::{
    BadgeInput, BadgeStyle, BadgeType, BadgeUseCase, BenchOutcome, BisectRequest, BisectUseCase,
    BlameRequest, BlameUseCase, CheckOutcome, CheckRequest, CheckUseCase, Clock, CompareRequest,
    CompareUseCase, DiffRequest, DiffUseCase, ExplainRequest, ExplainUseCase, ExportFormat,
    ExportUseCase, PairedRunRequest, PairedRunUseCase, PromoteRequest, PromoteUseCase,
    RatchetUseCase, ReportRequest, ReportUseCase, RunBenchRequest, RunBenchUseCase,
    SensorReportBuilder, SystemClock, classify_error, github_annotations, is_host_mismatch_reason,
    preview_lines, redact_command_for_diagnostics, render_json_diff, render_markdown,
    render_markdown_template, render_terminal_diff,
    watch::{Debouncer, WatchRunRequest, WatchState, execute_watch_run, render_watch_display},
};
use perfgate_client::{
    AuthMethod, BaselineClient, ClientConfig, ListBaselinesQuery, ListVerdictsQuery,
    SubmitVerdictRequest, UploadBaselineRequest,
};
use perfgate_config::{
    ResolvedServerConfig, apply_ratchet_toml_changes, load_config_file,
    preview_ratchet_toml_changes, resolve_server_config,
};
use perfgate_domain::{DependencyChangeType, SignificancePolicy};
use perfgate_error::{ConfigValidationError, IoError, PerfgateError};
use perfgate_github::{CommentOptions, GitHubClient};
use perfgate_ingest::IngestFormat;
use perfgate_profile::{ProfileRequest, capture_flamegraph};
use perfgate_scaling::{
    ScalingReport, SizeMeasurement, classify_complexity, parse_complexity, render_ascii_chart,
};
use perfgate_summary::{SummaryRequest, SummaryUseCase};
use perfgate_types::{
    AggregationPolicy, BASELINE_REASON_NO_BASELINE, BaselineServerConfig, ChangedFilesSummary,
    CompareReceipt, CompareRef, ConfigFile, FailIfNOfM, HostMismatchPolicy, MetricStatus,
    OtelSpanIdentifiers, PerfgateReport, REPAIR_CONTEXT_SCHEMA_V1, RatchetConfig,
    RepairContextReceipt, RepairGitMetadata, RepairMetricBreach, RunReceipt, SensorVerdictStatus,
    ToolInfo, VerdictStatus,
};
use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
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

    /// Preview or apply conservative budget ratcheting from compare evidence.
    Ratchet {
        #[command(subcommand)]
        action: RatchetAction,
    },

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

    /// Aggregate multiple run receipts (e.g. from a fleet) into a formal aggregate receipt.
    Aggregate {
        /// Paths to run receipts (glob patterns supported)
        #[arg(required = true, num_args = 1..)]
        files: Vec<String>,

        /// Aggregation policy: all, majority, weighted, quorum, fail_if_n_of_m
        #[arg(long, default_value = "all", value_parser = parse_aggregation_policy)]
        policy: AggregationPolicy,

        /// Quorum threshold used by quorum/weighted policies (0.0 to 1.0)
        #[arg(long)]
        quorum: Option<f64>,

        /// Fail threshold N for fail_if_n_of_m policy
        #[arg(long)]
        fail_n: Option<u32>,

        /// Optional expected total runners M for fail_if_n_of_m policy
        #[arg(long)]
        fail_m: Option<u32>,

        /// Runner weights as label=value (repeatable), e.g. linux-x86_64=0.5
        #[arg(long = "weight")]
        weights: Vec<String>,

        /// Optional runner class/tag added to each input in the aggregate receipt
        #[arg(long)]
        runner_class: Option<String>,

        /// Optional benchmark lane/group added to each input in the aggregate receipt
        #[arg(long)]
        lane: Option<String>,

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

    /// Wrap `cargo bench` and produce perfgate run receipts.
    ///
    /// Auto-detects Criterion or libtest bench output. Scans `target/criterion/`
    /// for Criterion JSON, or parses libtest bench output format.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    CargoBench(Box<CargoBenchArgs>),

    /// Analyze changes in Cargo.lock to identify dependency updates causing binary size regressions.
    Blame(Box<BlameArgs>),

    /// Fleet-wide dependency regression analysis across projects.
    Fleet {
        #[command(subcommand)]
        action: FleetAction,
    },

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

    /// Import benchmark results from external frameworks into perfgate format.
    ///
    /// Supports Criterion, hyperfine, Go bench, and pytest-benchmark.
    /// Produces a standard perfgate.run.v1 receipt that can be used with
    /// compare, report, check, and other perfgate commands.
    Ingest(Box<IngestArgs>),

    /// Generate an embeddable SVG status badge from a report or compare receipt.
    ///
    /// Badge types:
    /// - status: overall verdict (passing/warning/failing)
    /// - metric: single metric value with delta
    /// - trend: performance trend summary
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Badge(Box<BadgeArgs>),

    /// Auto-detect benchmarks in a repository without manual configuration.
    ///
    /// Scans the project directory for common benchmark frameworks:
    /// Rust/Criterion, Go benchmarks, Python/pytest-benchmark,
    /// JavaScript/Benchmark.js, and executable files in well-known directories.
    Discover {
        /// Directory to scan (defaults to current directory)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Output results as JSON instead of a table
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Quick "did I make it slower?" comparison against baselines.
    ///
    /// Auto-discovers perfgate.toml, runs benchmarks, compares against
    /// existing baselines, and prints a colored terminal diff.
    ///
    /// Exit codes:
    /// - 0: pass (no regressions)
    /// - 1: tool error
    /// - 2: fail (budget violated)
    Diff(Box<DiffArgs>),

    /// Scan a repository and generate a perfgate.toml config file.
    ///
    /// Auto-detects benchmarks (Cargo [[bench]], Criterion, Go, pytest-benchmark)
    /// and generates a ready-to-use configuration. Optionally scaffolds a CI workflow.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Init(Box<InitArgs>),

    /// Watch for file changes and re-run benchmarks with live terminal output.
    ///
    /// Monitors the workspace for file changes and automatically re-runs
    /// the specified benchmark, showing a live performance delta display.
    ///
    /// Exit codes: 0 on clean exit (Ctrl+C), 1 on error.
    Watch(Box<WatchArgs>),

    /// Start a local dashboard server backed by SQLite.
    ///
    /// Launches the perfgate server on localhost with no authentication,
    /// using ~/.perfgate/data.db as the default database. Opens the
    /// dashboard in the default browser unless --no-open is passed.
    Serve(Box<ServeArgs>),

    /// Validate computational complexity (scaling behavior) of a benchmark.
    ///
    /// Runs a command at multiple input sizes, fits complexity models (O(1)
    /// through O(n^3) and exponential), and reports the best-fitting class.
    /// If --expected is given, fails when detected complexity is worse.
    ///
    /// Exit codes:
    /// - 0: scaling validation passed
    /// - 1: tool error
    /// - 2: complexity degradation detected (policy fail)
    Scale(Box<ScaleArgs>),

    /// Post or update a performance report comment on a GitHub pull request.
    ///
    /// Reads a compare or report receipt and posts a rich Markdown comment
    /// on the specified PR. If a perfgate comment already exists, it is
    /// updated (idempotent). Supports both GitHub Actions tokens and
    /// personal access tokens.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Comment(Box<CommentArgs>),

    /// Analyze metric trends and predict budget threshold breaches.
    ///
    /// Fits a linear regression to recent metric history and warns when a
    /// benchmark is drifting toward its budget threshold.
    ///
    /// Exit codes: 0 for stable/improving, 1 for errors, 2 for critical drift.
    Trend(Box<TrendArgs>),
}

#[derive(Debug, Args)]
pub struct CargoBenchArgs {
    /// Specific bench target to run (passed as `cargo bench --bench <name>`)
    #[arg(long)]
    pub bench: Option<String>,

    /// Path to a baseline receipt for comparison
    #[arg(long)]
    pub compare: Option<PathBuf>,

    /// Output file path for the run receipt
    #[arg(long, default_value = "perfgate-cargo-bench.json")]
    pub out: PathBuf,

    /// Also write individual per-benchmark receipts to this directory
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Override the cargo target directory (default: auto-detect)
    #[arg(long)]
    pub target_dir: Option<PathBuf>,

    /// Include a hashed hostname in the host fingerprint
    #[arg(long, default_value_t = false)]
    pub include_hostname_hash: bool,

    /// Pretty-print JSON output
    #[arg(long, default_value_t = false)]
    pub pretty: bool,

    /// Extra arguments passed to `cargo bench` (after --)
    #[arg(last = true)]
    pub extra_args: Vec<String>,
}

/// Badge type selector for the badge subcommand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum BadgeTypeArg {
    /// Overall verdict badge (passing/warning/failing)
    #[default]
    Status,
    /// Single metric value with delta indicator
    Metric,
    /// Performance trend summary
    Trend,
}

/// Badge visual style selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum BadgeStyleArg {
    /// Rounded ends (default shields.io style)
    #[default]
    Flat,
    /// Square ends
    FlatSquare,
}

#[derive(Debug, Args)]
pub struct BadgeArgs {
    /// Path to a report receipt (perfgate.report.v1)
    #[arg(long, conflicts_with = "compare")]
    pub report: Option<PathBuf>,

    /// Path to a compare receipt (perfgate.compare.v1)
    #[arg(long, conflicts_with = "report")]
    pub compare: Option<PathBuf>,

    /// Badge type to generate
    #[arg(long, value_enum, default_value_t = BadgeTypeArg::Status, rename_all = "kebab-case")]
    pub r#type: BadgeTypeArg,

    /// Badge visual style
    #[arg(long, value_enum, default_value_t = BadgeStyleArg::Flat)]
    pub style: BadgeStyleArg,

    /// Metric name (required when --type metric)
    #[arg(long)]
    pub metric: Option<String>,

    /// Output SVG file path (omit for stdout)
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Print SVG to stdout (equivalent to omitting --out)
    #[arg(long)]
    pub stdout: bool,
}

#[derive(Debug, Args)]
pub struct ScaleArgs {
    /// Bench identifier
    #[arg(long)]
    pub name: String,

    /// Command template with {n} placeholder for input size.
    /// Example: "./target/release/my-bench --size {n}"
    #[arg(long)]
    pub command: String,

    /// Comma-separated input sizes to test.
    /// Example: 100,1000,10000,100000
    #[arg(long, value_delimiter = ',')]
    pub sizes: Vec<u64>,

    /// Number of repetitions per input size.
    #[arg(long, default_value_t = 5)]
    pub repeat: u32,

    /// Expected complexity class (e.g., "O(n)", "O(n^2)").
    /// If specified, fails when detected complexity is worse.
    #[arg(long)]
    pub expected: Option<String>,

    /// Minimum R-squared for a valid fit (0.0 to 1.0).
    #[arg(long, default_value_t = 0.90)]
    pub r_squared_threshold: f64,

    /// Working directory
    #[arg(long)]
    pub cwd: Option<PathBuf>,

    /// Per-run timeout (e.g. "2s")
    #[arg(long)]
    pub timeout: Option<String>,

    /// Output file path
    #[arg(long, default_value = "perfgate-scaling.json")]
    pub out: PathBuf,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,

    /// Chart width in characters
    #[arg(long, default_value_t = 60)]
    pub chart_width: usize,

    /// Chart height in lines
    #[arg(long, default_value_t = 20)]
    pub chart_height: usize,
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
pub struct DiffArgs {
    /// Run only a specific benchmark (must match a [[bench]] in config).
    #[arg(long)]
    pub bench: Option<String>,

    /// Compare against a specific git ref (commit or branch). Reserved for future use.
    #[arg(long)]
    pub against: Option<String>,

    /// Reduce repeat count for faster feedback.
    #[arg(long, default_value_t = false)]
    pub quick: bool,

    /// Output comparison as JSON instead of terminal rendering.
    #[arg(long, default_value_t = false)]
    pub json: bool,

    /// Path to the config file (default: auto-discover by walking up from cwd).
    #[arg(long)]
    pub config: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ServeArgs {
    /// Port to listen on (default: 8484)
    #[arg(long, default_value_t = 8484)]
    pub port: u16,

    /// Path to the SQLite database file (default: ~/.perfgate/data.db)
    #[arg(long)]
    pub db: Option<PathBuf>,

    /// Do not open the browser automatically
    #[arg(long, default_value_t = false)]
    pub no_open: bool,
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
pub struct CommentArgs {
    /// Path to a compare receipt (mutually exclusive with --report)
    #[arg(long, conflicts_with = "report")]
    pub compare: Option<PathBuf>,

    /// Path to a report receipt (mutually exclusive with --compare)
    #[arg(long, conflicts_with = "compare")]
    pub report: Option<PathBuf>,

    /// GitHub token for API authentication.
    /// Can also be set via GITHUB_TOKEN environment variable.
    #[arg(long)]
    pub github_token: Option<String>,

    /// Repository in owner/repo format.
    /// Can also be set via GITHUB_REPOSITORY environment variable.
    #[arg(long)]
    pub repo: Option<String>,

    /// Pull request number.
    /// If not specified, will attempt to parse from GITHUB_REF (refs/pull/N/merge).
    #[arg(long)]
    pub pr: Option<u64>,

    /// GitHub API base URL (default: https://api.github.com).
    #[arg(long, default_value = "https://api.github.com")]
    pub github_api_url: String,

    /// Optional blame text to include in the comment (output from `perfgate blame`).
    #[arg(long)]
    pub blame_text: Option<String>,

    /// Dry-run mode: render the comment to stdout instead of posting to GitHub.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct TrendArgs {
    /// Paths to run receipts in chronological order (glob patterns supported).
    #[arg(long = "history", required = true, num_args = 1..)]
    pub history: Vec<String>,

    /// Budget threshold as a fraction (e.g., 0.20 for 20% regression allowed).
    #[arg(long, default_value = "0.20")]
    pub threshold: f64,

    /// Specific metric to analyze (e.g., wall_ms, cpu_ms, max_rss_kb).
    /// If omitted, all available metrics are analyzed.
    #[arg(long)]
    pub metric: Option<String>,

    /// Number of runs within which a breach is considered "critical".
    #[arg(long, default_value = "10")]
    pub critical_window: u32,

    /// Output format (text|json).
    #[arg(long, default_value = "text")]
    pub format: String,
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

    /// Upload the run result to the local perfgate server (started via `perfgate serve`).
    /// Set the server URL via PERFGATE_LOCAL_DB (default: http://127.0.0.1:8484).
    #[arg(long, default_value_t = false)]
    pub local_db: bool,

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

    /// Automatically capture a flamegraph when a regression is detected (warn or fail).
    /// Requires a profiler: perf (Linux), dtrace (macOS), or cargo-flamegraph.
    #[arg(long, default_value_t = false)]
    pub profile_on_regression: bool,
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

    /// Enable budget ratcheting on local promote path (never for --to-server).
    #[arg(long, default_value_t = false)]
    pub ratchet: bool,

    /// Compare receipt providing trustworthy evidence for ratcheting.
    #[arg(long, requires = "ratchet")]
    pub compare: Option<PathBuf>,

    /// Config file to update when --ratchet is enabled.
    #[arg(long, default_value = "perfgate.toml", requires = "ratchet")]
    pub config: PathBuf,

    /// Output path for machine-readable ratchet artifact.
    #[arg(long, default_value = "perfgate.ratchet.v1.json", requires = "ratchet")]
    pub ratchet_out: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum RatchetAction {
    /// Show exactly what would change in perfgate.toml.
    Preview(Box<RatchetPreviewArgs>),
}

#[derive(Debug, Args)]
pub struct RatchetPreviewArgs {
    /// Path to perfgate.toml.
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,
    /// Compare receipt path.
    #[arg(long)]
    pub compare: PathBuf,
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

    /// Upload the run result to the local perfgate server (started via `perfgate serve`).
    /// Set the server URL via PERFGATE_LOCAL_DB (default: http://127.0.0.1:8484).
    #[arg(long, default_value_t = false)]
    pub local_db: bool,

    /// Automatically capture a flamegraph when a regression is detected (warn or fail).
    /// Requires a profiler: perf (Linux), dtrace (macOS), or cargo-flamegraph.
    #[arg(long, default_value_t = false)]
    pub profile_on_regression: bool,

    /// Force `repair_context.json` emission even on passing checks.
    /// Warning and failing checks already emit it automatically.
    #[arg(long, default_value_t = false)]
    pub emit_repair_context: bool,
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

    /// CV threshold for early termination of retries.
    /// If the coefficient of variation of wall-time differences exceeds this value,
    /// retries are aborted because the benchmark is too noisy. Default: 0.5 (50%).
    #[arg(long)]
    pub cv_threshold: Option<f64>,

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

#[derive(Debug, Args)]
pub struct IngestArgs {
    /// Input format: criterion, hyperfine, gobench, pytest, otel
    #[arg(long)]
    pub format: String,

    /// Path to the input file (or directory for criterion)
    #[arg(long)]
    pub input: PathBuf,

    /// Benchmark name (default: derived from input data)
    #[arg(long)]
    pub name: Option<String>,

    /// Include span names when ingesting OTel JSON (exact match, repeatable).
    #[arg(long = "include-span")]
    pub include_span: Vec<String>,

    /// Exclude span names when ingesting OTel JSON (exact match, repeatable).
    #[arg(long = "exclude-span")]
    pub exclude_span: Vec<String>,

    /// Output file path
    #[arg(long, default_value = "perfgate-ingest.json")]
    pub out: PathBuf,

    /// Pretty-print JSON
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InitPreset {
    /// Balanced accuracy and speed (repeat=5, warmup=1, threshold=20%)
    Standard,
    /// High accuracy, tight thresholds (repeat=10, warmup=2, threshold=10%)
    Release,
    /// Quick validation, wide thresholds (repeat=3, warmup=1, threshold=30%)
    Tier1Fast,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InitCiPlatform {
    Github,
    Gitlab,
    Bitbucket,
    Circleci,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Directory to scan for benchmarks (default: current directory).
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// Output path for the generated config file.
    #[arg(long, default_value = "perfgate.toml")]
    pub output: PathBuf,

    /// Budget preset.
    #[arg(long, default_value = "standard")]
    pub preset: InitPreset,

    /// Also generate a CI workflow file.
    #[arg(long)]
    pub ci: Option<InitCiPlatform>,

    /// Accept defaults without prompting.
    #[arg(long, default_value_t = false)]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct WatchArgs {
    /// Path to the config file (TOML or JSON)
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Name of the benchmark to watch (must match a [[bench]] in config)
    #[arg(long)]
    pub bench: Option<String>,

    /// Watch all benchmarks defined in the config file
    #[arg(long, default_value_t = false)]
    pub all: bool,

    /// Debounce interval in milliseconds (wait for changes to settle)
    #[arg(long, default_value_t = 500)]
    pub debounce: u64,

    /// Do not clear the screen between runs
    #[arg(long, default_value_t = false)]
    pub no_clear: bool,

    /// Directories to watch (defaults to current directory)
    #[arg(long)]
    pub watch_dir: Vec<PathBuf>,

    /// Policy for handling host mismatches between baseline and current runs.
    #[arg(long, default_value = "warn", value_parser = parse_host_mismatch_policy)]
    pub host_mismatch: HostMismatchPolicy,

    /// Environment variable (KEY=VALUE). Repeatable.
    #[arg(long, value_parser = parse_key_val_string)]
    pub env: Vec<(String, String)>,
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

/// Subcommands for fleet-wide dependency regression analysis.
#[derive(Debug, Subcommand)]
enum FleetAction {
    /// List fleet-wide dependency regression alerts.
    Alerts {
        /// Minimum number of affected projects (default: 2)
        #[arg(long, default_value_t = 2)]
        min_affected: usize,

        /// Maximum number of alerts to show
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },

    /// Show impact of a specific dependency across projects.
    Impact {
        /// Dependency name to check
        #[arg(long)]
        dependency: String,

        /// Maximum number of results
        #[arg(long, default_value_t = 50)]
        limit: u32,
    },

    /// Record a dependency change event with performance impact.
    RecordEvent {
        /// Project name
        #[arg(long)]
        project: String,

        /// Benchmark name
        #[arg(long)]
        benchmark: String,

        /// Path to a compare receipt to extract delta from
        #[arg(long)]
        compare: PathBuf,

        /// Path to baseline Cargo.lock
        #[arg(long)]
        baseline_lock: PathBuf,

        /// Path to current Cargo.lock
        #[arg(long)]
        current_lock: PathBuf,

        /// Metric to track (default: wall_ms)
        #[arg(long, default_value = "wall_ms")]
        metric: String,
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
                local_db,
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
                    benchmark: name.clone(),
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

            // Upload to local server if --local-db is set
            if local_db {
                upload_to_local_db(&name, &outcome.receipt)?;
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
                profile_on_regression,
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
                tradeoffs: Vec::new(),
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

            // Profile on regression if requested
            if profile_on_regression && is_regression(compare_result.receipt.verdict.status) {
                let bench = &compare_result.receipt.bench;
                let out_parent = out.parent().unwrap_or_else(|| Path::new("."));
                try_capture_flamegraph(
                    &bench.command,
                    bench.cwd.as_deref(),
                    &bench.name,
                    out_parent,
                );
            }

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
                ratchet,
                compare,
                config,
                ratchet_out,
            } = *args;

            let receipt: RunReceipt = read_json_from_location(&current)?;

            if to_server && ratchet {
                anyhow::bail!(
                    "--ratchet is only supported for local promote (--to), not --to-server"
                );
            }

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

                if ratchet {
                    let compare_path = compare
                        .ok_or_else(|| anyhow::anyhow!("--compare is required with --ratchet"))?;
                    let compare_receipt: CompareReceipt = read_json(&compare_path)?;
                    let cfg = load_config_file(&config)?;
                    let policy = cfg.ratchet.unwrap_or_else(RatchetConfig::default);
                    let host_mismatch = is_host_mismatch_reason(&compare_receipt.verdict.reasons);
                    let plan = RatchetUseCase::preview(
                        &compare_receipt,
                        &policy,
                        Some(compare_path.display().to_string()),
                        host_mismatch,
                        tool_info(),
                    );

                    for line in preview_lines(&plan.receipt.changes) {
                        eprintln!("{line}");
                    }
                    let _ = preview_ratchet_toml_changes(&plan.receipt.changes);
                    let updated = apply_ratchet_toml_changes(
                        &config,
                        &compare_receipt.bench.name,
                        &plan.receipt.changes,
                    )?;
                    write_json(&ratchet_out, &plan.receipt, pretty)?;
                    if updated {
                        eprintln!("Applied ratchet updates to {}", config.display());
                    } else {
                        eprintln!("No ratchet updates were applied.");
                    }
                }
            }

            Ok(())
        }

        Command::Ratchet { action } => match action {
            RatchetAction::Preview(args) => {
                let compare_receipt: CompareReceipt = read_json(&args.compare)?;
                let cfg = load_config_file(&args.config)?;
                let policy = cfg.ratchet.unwrap_or_else(RatchetConfig::default);
                let host_mismatch = is_host_mismatch_reason(&compare_receipt.verdict.reasons);
                let plan = RatchetUseCase::preview(
                    &compare_receipt,
                    &policy,
                    Some(args.compare.display().to_string()),
                    host_mismatch,
                    tool_info(),
                );
                for line in preview_lines(&plan.receipt.changes) {
                    println!("{line}");
                }
                for line in preview_ratchet_toml_changes(&plan.receipt.changes) {
                    println!("{line}");
                }
                Ok(())
            }
        },

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
                local_db,
                profile_on_regression,
                emit_repair_context,
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
                profile_on_regression,
                emit_repair_context,
                server_flags,
                local_db,
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
                cv_threshold,
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
                cv_threshold,
            })?;

            write_json(&out, &outcome.receipt, pretty)?;

            if outcome.failed && !allow_nonzero {
                anyhow::bail!("paired benchmark failed: {}", outcome.reasons.join(", "));
            }

            Ok(())
        }

        Command::Baseline { action } => execute_baseline_action(action, &server_flags),

        Command::Fleet { action } => execute_fleet_action(action, &server_flags),

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

        Command::Aggregate {
            files,
            policy,
            quorum,
            fail_n,
            fail_m,
            weights,
            runner_class,
            lane,
            out,
            pretty,
        } => {
            let usecase = perfgate_app::AggregateUseCase;
            let mut resolved_files = Vec::new();
            for pattern in files {
                for entry in glob(&pattern).map_err(|e| anyhow::anyhow!("invalid glob: {}", e))? {
                    resolved_files.push(entry?);
                }
            }
            let (quorum, fail_if) = validate_aggregate_options(policy, quorum, fail_n, fail_m)?;
            let weight_map = parse_weight_map(&weights)?;
            let outcome = usecase.execute(perfgate_app::AggregateRequest {
                files: resolved_files,
                policy,
                quorum,
                fail_if,
                weights: weight_map,
                runner_class,
                lane,
            })?;
            write_json(&out, &outcome.aggregate, pretty)?;
            match outcome.aggregate.verdict.status {
                MetricStatus::Fail => exit_with_code(2),
                MetricStatus::Pass | MetricStatus::Warn | MetricStatus::Skip => Ok(()),
            }
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

        Command::CargoBench(args) => execute_cargo_bench(*args),

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

        Command::Ingest(args) => {
            let IngestArgs {
                format,
                input,
                name,
                include_span,
                exclude_span,
                out,
                pretty,
            } = *args;

            let format = IngestFormat::parse(&format).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown ingest format '{}'; supported: criterion, hyperfine, gobench, pytest, otel",
                    format
                )
            })?;

            let content = fs::read_to_string(&input)
                .with_context(|| format!("read input file {}", input.display()))?;

            let request = perfgate_ingest::IngestRequest {
                format,
                input: content,
                name,
                include_spans: include_span,
                exclude_spans: exclude_span,
            };

            let receipt = perfgate_ingest::ingest(&request)?;
            write_json(&out, &receipt, pretty)?;
            eprintln!("Ingested {} -> {}", input.display(), out.display());
            Ok(())
        }
        Command::Badge(args) => execute_badge(*args),

        Command::Discover { path, json } => {
            let scan_root = path.unwrap_or_else(|| PathBuf::from("."));
            let scan_root = fs::canonicalize(&scan_root)
                .with_context(|| format!("failed to resolve path: {}", scan_root.display()))?;
            let benchmarks = perfgate_app::discover::discover_all(&scan_root);

            if benchmarks.is_empty() {
                eprintln!("No benchmarks discovered in {}", scan_root.display());
                return Ok(());
            }

            if json {
                println!("{}", serde_json::to_string_pretty(&benchmarks)?);
            } else {
                print_discover_table(&benchmarks);
            }

            Ok(())
        }

        Command::Diff(args) => {
            let DiffArgs {
                bench,
                against,
                quick,
                json,
                config,
            } = *args;

            if against.is_some() {
                eprintln!("warning: --against is reserved for future use and currently ignored");
            }

            let runner = StdProcessRunner;
            let host_probe = StdHostProbe;
            let clock = SystemClock;
            let usecase = DiffUseCase::new(runner, host_probe, clock);

            let outcome = usecase.execute(DiffRequest {
                config_path: config,
                bench_filter: bench,
                against,
                quick,
                json,
                tool: tool_info(),
            })?;

            if json {
                let output = render_json_diff(&outcome)?;
                println!("{output}");
            } else {
                let output = render_terminal_diff(&outcome);
                print!("{output}");
            }

            if outcome.exit_code != 0 {
                exit_with_code(outcome.exit_code);
            }
            Ok(())
        }

        Command::Init(args) => execute_init(*args),
        Command::Watch(args) => run_watch(*args),
        Command::Serve(args) => execute_serve(*args),
        Command::Scale(args) => execute_scale(*args),
        Command::Comment(args) => execute_comment(*args),
        Command::Trend(args) => execute_trend(*args),
    }
}

fn execute_badge(args: BadgeArgs) -> anyhow::Result<()> {
    let input = match (&args.report, &args.compare) {
        (Some(path), None) => {
            let report: PerfgateReport = read_json(path)?;
            BadgeInput::Report(Box::new(report))
        }
        (None, Some(path)) => {
            let compare: CompareReceipt = read_json(path)?;
            BadgeInput::Compare(Box::new(compare))
        }
        (None, None) => {
            anyhow::bail!("one of --report or --compare is required");
        }
        (Some(_), Some(_)) => {
            // clap `conflicts_with` should prevent this, but be safe
            anyhow::bail!("--report and --compare are mutually exclusive");
        }
    };

    let badge_type = match args.r#type {
        BadgeTypeArg::Status => BadgeType::Status,
        BadgeTypeArg::Metric => BadgeType::Metric,
        BadgeTypeArg::Trend => BadgeType::Trend,
    };

    let badge_style = match args.style {
        BadgeStyleArg::Flat => BadgeStyle::Flat,
        BadgeStyleArg::FlatSquare => BadgeStyle::FlatSquare,
    };

    let usecase = BadgeUseCase;
    let outcome = usecase.execute(&input, badge_type, badge_style, args.metric.as_deref())?;

    match args.out {
        Some(ref path) if !args.stdout => {
            fs::write(path, &outcome.svg).with_context(|| format!("write {}", path.display()))?;
            eprintln!("wrote {}", path.display());
        }
        _ => {
            print!("{}", outcome.svg);
        }
    }

    Ok(())
}

fn print_discover_table(benchmarks: &[perfgate_app::discover::DiscoveredBenchmark]) {
    // Compute column widths
    let name_w = benchmarks
        .iter()
        .map(|b| b.name.len())
        .max()
        .unwrap_or(4)
        .max(4);
    let fw_w = benchmarks
        .iter()
        .map(|b| b.framework.len())
        .max()
        .unwrap_or(9)
        .max(9);
    let lang_w = benchmarks
        .iter()
        .map(|b| b.language.len())
        .max()
        .unwrap_or(8)
        .max(8);
    let cmd_w = benchmarks
        .iter()
        .map(|b| b.command.len())
        .max()
        .unwrap_or(7)
        .max(7);
    let conf_w = 10;

    // Header
    println!(
        "{:<name_w$}  {:<fw_w$}  {:<lang_w$}  {:<cmd_w$}  {:<conf_w$}",
        "NAME", "FRAMEWORK", "LANGUAGE", "COMMAND", "CONFIDENCE",
    );
    println!(
        "{:-<name_w$}  {:-<fw_w$}  {:-<lang_w$}  {:-<cmd_w$}  {:-<conf_w$}",
        "", "", "", "", "",
    );

    // Rows
    for b in benchmarks {
        println!(
            "{:<name_w$}  {:<fw_w$}  {:<lang_w$}  {:<cmd_w$}  {:<conf_w$}",
            b.name, b.framework, b.language, b.command, b.confidence,
        );
    }

    println!("\nDiscovered {} benchmark(s)", benchmarks.len());
}

fn execute_trend(args: TrendArgs) -> anyhow::Result<()> {
    use perfgate_app::{TrendRequest, TrendUseCase, format_trend_output};
    use perfgate_domain::TrendConfig;
    use perfgate_types::Metric;

    let mut resolved_files = Vec::new();
    for pattern in &args.history {
        for entry in glob(pattern).map_err(|e| anyhow::anyhow!("invalid glob: {}", e))? {
            resolved_files.push(entry?);
        }
    }

    if resolved_files.is_empty() {
        anyhow::bail!("no run receipt files matched the provided patterns");
    }

    let mut history: Vec<perfgate_types::RunReceipt> = Vec::new();
    for path in &resolved_files {
        let data = fs::read_to_string(path)
            .with_context(|| format!("reading run receipt: {}", path.display()))?;
        let receipt: perfgate_types::RunReceipt = serde_json::from_str(&data)
            .with_context(|| format!("parsing run receipt: {}", path.display()))?;
        history.push(receipt);
    }

    let metric = if let Some(ref m) = args.metric {
        Some(Metric::parse_key(m).ok_or_else(|| anyhow::anyhow!("unknown metric: {}", m))?)
    } else {
        None
    };

    let config = TrendConfig {
        critical_window: args.critical_window,
        ..TrendConfig::default()
    };

    let request = TrendRequest {
        history,
        threshold: args.threshold,
        metric,
        config,
    };

    let outcome = TrendUseCase.execute(request)?;

    if args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&outcome.analyses)?);
    } else {
        print!("{}", format_trend_output(&outcome));

        // Print mini charts for each analyzed metric
        for analysis in &outcome.analyses {
            let metric_key = &analysis.metric;
            if let Some(m) = Metric::parse_key(metric_key) {
                let values: Vec<f64> = resolved_files
                    .iter()
                    .filter_map(|p| {
                        let data = fs::read_to_string(p).ok()?;
                        let receipt: perfgate_types::RunReceipt =
                            serde_json::from_str(&data).ok()?;
                        perfgate_domain::metric_value(&receipt.stats, m)
                    })
                    .collect();
                if !values.is_empty() {
                    println!("{}", perfgate_app::format_trend_chart(&values, metric_key));
                }
            }
        }
    }

    // Exit with code 2 if any metric is in critical drift
    let has_critical = outcome
        .analyses
        .iter()
        .any(|a| a.drift == perfgate_domain::DriftClass::Critical);
    if has_critical {
        std::process::exit(2);
    }

    Ok(())
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

/// Execute the `cargo-bench` subcommand.
fn execute_cargo_bench(args: CargoBenchArgs) -> anyhow::Result<()> {
    use perfgate_app::cargo_bench::{
        BenchSource, benchmarks_to_individual_receipts, benchmarks_to_receipt,
        build_cargo_bench_command, detect_criterion, detect_target_dir, parse_libtest_output,
        scan_criterion_dir,
    };

    let target_dir = args.target_dir.unwrap_or_else(detect_target_dir);

    // Build and run `cargo bench`
    let cargo_cmd = build_cargo_bench_command(args.bench.as_deref(), &args.extra_args);

    eprintln!("Running: {}", cargo_cmd.join(" "));

    let output = std::process::Command::new(&cargo_cmd[0])
        .args(&cargo_cmd[1..])
        .output()
        .with_context(|| format!("failed to execute: {}", cargo_cmd.join(" ")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        // Print stderr so the user sees what went wrong
        eprint!("{stderr}");
        anyhow::bail!(
            "cargo bench exited with status {}",
            output.status.code().unwrap_or(-1)
        );
    }

    // Print stderr (Criterion/cargo compiler output) for user visibility
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }

    // Determine source: check Criterion first, then libtest
    let benchmarks = if detect_criterion(&target_dir) {
        let criterion_dir = target_dir.join("criterion");
        eprintln!("Detected Criterion output in {}", criterion_dir.display());
        scan_criterion_dir(&criterion_dir)?
    } else {
        // Try parsing libtest output from stdout
        let parsed = parse_libtest_output(&stdout);
        if parsed.is_empty() {
            // Also try stderr (some runners print there)
            let parsed_stderr = parse_libtest_output(&stderr);
            if parsed_stderr.is_empty() {
                anyhow::bail!(
                    "no benchmark results found: no Criterion data in {} and no libtest bench output detected",
                    target_dir.display()
                );
            }
            parsed_stderr
        } else {
            parsed
        }
    };

    let source = benchmarks
        .first()
        .map(|b| b.source)
        .unwrap_or(BenchSource::Libtest);

    eprintln!("Found {} benchmark(s) from {:?}", benchmarks.len(), source);

    // Collect host info and tool info
    let tool = tool_info();
    let host_probe = StdHostProbe;
    let host = host_probe.probe(&perfgate_adapters::HostProbeOptions {
        include_hostname_hash: args.include_hostname_hash,
    });
    let clock = SystemClock;

    let command_strs: Vec<String> = cargo_cmd;

    // Create aggregate receipt (all benchmarks in one receipt)
    let receipt = benchmarks_to_receipt(
        &benchmarks,
        "cargo-bench",
        &tool,
        &host,
        &clock,
        &command_strs,
    )?;

    write_json(&args.out, &receipt, args.pretty)?;
    eprintln!("Wrote run receipt to {}", args.out.display());

    // Optionally write individual receipts
    if let Some(ref out_dir) = args.out_dir {
        let individual =
            benchmarks_to_individual_receipts(&benchmarks, &tool, &host, &clock, &command_strs)?;
        fs::create_dir_all(out_dir).with_context(|| format!("create dir {}", out_dir.display()))?;

        for r in &individual {
            let safe_name = r.bench.name.replace(['/', '\\'], "_");
            let path = out_dir.join(format!("{safe_name}.json"));
            write_json(&path, r, args.pretty)?;
            eprintln!("  Wrote {}", path.display());
        }
    }

    // Optionally compare against baseline
    if let Some(baseline_path) = &args.compare {
        let baseline: RunReceipt = read_json(baseline_path)?;
        let budgets = BTreeMap::new(); // default budgets
        let metric_statistics = BTreeMap::new();

        let compare_result = perfgate_app::CompareUseCase::execute(perfgate_app::CompareRequest {
            baseline,
            current: receipt.clone(),
            budgets,
            metric_statistics,
            significance: None,
            tradeoffs: Vec::new(),
            baseline_ref: CompareRef {
                path: Some(baseline_path.to_string_lossy().to_string()),
                run_id: None,
            },
            current_ref: CompareRef {
                path: Some(args.out.to_string_lossy().to_string()),
                run_id: None,
            },
            tool: tool.clone(),
            host_mismatch_policy: HostMismatchPolicy::Warn,
        })?;

        // Write compare receipt next to the run receipt
        let compare_out = args.out.with_file_name("perfgate-cargo-bench-compare.json");
        write_json(&compare_out, &compare_result.receipt, args.pretty)?;
        eprintln!("Wrote compare receipt to {}", compare_out.display());

        // Print verdict
        let verdict = &compare_result.receipt.verdict;
        eprintln!(
            "Verdict: {:?} (pass={}, warn={}, fail={})",
            verdict.status, verdict.counts.pass, verdict.counts.warn, verdict.counts.fail
        );

        // Print host mismatch warning if detected
        if let Some(mismatch) = &compare_result.host_mismatch {
            eprintln!(
                "Warning: host mismatch detected: {}",
                mismatch.reasons.join("; ")
            );
        }

        // Print markdown summary
        let md = perfgate_app::render_markdown(&compare_result.receipt);
        println!("{md}");
    }

    Ok(())
}

/// Execute the `init` subcommand.
fn execute_init(args: InitArgs) -> anyhow::Result<()> {
    let preset = match args.preset {
        InitPreset::Standard => Preset::Standard,
        InitPreset::Release => Preset::Release,
        InitPreset::Tier1Fast => Preset::Tier1Fast,
    };

    let ci_platform = args.ci.map(|p| match p {
        InitCiPlatform::Github => CiPlatform::GitHub,
        InitCiPlatform::Gitlab => CiPlatform::GitLab,
        InitCiPlatform::Bitbucket => CiPlatform::Bitbucket,
        InitCiPlatform::Circleci => CiPlatform::CircleCi,
    });

    let scan_dir = if args.dir == Path::new(".") {
        std::env::current_dir().context("cannot determine current directory")?
    } else {
        args.dir.clone()
    };

    // Check if config already exists.
    if args.output.exists() && !args.yes {
        anyhow::bail!(
            "{} already exists; use --yes to overwrite",
            args.output.display()
        );
    }

    eprintln!("Scanning {} for benchmarks...", scan_dir.display());
    let benchmarks = discover_benchmarks(&scan_dir);

    if benchmarks.is_empty() {
        eprintln!("No benchmarks discovered. The generated config will have no [[bench]] entries.");
        eprintln!("You can add them manually to {}.", args.output.display());
    } else {
        eprintln!("Discovered {} benchmark(s):", benchmarks.len());
        for b in &benchmarks {
            eprintln!("  - {} ({})", b.name, b.source);
        }
    }

    let config = generate_config(&benchmarks, preset);
    let toml_content = render_config_toml(&config);

    fs::write(&args.output, &toml_content)
        .with_context(|| format!("write {}", args.output.display()))?;
    eprintln!("Wrote {}", args.output.display());

    // CI workflow
    if let Some(platform) = ci_platform {
        let workflow_path = ci_workflow_path(platform);
        let workflow_content = scaffold_ci(platform, &args.output);

        if let Some(parent) = workflow_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }

        fs::write(&workflow_path, &workflow_content)
            .with_context(|| format!("write {}", workflow_path.display()))?;
        eprintln!("Wrote {}", workflow_path.display());
    }

    eprintln!("\nNext steps:");
    eprintln!("  1. Review and edit {}", args.output.display());
    eprintln!(
        "  2. Run: perfgate check --config {} --all",
        args.output.display()
    );

    Ok(())
}

const DEFAULT_LOCAL_SERVER_URL: &str = "http://127.0.0.1:8484/api/v1";

/// Resolve the local server URL from the PERFGATE_LOCAL_DB environment variable
/// or fall back to the default.
fn resolve_local_db_url() -> String {
    match std::env::var("PERFGATE_LOCAL_DB") {
        Ok(val) if !val.is_empty() && val != "true" && val != "1" => val,
        _ => DEFAULT_LOCAL_SERVER_URL.to_string(),
    }
}

/// Upload a run receipt to the local perfgate server (no auth).
fn upload_to_local_db(name: &str, receipt: &RunReceipt) -> anyhow::Result<()> {
    let url = resolve_local_db_url();
    let config = ClientConfig {
        server_url: url.clone(),
        auth: AuthMethod::None,
        ..Default::default()
    };
    let client = BaselineClient::new(config)
        .with_context(|| format!("create client for local server at {url}"))?;

    let request = UploadBaselineRequest {
        benchmark: name.to_string(),
        version: None,
        git_ref: None,
        git_sha: None,
        receipt: receipt.clone(),
        metadata: BTreeMap::new(),
        tags: Vec::new(),
        normalize: false,
    };

    with_tokio_runtime(async {
        let response: perfgate_client::types::UploadBaselineResponse = client
            .upload_baseline("default", &request)
            .await
            .with_context(|| {
                format!("upload to local server at {url} -- is `perfgate serve` running?")
            })?;
        eprintln!(
            "Uploaded to local server: {} version {}",
            response.benchmark, response.version
        );
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

/// Resolve the default data directory (~/.perfgate/) and ensure it exists.
fn default_data_dir() -> anyhow::Result<PathBuf> {
    let home = if cfg!(windows) {
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .context("could not determine home directory (USERPROFILE / HOME not set)")?
    } else {
        std::env::var("HOME").context("could not determine home directory (HOME not set)")?
    };
    let dir = PathBuf::from(home).join(".perfgate");
    fs::create_dir_all(&dir).with_context(|| format!("create data directory {}", dir.display()))?;
    Ok(dir)
}

/// Start the local dashboard server.
fn execute_serve(args: ServeArgs) -> anyhow::Result<()> {
    let db_path = match args.db {
        Some(p) => p,
        None => default_data_dir()?.join("data.db"),
    };

    // Ensure the parent directory exists for the database file.
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create database directory {}", parent.display()))?;
    }

    let bind_addr = format!("127.0.0.1:{}", args.port);
    let url = format!("http://127.0.0.1:{}", args.port);

    eprintln!("perfgate serve");
    eprintln!("  database : {}", db_path.display());
    eprintln!("  dashboard: {url}");
    eprintln!("  auth     : disabled (local mode)");
    eprintln!();
    eprintln!("Press Ctrl+C to stop.");

    let config = perfgate_server::ServerConfig::new()
        .bind(&bind_addr)?
        .storage_backend(perfgate_server::StorageBackend::Sqlite)
        .sqlite_path(&db_path)
        .local_mode(true)
        .cors(true);

    // Open the browser unless --no-open is passed.
    if !args.no_open {
        open_browser(&url);
    }

    let rt = tokio::runtime::Runtime::new().context("initialize async runtime")?;
    rt.block_on(async {
        perfgate_server::run_server(config)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))
    })?;

    Ok(())
}

/// Open a URL in the default browser.
fn open_browser(url: &str) {
    let result = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn()
    };

    if let Err(e) = result {
        eprintln!("warning: could not open browser: {e}");
    }
}

fn execute_scale(args: ScaleArgs) -> anyhow::Result<()> {
    use std::process::Command as ProcessCommand;

    let ScaleArgs {
        name,
        command,
        sizes,
        repeat,
        expected,
        r_squared_threshold,
        cwd,
        timeout,
        out,
        pretty,
        chart_width,
        chart_height,
    } = args;

    if sizes.len() < 3 {
        anyhow::bail!("--sizes must contain at least 3 values for curve fitting");
    }

    let expected_class = expected
        .as_deref()
        .map(parse_complexity)
        .transpose()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    let timeout_duration = timeout.as_deref().map(parse_duration).transpose()?;

    eprintln!("Scaling analysis: {}", name);
    eprintln!("  Command template: {}", command);
    eprintln!("  Input sizes: {:?}", sizes);
    eprintln!("  Repeat: {}", repeat);
    if let Some(ref ec) = expected_class {
        eprintln!("  Expected complexity: {}", ec);
    }

    let mut measurements = Vec::new();

    for &size in &sizes {
        let cmd_str = command.replace("{n}", &size.to_string());
        eprintln!("\n  Running size={} ...", size);

        let mut wall_times = Vec::new();
        for iteration in 0..repeat {
            let argv = shell_words::split(&cmd_str)
                .with_context(|| format!("failed to parse command: {}", cmd_str))?;
            if argv.is_empty() {
                anyhow::bail!("command is empty after substituting size={}", size);
            }

            let mut proc = ProcessCommand::new(&argv[0]);
            proc.args(&argv[1..]);

            if let Some(ref dir) = cwd {
                proc.current_dir(dir);
            }

            let start = Instant::now();
            let status = proc
                .status()
                .with_context(|| format!("failed to execute: {}", cmd_str))?;
            let elapsed = start.elapsed();

            if !status.success() {
                anyhow::bail!(
                    "command failed with {} at size={}, iteration={}",
                    status,
                    size,
                    iteration + 1
                );
            }

            // Check timeout
            if let Some(limit) = timeout_duration
                && elapsed > limit
            {
                anyhow::bail!(
                    "command timed out ({}ms > {}ms) at size={}, iteration={}",
                    elapsed.as_millis(),
                    limit.as_millis(),
                    size,
                    iteration + 1,
                );
            }

            wall_times.push(elapsed.as_secs_f64() * 1000.0);
        }

        // Use median of wall times
        wall_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if wall_times.len() % 2 == 1 {
            wall_times[wall_times.len() / 2]
        } else {
            let mid = wall_times.len() / 2;
            (wall_times[mid - 1] + wall_times[mid]) / 2.0
        };

        eprintln!(
            "    size={}: median={:.2}ms (n={})",
            size,
            median,
            wall_times.len()
        );

        measurements.push(SizeMeasurement {
            input_size: size,
            time_ms: median,
        });
    }

    // Classify complexity
    let result = classify_complexity(&measurements, Some(r_squared_threshold))
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Build report
    let report = ScalingReport::new(
        name.clone(),
        command.clone(),
        sizes.clone(),
        repeat,
        expected_class,
        measurements.clone(),
        result.clone(),
    );

    // Print results
    eprintln!("\n{}", "=".repeat(60));
    eprintln!("Scaling Analysis Results");
    eprintln!("{}", "=".repeat(60));
    eprintln!("Benchmark: {}", name);
    eprintln!("Detected complexity: {}", result.best_fit);
    eprintln!("R-squared: {:.4}", result.r_squared);
    if let Some(ec) = expected_class {
        eprintln!("Expected: {}", ec);
    }
    eprintln!("Verdict: {}", report.verdict);

    // Print all model fits
    eprintln!("\nModel fits (sorted by R-squared):");
    for (class, r2) in &result.all_fits {
        let marker = if *class == result.best_fit {
            " <-- best"
        } else {
            ""
        };
        eprintln!("  {:<12} R^2={:.4}{}", class.to_string(), r2, marker);
    }

    // Print ASCII chart
    eprintln!();
    let chart = render_ascii_chart(
        &measurements,
        result.best_fit,
        &result.coefficients,
        chart_width,
        chart_height,
    );
    eprintln!("{}", chart);

    // Write JSON report
    write_json(&out, &report, pretty)?;
    eprintln!("\nReport written to: {}", out.display());

    if !report.pass {
        // Exit code 2 for policy failure
        std::process::exit(2);
    }

    Ok(())
}

/// Execute the `comment` subcommand: post or update a PR comment on GitHub.
fn execute_comment(args: CommentArgs) -> anyhow::Result<()> {
    let CommentArgs {
        compare,
        report,
        github_token,
        repo,
        pr,
        github_api_url,
        blame_text,
        dry_run,
    } = args;

    // Load the receipt data
    let comment_body = if let Some(compare_path) = compare {
        let receipt: CompareReceipt = read_json(&compare_path)?;
        let options = CommentOptions {
            blame_text,
            explain_text: None,
        };
        perfgate_github::render_comment(&receipt, &options)
    } else if let Some(report_path) = report {
        let report_receipt: PerfgateReport = read_json(&report_path)?;
        let options = CommentOptions {
            blame_text,
            explain_text: None,
        };
        perfgate_github::render_comment_from_report(&report_receipt, &options)
    } else {
        anyhow::bail!("Either --compare or --report is required");
    };

    // Dry-run: print and exit
    if dry_run {
        println!("{}", comment_body);
        return Ok(());
    }

    // Resolve GitHub token
    let token = github_token
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "GitHub token is required. Use --github-token or set GITHUB_TOKEN env var."
            )
        })?;

    // Resolve repo (owner/repo)
    let repo_str = repo
        .or_else(|| std::env::var("GITHUB_REPOSITORY").ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Repository is required. Use --repo owner/repo or set GITHUB_REPOSITORY env var."
            )
        })?;

    let (owner, repo_name) =
        perfgate_github::parse_github_repository(&repo_str).ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid repository format: '{}'. Expected owner/repo.",
                repo_str
            )
        })?;

    // Resolve PR number
    let pr_number = pr
        .or_else(|| {
            std::env::var("GITHUB_REF")
                .ok()
                .and_then(|r| perfgate_github::parse_pr_number_from_ref(&r))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "PR number is required. Use --pr NUMBER or set GITHUB_REF env var (refs/pull/N/merge)."
            )
        })?;

    // Post/update comment via GitHub API
    let client = GitHubClient::new(&github_api_url, &token)
        .map_err(|e| anyhow::anyhow!("Failed to create GitHub client: {}", e))?;

    with_tokio_runtime(async {
        let (comment, created) = client
            .upsert_comment(&owner, &repo_name, pr_number, &comment_body)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to post comment: {}", e))?;

        if created {
            eprintln!("Created perfgate comment: {}", comment.html_url);
        } else {
            eprintln!("Updated perfgate comment: {}", comment.html_url);
        }

        Ok(())
    })
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

fn execute_fleet_action(action: FleetAction, server_flags: &ServerFlags) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;

    let client = server_config
        .create_client()?
        .ok_or_else(|| anyhow::anyhow!(BASELINE_SERVER_NOT_CONFIGURED))?;
    let rt = tokio::runtime::Runtime::new()?;

    match action {
        FleetAction::Alerts {
            min_affected,
            limit,
        } => {
            rt.block_on(async {
                let query = perfgate_client::ListFleetAlertsQuery {
                    min_affected,
                    since: None,
                    limit,
                };
                let response = client
                    .list_fleet_alerts(&query)
                    .await
                    .context("Failed to list fleet alerts")?;

                if response.alerts.is_empty() {
                    println!("No fleet-wide dependency regression alerts.");
                } else {
                    println!("Fleet Alerts ({} found):\n", response.alerts.len());
                    for alert in &response.alerts {
                        let ver_change = format!(
                            "{} -> {}",
                            alert.old_version.as_deref().unwrap_or("(none)"),
                            alert.new_version.as_deref().unwrap_or("(none)")
                        );
                        println!(
                            "  {} ({})  confidence={:.0}%  avg_delta={:+.1}%",
                            alert.dependency,
                            ver_change,
                            alert.confidence * 100.0,
                            alert.avg_delta_pct
                        );
                        println!("    Affected projects ({}):", alert.affected_projects.len());
                        for p in &alert.affected_projects {
                            println!(
                                "      - {}/{}: {:+.1}% ({})",
                                p.project, p.benchmark, p.delta_pct, p.metric
                            );
                        }
                        println!();
                    }
                }
                Ok::<(), anyhow::Error>(())
            })?;
        }

        FleetAction::Impact { dependency, limit } => {
            rt.block_on(async {
                let query = perfgate_client::DependencyImpactQuery { since: None, limit };
                let response = client
                    .dependency_impact(&dependency, &query)
                    .await
                    .with_context(|| {
                        format!("Failed to get impact for dependency '{}'", dependency)
                    })?;

                println!(
                    "Dependency: {}  ({} projects, avg delta: {:+.1}%)\n",
                    response.dependency, response.project_count, response.avg_delta_pct
                );
                if response.events.is_empty() {
                    println!("  No recorded events.");
                } else {
                    for event in &response.events {
                        let ver = format!(
                            "{} -> {}",
                            event.old_version.as_deref().unwrap_or("(none)"),
                            event.new_version.as_deref().unwrap_or("(none)")
                        );
                        println!(
                            "  {}/{}: {:+.1}% ({}) [{}]",
                            event.project, event.benchmark, event.delta_pct, event.metric, ver
                        );
                    }
                }
                Ok::<(), anyhow::Error>(())
            })?;
        }

        FleetAction::RecordEvent {
            project,
            benchmark,
            compare,
            baseline_lock,
            current_lock,
            metric,
        } => {
            // Parse compare receipt to extract delta
            let compare_receipt: CompareReceipt = read_json(&compare)?;

            // Extract the delta for the requested metric
            let delta_pct = compare_receipt
                .deltas
                .iter()
                .find(|(m, _)| m.as_str() == metric)
                .map(|(_, d)| d.pct)
                .unwrap_or(0.0);

            // Parse lockfiles to find dependency changes
            let baseline_content = fs::read_to_string(&baseline_lock)
                .with_context(|| format!("read baseline lockfile {:?}", baseline_lock))?;
            let current_content = fs::read_to_string(&current_lock)
                .with_context(|| format!("read current lockfile {:?}", current_lock))?;

            let blame = perfgate_domain::compare_lockfiles(&baseline_content, &current_content);

            if blame.changes.is_empty() {
                println!("No dependency changes detected.");
                return Ok(());
            }

            let dep_changes: Vec<perfgate_client::DependencyChange> = blame
                .changes
                .iter()
                .filter(|c| c.change_type == DependencyChangeType::Updated)
                .map(|c| perfgate_client::DependencyChange {
                    name: c.name.clone(),
                    old_version: c.old_version.clone(),
                    new_version: c.new_version.clone(),
                })
                .collect();

            if dep_changes.is_empty() {
                println!("No dependency version updates detected (only adds/removes).");
                return Ok(());
            }

            let request = perfgate_client::RecordDependencyEventRequest {
                project: project.clone(),
                benchmark: benchmark.clone(),
                dependency_changes: dep_changes,
                metric: metric.clone(),
                delta_pct,
            };

            rt.block_on(async {
                let response = client
                    .record_dependency_event(&request)
                    .await
                    .context("Failed to record dependency events")?;

                println!(
                    "Recorded {} dependency event(s) for {}/{} (delta: {:+.1}%)",
                    response.recorded, project, benchmark, delta_pct
                );
                Ok::<(), anyhow::Error>(())
            })?;
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
    profile_on_regression: bool,
    emit_repair_context: bool,
    server_flags: ServerFlags,
    local_db: bool,
}

/// Returns true if the verdict indicates a regression (warn or fail).
fn is_regression(status: VerdictStatus) -> bool {
    matches!(status, VerdictStatus::Warn | VerdictStatus::Fail)
}

/// Attempt to capture a flamegraph for a regressing benchmark.
///
/// This is a best-effort operation: failures are reported to stderr
/// but do not affect the exit code.
fn try_capture_flamegraph(command: &[String], cwd: Option<&str>, label: &str, out_dir: &Path) {
    let profiles_dir = out_dir.join("profiles");
    let request = ProfileRequest {
        command: command.to_vec(),
        output_dir: profiles_dir,
        label: label.to_string(),
        cwd: cwd.map(PathBuf::from),
        env: Vec::new(),
    };

    match capture_flamegraph(&request) {
        Ok(Some(result)) => {
            eprintln!(
                "flamegraph captured: {} (profiler: {}, {}ms)",
                result.svg_path.display(),
                result.profiler_used,
                result.duration_ms
            );
        }
        Ok(None) => {
            // No profiler available; diagnostic already printed by capture_flamegraph.
        }
        Err(e) => {
            eprintln!("warning: flamegraph capture failed: {e}");
        }
    }
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

        if let Err(e) = with_tokio_runtime(async {
            client
                .submit_verdict(&project, &request)
                .await
                .map_err(anyhow::Error::from)
        }) {
            eprintln!("warning: failed to submit verdict: {:#}", e);
        }
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

        // Upload to local server if --local-db is set
        if req.local_db
            && let Err(e) = upload_to_local_db(bench_name, &outcome.run_receipt)
        {
            eprintln!("warning: local-db upload failed: {:#}", e);
        }

        // Write artifacts
        write_check_artifacts(&outcome, req.pretty)
            .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;

        maybe_write_repair_context(
            &outcome,
            Some(&baseline_path),
            req.emit_repair_context,
            req.pretty,
        )
        .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;

        // Profile on regression if requested
        if req.profile_on_regression
            && let Some(compare) = &outcome.compare_receipt
            && is_regression(compare.verdict.status)
        {
            try_capture_flamegraph(
                &compare.bench.command,
                compare.bench.cwd.as_deref(),
                &compare.bench.name,
                &bench_out_dir,
            );
        }

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

            // Upload to local server if --local-db is set
            if req.local_db
                && let Err(e) = upload_to_local_db(bench_name, &check_outcome.run_receipt)
            {
                eprintln!("warning: local-db upload failed: {:#}", e);
            }

            // Write native artifacts to extras/
            write_check_artifacts(&check_outcome, req.pretty)
                .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;

            maybe_write_repair_context(
                &check_outcome,
                Some(&baseline_path),
                req.emit_repair_context,
                req.pretty,
            )
            .map_err(|e| PerfgateError::Io(IoError::ArtifactWrite(e.to_string())))?;

            // Profile on regression if requested
            if req.profile_on_regression
                && let Some(compare) = &check_outcome.compare_receipt
                && is_regression(compare.verdict.status)
            {
                try_capture_flamegraph(
                    &compare.bench.command,
                    compare.bench.cwd.as_deref(),
                    &compare.bench.name,
                    &extras_dir,
                );
            }

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

/// Run the watch command: monitor filesystem and re-run benchmarks on changes.
fn run_watch(args: WatchArgs) -> anyhow::Result<()> {
    use notify::{EventKind, RecursiveMode, Watcher};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    let WatchArgs {
        config: config_path,
        bench,
        all,
        debounce,
        no_clear,
        watch_dir,
        host_mismatch,
        env,
    } = args;

    // Load and validate config
    let config_content = fs::read_to_string(&config_path)
        .with_context(|| format!("read {}", config_path.display()))?;

    let config_file: ConfigFile = if config_path
        .extension()
        .map(|e| e == "json")
        .unwrap_or(false)
    {
        serde_json::from_str(&config_content)
            .with_context(|| format!("parse JSON config {}", config_path.display()))?
    } else {
        toml::from_str(&config_content)
            .with_context(|| format!("parse TOML config {}", config_path.display()))?
    };

    config_file
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;

    // Determine which bench to run
    let bench_name = if all {
        // For --all, we run each bench sequentially on each change
        None
    } else if let Some(name) = bench {
        Some(name)
    } else if config_file.benches.len() == 1 {
        // If only one bench, use it automatically
        Some(config_file.benches[0].name.clone())
    } else {
        anyhow::bail!(
            "multiple benchmarks in config; specify --bench <name> or --all\navailable: {}",
            config_file
                .benches
                .iter()
                .map(|b| b.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    };

    let bench_names: Vec<String> = if let Some(name) = &bench_name {
        // Verify the bench exists
        if !config_file.benches.iter().any(|b| b.name == *name) {
            anyhow::bail!(
                "bench '{}' not found in config; available: {}",
                name,
                config_file
                    .benches
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        vec![name.clone()]
    } else {
        config_file.benches.iter().map(|b| b.name.clone()).collect()
    };

    let display_name = bench_name.as_deref().unwrap_or("all").to_string();

    // Setup Ctrl+C handler
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .with_context(|| "failed to set Ctrl+C handler")?;

    // Setup file watcher
    let (tx, rx) = mpsc::channel();
    let mut watcher =
        notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                        let _ = tx.send(());
                    }
                    _ => {}
                }
            }
        })
        .with_context(|| "failed to create file watcher")?;

    // Watch directories
    let watch_dirs = if watch_dir.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        watch_dir
    };

    for dir in &watch_dirs {
        watcher
            .watch(dir.as_ref(), RecursiveMode::Recursive)
            .with_context(|| format!("failed to watch directory: {}", dir.display()))?;
    }

    // Temporary output directory for watch artifacts
    let out_dir = PathBuf::from("artifacts/perfgate-watch");
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("create watch output dir {}", out_dir.display()))?;

    let mut state = WatchState::new();
    let mut debouncer = Debouncer::new(debounce);

    eprintln!(
        "perfgate watch: monitoring {} for changes (debounce: {}ms)",
        watch_dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        debounce
    );
    eprintln!("press Ctrl+C to stop\n");

    let ctx = WatchIterationCtx {
        config: &config_file,
        bench_names: &bench_names,
        out_dir: &out_dir,
        env: &env,
        host_mismatch_policy: host_mismatch,
        no_clear,
        display_name: &display_name,
    };

    // Initial run
    run_watch_iteration(&ctx, &mut state);

    // Main watch loop
    while running.load(Ordering::SeqCst) {
        // Check for file events (non-blocking with timeout)
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(()) => {
                debouncer.event();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Drain any queued events
        while rx.try_recv().is_ok() {
            debouncer.event();
        }

        // Check if debounce has settled
        if debouncer.should_trigger() {
            run_watch_iteration(&ctx, &mut state);
        }
    }

    // Print summary on exit
    eprintln!("\n--- watch summary ---");
    eprintln!(
        "iterations: {} | pass: {} | warn: {} | fail: {}",
        state.iteration_count, state.pass_count, state.warn_count, state.fail_count
    );

    Ok(())
}

/// Context for a watch iteration (avoids too many function arguments).
struct WatchIterationCtx<'a> {
    config: &'a ConfigFile,
    bench_names: &'a [String],
    out_dir: &'a Path,
    env: &'a [(String, String)],
    host_mismatch_policy: HostMismatchPolicy,
    no_clear: bool,
    display_name: &'a str,
}

/// Execute one watch iteration: run all requested benchmarks and update display.
fn run_watch_iteration(ctx: &WatchIterationCtx<'_>, state: &mut WatchState) {
    for bench_name in ctx.bench_names {
        let bench_out_dir = if ctx.bench_names.len() > 1 {
            ctx.out_dir.join(bench_name)
        } else {
            ctx.out_dir.to_path_buf()
        };

        if let Err(e) = fs::create_dir_all(&bench_out_dir) {
            eprintln!("error: create dir {}: {}", bench_out_dir.display(), e);
            continue;
        }

        // Resolve baseline
        let baseline_path =
            perfgate_app::baseline_resolve::resolve_baseline_path(&None, bench_name, ctx.config);
        let baseline = load_optional_baseline_receipt(&baseline_path)
            .ok()
            .flatten();

        let request = WatchRunRequest {
            config: ctx.config.clone(),
            bench_name: bench_name.clone(),
            out_dir: bench_out_dir,
            baseline,
            baseline_path: Some(baseline_path),
            tool: tool_info(),
            env: ctx.env.to_vec(),
            output_cap_bytes: 8192,
            host_mismatch_policy: ctx.host_mismatch_policy,
        };

        // Print status
        if !ctx.no_clear {
            // ANSI clear screen
            print!("\x1b[2J\x1b[H");
        }

        let lines = render_watch_display(state, ctx.display_name, "running...");
        for line in &lines {
            println!("{}", line);
        }

        let runner = StdProcessRunner;
        let host_probe = StdHostProbe;
        let clock = SystemClock;

        match execute_watch_run(runner, host_probe, clock, &request) {
            Ok(result) => {
                state.update(result);
            }
            Err(err) => {
                eprintln!("error running bench '{}': {:#}", bench_name, err);
            }
        }
    }

    // Final display after all benches
    if !ctx.no_clear {
        print!("\x1b[2J\x1b[H");
    }
    let lines = render_watch_display(state, ctx.display_name, "idle (watching)");
    for line in &lines {
        println!("{}", line);
    }
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

fn maybe_write_repair_context(
    outcome: &CheckOutcome,
    baseline_path: Option<&Path>,
    emit_requested: bool,
    pretty: bool,
) -> anyhow::Result<()> {
    let should_emit = emit_requested
        || matches!(
            outcome.report.verdict.status,
            VerdictStatus::Warn | VerdictStatus::Fail
        );
    if !should_emit {
        return Ok(());
    }

    let repair = build_repair_context(outcome, baseline_path);
    let out_path = outcome
        .run_path
        .parent()
        .unwrap_or(Path::new(""))
        .join("repair_context.json");
    write_json(&out_path, &repair, pretty)?;
    Ok(())
}

fn build_repair_context(
    outcome: &CheckOutcome,
    baseline_path: Option<&Path>,
) -> RepairContextReceipt {
    let breached_metrics = if let Some(compare) = &outcome.compare_receipt {
        compare
            .deltas
            .iter()
            .filter_map(|(metric, delta)| {
                if !matches!(delta.status.as_str(), "warn" | "fail" | "skip") {
                    return None;
                }
                let budget = compare.budgets.get(metric)?;
                Some(RepairMetricBreach {
                    metric: *metric,
                    status: delta.status.as_str().to_string(),
                    baseline: delta.baseline,
                    current: delta.current,
                    regression: delta.regression,
                    fail_threshold: budget.threshold,
                    warn_threshold: budget.warn_threshold,
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    let compare_path = outcome
        .compare_path
        .as_ref()
        .map(|p| p.display().to_string());
    let report_path = outcome.report_path.display().to_string();
    let profile_path = outcome.report.profile_path.clone();
    let otel_span = otel_span_from_env();
    let git = git_metadata();
    let changed_files = changed_files_summary();
    let suggested = recommended_next_commands(outcome, baseline_path);

    RepairContextReceipt {
        schema: REPAIR_CONTEXT_SCHEMA_V1.to_string(),
        benchmark: outcome.run_receipt.bench.name.clone(),
        verdict: outcome.report.verdict.clone(),
        status: outcome.report.verdict.status,
        breached_metrics,
        compare_receipt_path: compare_path,
        report_path,
        profile_path,
        git,
        changed_files,
        otel_span,
        recommended_next_commands: suggested,
    }
}

fn recommended_next_commands(outcome: &CheckOutcome, baseline_path: Option<&Path>) -> Vec<String> {
    let mut cmds = Vec::new();
    let rerun_cmd = redact_command_for_diagnostics(&outcome.run_receipt.bench.command).join(" ");
    if !rerun_cmd.is_empty() {
        cmds.push(format!("rerun current command: {rerun_cmd}"));
    }
    if let Some(compare_path) = &outcome.compare_path {
        cmds.push(format!(
            "perfgate explain --compare {}",
            compare_path.display()
        ));
    }
    cmds.push(format!(
        "perfgate paired --name {} --baseline-cmd \"<baseline-cmd>\" --current-cmd \"<current-cmd>\" --repeat {} --out {}/paired.json",
        outcome.run_receipt.bench.name,
        outcome.run_receipt.bench.repeat.max(10),
        outcome.run_path.parent().unwrap_or(Path::new("")).display()
    ));
    if let Some(base) = baseline_path {
        cmds.push(format!(
            "perfgate compare --baseline {} --current {} --out {}/recompare.json",
            base.display(),
            outcome.run_path.display(),
            outcome.run_path.parent().unwrap_or(Path::new("")).display()
        ));
    }
    cmds.push(
        "perfgate bisect --good <good-ref> --bad HEAD --executable <bench-binary>".to_string(),
    );
    cmds
}

fn otel_span_from_env() -> Option<OtelSpanIdentifiers> {
    let trace_id = std::env::var("OTEL_TRACE_ID").ok();
    let span_id = std::env::var("OTEL_SPAN_ID").ok();
    if trace_id.is_none() && span_id.is_none() {
        None
    } else {
        Some(OtelSpanIdentifiers { trace_id, span_id })
    }
}

fn git_metadata() -> Option<RepairGitMetadata> {
    let branch = run_git_capture(&["rev-parse", "--abbrev-ref", "HEAD"]);
    let sha = run_git_capture(&["rev-parse", "HEAD"]);
    if branch.is_none() && sha.is_none() {
        None
    } else {
        Some(RepairGitMetadata { branch, sha })
    }
}

fn changed_files_summary() -> Option<ChangedFilesSummary> {
    let output = run_git_capture_bytes(&["status", "--porcelain", "-z"])?;
    Some(parse_changed_files_summary(&output))
}

fn parse_changed_files_summary(output: &[u8]) -> ChangedFilesSummary {
    let mut files = Vec::new();
    let mut by_top = BTreeMap::new();

    let mut entries = output
        .split(|byte| *byte == b'\0')
        .filter(|entry| !entry.is_empty());
    while let Some(entry) = entries.next() {
        if entry.len() <= 3 {
            continue;
        }

        let status = &entry[..2];
        let current_path = if status.iter().any(|code| matches!(code, b'R' | b'C')) {
            entries.next().unwrap_or(&[])
        } else {
            &entry[3..]
        };

        if current_path.is_empty() {
            continue;
        }

        let path = String::from_utf8_lossy(current_path).into_owned();
        files.push(path.clone());
        let top = path
            .split(['/', '\\'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or(".")
            .to_string();
        *by_top.entry(top).or_insert(0) += 1;
    }

    ChangedFilesSummary {
        file_count: files.len() as u32,
        files,
        file_count_by_top_level: by_top,
    }
}

fn run_git_capture(args: &[&str]) -> Option<String> {
    let output = ProcessCommand::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn run_git_capture_bytes(args: &[&str]) -> Option<Vec<u8>> {
    let output = ProcessCommand::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
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

fn parse_aggregation_policy(s: &str) -> Result<AggregationPolicy, String> {
    match s {
        "all" => Ok(AggregationPolicy::All),
        "majority" => Ok(AggregationPolicy::Majority),
        "weighted" => Ok(AggregationPolicy::Weighted),
        "quorum" => Ok(AggregationPolicy::Quorum),
        "fail_if_n_of_m" => Ok(AggregationPolicy::FailIfNOfM),
        _ => Err(format!(
            "invalid aggregation policy: {s} (expected all|majority|weighted|quorum|fail_if_n_of_m)"
        )),
    }
}

fn parse_weight_map(weights: &[String]) -> anyhow::Result<BTreeMap<String, f64>> {
    let mut map = BTreeMap::new();
    for raw in weights {
        let (label, weight_raw) = raw
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --weight '{raw}', expected label=value"))?;
        if label.trim().is_empty() {
            anyhow::bail!("invalid --weight '{raw}': label cannot be empty");
        }
        let weight: f64 = weight_raw
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid --weight '{raw}': weight must be a number"))?;
        if !weight.is_finite() || weight < 0.0 {
            anyhow::bail!("invalid --weight '{raw}': weight must be a non-negative finite number");
        }
        map.insert(label.trim().to_string(), weight);
    }
    Ok(map)
}

fn validate_aggregate_options(
    policy: AggregationPolicy,
    quorum: Option<f64>,
    fail_n: Option<u32>,
    fail_m: Option<u32>,
) -> anyhow::Result<(Option<f64>, Option<FailIfNOfM>)> {
    if let Some(quorum) = quorum {
        if !quorum.is_finite() || !(0.0..=1.0).contains(&quorum) {
            anyhow::bail!("--quorum must be between 0.0 and 1.0, got {quorum}");
        }
        if !matches!(
            policy,
            AggregationPolicy::Weighted | AggregationPolicy::Quorum
        ) {
            anyhow::bail!("--quorum requires --policy weighted or quorum");
        }
    }

    match policy {
        AggregationPolicy::FailIfNOfM => {
            let n = fail_n
                .ok_or_else(|| anyhow::anyhow!("--policy fail_if_n_of_m requires --fail-n"))?;
            if n == 0 {
                anyhow::bail!("--fail-n must be at least 1");
            }
            if let Some(m) = fail_m {
                if m == 0 {
                    anyhow::bail!("--fail-m must be at least 1");
                }
                if m < n {
                    anyhow::bail!("--fail-m must be greater than or equal to --fail-n");
                }
            }
            Ok((quorum, Some(FailIfNOfM { n, m: fail_m })))
        }
        _ => {
            if fail_n.is_some() || fail_m.is_some() {
                anyhow::bail!("--fail-n and --fail-m require --policy fail_if_n_of_m");
            }
            Ok((quorum, None))
        }
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
    fn parse_weight_map_accepts_non_negative_weights_above_one() {
        let weights = parse_weight_map(&[
            "linux-x86_64=2.5".to_string(),
            "macos-aarch64=0".to_string(),
        ])
        .unwrap();

        assert_eq!(weights.get("linux-x86_64"), Some(&2.5));
        assert_eq!(weights.get("macos-aarch64"), Some(&0.0));
    }

    #[test]
    fn parse_weight_map_rejects_negative_weights() {
        let err = parse_weight_map(&["linux-x86_64=-0.1".to_string()]).unwrap_err();
        assert!(
            err.to_string().contains("non-negative finite number"),
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
            complexity: None,
            profile_path: None,
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
            suggest_paired: false,
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
            complexity: None,
            profile_path: None,
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
            suggest_paired: false,
        };

        write_check_artifacts(&outcome, false).unwrap();

        assert!(run_path.exists());
        assert!(report_path.exists());
        assert!(markdown_path.exists());
    }

    #[test]
    fn maybe_write_repair_context_emits_on_fail() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();

        let compare_receipt: CompareReceipt =
            serde_json::from_str(&create_compare_receipt_json("fail", "fail")).unwrap();

        let outcome = CheckOutcome {
            run_receipt: make_receipt(make_stats_with_wall(100)),
            run_path: out_dir.join("run.json"),
            compare_receipt: Some(compare_receipt),
            compare_path: Some(out_dir.join("compare.json")),
            report: PerfgateReport {
                report_type: "perfgate.report.v1".to_string(),
                verdict: Verdict {
                    status: VerdictStatus::Fail,
                    counts: VerdictCounts {
                        pass: 0,
                        warn: 0,
                        fail: 1,
                        skip: 0,
                    },
                    reasons: vec!["wall_ms.fail".to_string()],
                },
                compare: None,
                findings: Vec::new(),
                summary: ReportSummary {
                    pass_count: 0,
                    warn_count: 0,
                    fail_count: 1,
                    skip_count: 0,
                    total_count: 1,
                },
                complexity: None,
                profile_path: Some("profiles/bench.svg".to_string()),
            },
            report_path: out_dir.join("report.json"),
            markdown: String::new(),
            markdown_path: out_dir.join("comment.md"),
            warnings: Vec::new(),
            failed: true,
            exit_code: 2,
            suggest_paired: false,
        };

        maybe_write_repair_context(&outcome, None, false, true).unwrap();

        assert!(out_dir.join("repair_context.json").exists());
    }

    #[test]
    fn maybe_write_repair_context_omits_on_pass_without_flag() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();

        let outcome = CheckOutcome {
            run_receipt: make_receipt(make_stats_with_wall(100)),
            run_path: out_dir.join("run.json"),
            compare_receipt: None,
            compare_path: None,
            report: PerfgateReport {
                report_type: "perfgate.report.v1".to_string(),
                verdict: Verdict {
                    status: VerdictStatus::Pass,
                    counts: VerdictCounts {
                        pass: 1,
                        warn: 0,
                        fail: 0,
                        skip: 0,
                    },
                    reasons: Vec::new(),
                },
                compare: None,
                findings: Vec::new(),
                summary: ReportSummary {
                    pass_count: 1,
                    warn_count: 0,
                    fail_count: 0,
                    skip_count: 0,
                    total_count: 1,
                },
                complexity: None,
                profile_path: None,
            },
            report_path: out_dir.join("report.json"),
            markdown: String::new(),
            markdown_path: out_dir.join("comment.md"),
            warnings: Vec::new(),
            failed: false,
            exit_code: 0,
            suggest_paired: false,
        };

        maybe_write_repair_context(&outcome, None, false, true).unwrap();

        assert!(!out_dir.join("repair_context.json").exists());
    }

    #[test]
    fn maybe_write_repair_context_emits_on_pass_with_flag() {
        let dir = tempdir().unwrap();
        let out_dir = dir.path().join("out");
        fs::create_dir_all(&out_dir).unwrap();

        let outcome = CheckOutcome {
            run_receipt: make_receipt(make_stats_with_wall(100)),
            run_path: out_dir.join("run.json"),
            compare_receipt: None,
            compare_path: None,
            report: PerfgateReport {
                report_type: "perfgate.report.v1".to_string(),
                verdict: Verdict {
                    status: VerdictStatus::Pass,
                    counts: VerdictCounts {
                        pass: 1,
                        warn: 0,
                        fail: 0,
                        skip: 0,
                    },
                    reasons: Vec::new(),
                },
                compare: None,
                findings: Vec::new(),
                summary: ReportSummary {
                    pass_count: 1,
                    warn_count: 0,
                    fail_count: 0,
                    skip_count: 0,
                    total_count: 1,
                },
                complexity: None,
                profile_path: None,
            },
            report_path: out_dir.join("report.json"),
            markdown: String::new(),
            markdown_path: out_dir.join("comment.md"),
            warnings: Vec::new(),
            failed: false,
            exit_code: 0,
            suggest_paired: false,
        };

        maybe_write_repair_context(&outcome, None, true, true).unwrap();

        assert!(out_dir.join("repair_context.json").exists());
    }

    #[test]
    fn parse_changed_files_summary_keeps_spaces_and_renames() {
        let output = b"M  crates/perfgate-cli/src/main.rs\0R  docs/old name.md\0docs/new name.md\0?? fixtures/file with spaces.json\0";

        let summary = parse_changed_files_summary(output);

        assert_eq!(summary.file_count, 3);
        assert_eq!(
            summary.files,
            vec![
                "crates/perfgate-cli/src/main.rs".to_string(),
                "docs/new name.md".to_string(),
                "fixtures/file with spaces.json".to_string(),
            ]
        );
        assert_eq!(summary.file_count_by_top_level.get("crates"), Some(&1));
        assert_eq!(summary.file_count_by_top_level.get("docs"), Some(&1));
        assert_eq!(summary.file_count_by_top_level.get("fixtures"), Some(&1));
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
