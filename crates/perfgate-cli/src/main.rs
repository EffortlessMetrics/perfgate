//! perfgate CLI - entry point for all workflows.

mod baseline;
mod storage;

use storage::{
    atomic_write, load_optional_baseline_receipt, location_exists, read_json,
    read_json_from_location, with_tokio_runtime, write_json, write_json_to_location,
};

use anyhow::Context;
use baseline::{BaselineSelector, parse_baseline_selector};
use clap::{Args, Parser, Subcommand, ValueEnum};
use glob::glob;
use perfgate::app as perfgate_app;
use perfgate::domain as perfgate_domain;
use perfgate::integrations::github::{self, CommentOptions, GitHubClient};
use perfgate::integrations::ingest::{self, IngestFormat};
use perfgate::runtime::profile::{ProfileRequest, capture_flamegraph};
use perfgate::runtime::{HostProbe, HostProbeOptions, StdHostProbe, StdProcessRunner};
use perfgate_app::baseline_resolve::{is_remote_storage_uri, resolve_baseline_path};
use perfgate_app::comparison_logic::{build_budgets, build_metric_statistics, verdict_from_counts};
use perfgate_app::init::{
    CiPlatform, Preset, ci_workflow_path, discover_benchmarks, generate_config, render_config_toml,
    render_onboarding_readme, scaffold_ci,
};
use perfgate_app::render::summary::{SummaryRequest, SummaryUseCase};
use perfgate_app::{
    BadgeInput, BadgeStyle, BadgeType, BadgeUseCase, BenchOutcome, BisectRequest, BisectUseCase,
    BlameRequest, BlameUseCase, CheckOutcome, CheckRequest, CheckUseCase, Clock, CompareRequest,
    CompareUseCase, DiffRequest, DiffUseCase, ExplainRequest, ExplainUseCase, ExportFormat,
    ExportUseCase, PairedRunRequest, PairedRunUseCase, ProbeCompareRequest, ProbeCompareUseCase,
    PromoteRequest, PromoteUseCase, RatchetUseCase, ReportRequest, ReportUseCase, RunBenchRequest,
    RunBenchUseCase, ScenarioEvaluateInput, ScenarioEvaluateRequest, ScenarioUseCase,
    SensorReportBuilder, SystemClock, TradeoffEvaluateRequest, TradeoffUseCase, classify_error,
    github_annotations, is_host_mismatch_reason, preview_lines, redact_command_for_diagnostics,
    render_json_diff, render_markdown, render_markdown_template, render_terminal_diff,
    render_tradeoff_markdown,
    watch::{Debouncer, WatchRunRequest, WatchState, execute_watch_run, render_watch_display},
};
use perfgate_client::types::auth::Role;
use perfgate_client::types::{BaselineRecord, CreateKeyRequest, KeyEntry};
use perfgate_client::{
    AuthMethod, BaselineClient, ClientConfig, ListAuditEventsQuery, ListAuditEventsResponse,
    ListBaselinesQuery, ListDecisionsQuery, ListVerdictsQuery, PruneDecisionsRequest,
    ResolvedServerConfig, RetryConfig, SubmitVerdictRequest, UploadBaselineRequest,
    UploadDecisionRequest, resolve_server_config,
};
use perfgate_domain::scaling::{
    ScalingReport, SizeMeasurement, classify_complexity, parse_complexity, render_ascii_chart,
};
use perfgate_domain::{DependencyChangeType, SignificancePolicy};
use perfgate_types::config::{
    apply_ratchet_toml_changes, load_config_file, preview_ratchet_toml_changes,
};
use perfgate_types::error::{AdapterError, ConfigValidationError, IoError, PerfgateError};
use perfgate_types::fingerprint::sha256_hex;
use perfgate_types::{
    AggregateWeightMode, AggregationPolicy, BASELINE_REASON_NO_BASELINE, BaselineServerConfig,
    ChangedFilesSummary, CompareReceipt, CompareRef, ConfigFile, DECISION_BUNDLE_SCHEMA_V1,
    DECISION_INDEX_SCHEMA_V1, DecisionArtifactIndex, DecisionBundleArtifact,
    DecisionBundleArtifactContent, DecisionBundleArtifactKind, DecisionBundleMetadata,
    DecisionBundleReceipt, FailIfNOfM, HostMismatchPolicy, Metric, MetricStatus,
    OtelSpanIdentifiers, PerfgateReport, ProbeCompareReceipt, ProbeReceipt,
    REPAIR_CONTEXT_SCHEMA_V1, RatchetConfig, RepairContextReceipt, RepairGitMetadata,
    RepairMetricBreach, RunReceipt, ScenarioConfigFile, ScenarioReceipt, SensorVerdictStatus,
    ToolInfo, TradeoffReceipt, VerdictStatus,
};
use regex::Regex;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BASELINE_SERVER_NOT_CONFIGURED: &str = "baseline server is not configured; set `--baseline-server`, `PERFGATE_SERVER_URL`, or `[baseline_server].url` in `perfgate.toml`";
const DEFAULT_FALLBACK_BASELINE_DIR: &str = "baselines";
const DEFAULT_ARTIFACT_DIR: &str = "artifacts/perfgate";
const RUN_RECEIPT_FILE: &str = "run.json";
const COMPARE_RECEIPT_FILE: &str = "compare.json";

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
        compare: Option<PathBuf>,

        /// Path to a tradeoff receipt.
        #[arg(long, conflicts_with = "compare")]
        tradeoff: Option<PathBuf>,

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

    /// Suggest advisory thresholds and noise policy from existing receipts.
    Calibrate(Box<CalibrateArgs>),

    /// Diagnose local setup, config, baselines, artifacts, CI, and server reachability.
    Doctor(Box<DoctorArgs>),

    /// Run paired benchmark: interleave baseline and current commands for reduced noise.
    ///
    /// Executes baseline-1, current-1, baseline-2, current-2, etc. to minimize
    /// environmental variation between measurements.
    ///
    /// Exit codes: 0 for success, 1 for errors.
    Paired(Box<PairedArgs>),

    /// Inspect local baselines and manage baselines on the baseline server.
    Baseline {
        #[command(subcommand)]
        action: BaselineAction,
    },

    /// Administer baseline service operations.
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },

    /// List and export baseline service audit events.
    Audit {
        #[command(subcommand)]
        action: AuditActionCli,
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

        /// Weighting mode for weighted policy: configured or inverse_variance
        #[arg(long, default_value = "configured", value_parser = parse_aggregate_weight_mode)]
        weight_mode: AggregateWeightMode,

        /// Minimum variance used by inverse-variance weighting when a runner is extremely stable
        #[arg(long)]
        variance_floor: Option<f64>,

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

    /// Evaluate scenario and tradeoff evidence into a review-ready decision summary.
    Decision {
        #[command(subcommand)]
        action: DecisionAction,
    },

    /// Compare named probe receipts and emit probe-level deltas.
    Probe {
        #[command(subcommand)]
        action: ProbeAction,
    },

    /// Evaluate configured workload scenarios from compare receipts.
    Scenario {
        #[command(subcommand)]
        action: ScenarioAction,
    },

    /// Evaluate configured tradeoff rules against scenario evidence.
    Tradeoff {
        #[command(subcommand)]
        action: TradeoffAction,
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

    /// Provide AI-ready prompts, artifact explanations, and automated playbooks.
    Explain {
        #[command(subcommand)]
        action: Option<ExplainAction>,

        /// Path to a compare receipt
        #[arg(long)]
        compare: Option<PathBuf>,

        /// Path to baseline Cargo.lock for binary blame analysis
        #[arg(long)]
        baseline_lock: Option<PathBuf>,

        /// Path to current Cargo.lock for binary blame analysis
        #[arg(long)]
        current_lock: Option<PathBuf>,
    },

    /// Import benchmark results from external frameworks into perfgate format.
    ///
    /// Supports Criterion, hyperfine, Go bench, pytest-benchmark, OTel spans,
    /// and probe JSONL. Produces standard perfgate.run.v1 or perfgate.probe.v1
    /// receipts.
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

    /// Inspect optional decision-ledger readiness without making it required.
    Ledger {
        #[command(subcommand)]
        action: LedgerAction,
    },

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

    /// Check the local server database path and port, then exit
    #[arg(long, default_value_t = false)]
    pub doctor: bool,

    /// Do not open the browser automatically
    #[arg(long, default_value_t = false)]
    pub no_open: bool,
}

#[derive(Debug, Subcommand)]
pub enum LedgerAction {
    /// Report optional server-ledger readiness and next actions.
    Doctor(LedgerDoctorArgs),
}

#[derive(Debug, Args)]
pub struct LedgerDoctorArgs {
    /// Path to perfgate.toml.
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Artifact directory used to verify local receipt readiness.
    #[arg(long, default_value = DEFAULT_ARTIFACT_DIR)]
    pub out_dir: PathBuf,

    /// Report configured state without contacting the server.
    #[arg(long, default_value_t = false)]
    pub offline: bool,
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
    /// Path to a compare receipt (mutually exclusive with --report and --tradeoff)
    #[arg(long, conflicts_with_all = ["report", "tradeoff"])]
    pub compare: Option<PathBuf>,

    /// Path to a report receipt (mutually exclusive with --compare and --tradeoff)
    #[arg(long, conflicts_with_all = ["compare", "tradeoff"])]
    pub report: Option<PathBuf>,

    /// Path to a tradeoff receipt (mutually exclusive with --compare and --report)
    #[arg(long, conflicts_with_all = ["compare", "report"])]
    pub tradeoff: Option<PathBuf>,

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

    /// Project name for server baseline lookup (overrides global --project).
    #[arg(long)]
    pub baseline_project: Option<String>,

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

    /// Output directory for artifacts. Defaults to [defaults].out_dir or artifacts/perfgate.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

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
pub struct CalibrateArgs {
    /// Path to the config file (TOML or JSON)
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Name of the benchmark to calibrate (must match a [[bench]] in config)
    #[arg(long)]
    pub bench: String,

    /// Output directory containing recent artifacts. Defaults to [defaults].out_dir or artifacts/perfgate.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// Explicit recent run receipt. Defaults to <out-dir>/<bench>/run.json, then <out-dir>/run.json.
    #[arg(long)]
    pub run: Option<PathBuf>,

    /// Explicit baseline receipt. Defaults to the configured baseline path.
    #[arg(long)]
    pub baseline: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Path to the config file (TOML or JSON)
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Output directory checked for artifact writability. Defaults to [defaults].out_dir or artifacts/perfgate.
    #[arg(long, value_name = "DIR")]
    pub out_dir: Option<PathBuf>,

    /// Exit non-zero if doctor finds a failed required check
    #[arg(long, default_value_t = false)]
    pub strict: bool,
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

    /// Maximum retry batches to collect when significance is required but not reached.
    /// Each retry adds an adaptive number of measured pairs.
    #[arg(long, default_value_t = 0)]
    pub max_retries: u32,

    /// Optional CV threshold for early termination of significance retries.
    /// If wall-time diff CV exceeds this value, retries stop because the benchmark is too noisy.
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
    #[command(subcommand)]
    pub command: Option<IngestCommand>,

    /// Input format: criterion, hyperfine, gobench, pytest, otel
    #[arg(long)]
    pub format: Option<String>,

    /// Path to the input file (or directory for criterion)
    #[arg(long)]
    pub input: Option<PathBuf>,

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

#[derive(Debug, Subcommand)]
pub enum IngestCommand {
    /// Ingest language-agnostic probe JSONL into a perfgate.probe.v1 receipt.
    Probes(IngestProbesArgs),
}

#[derive(Debug, Subcommand)]
pub enum ProbeAction {
    /// Generate reviewable probe JSONL and policy starter templates.
    Init(ProbeInitArgs),
    /// Compare two perfgate.probe.v1 receipts into a perfgate.probe_compare.v1 receipt.
    Compare(ProbeCompareArgs),
}

#[derive(Debug, Args)]
pub struct ProbeInitArgs {
    /// Starter template to generate.
    #[arg(long, value_enum)]
    pub template: ProbeTemplate,

    /// Directory for generated probe starter files.
    #[arg(long, default_value = "perfgate-probes")]
    pub out_dir: PathBuf,

    /// Overwrite existing generated files.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProbeTemplate {
    Parser,
    Batch,
    Cli,
    Server,
}

#[derive(Debug, Subcommand)]
pub enum ExplainAction {
    /// Explain known perfgate artifacts in a receipt directory.
    Artifacts(ExplainArtifactsArgs),
}

#[derive(Debug, Args)]
pub struct ExplainArtifactsArgs {
    /// Artifact directory to explain.
    #[arg(long, default_value = DEFAULT_ARTIFACT_DIR)]
    pub out_dir: PathBuf,
}

#[derive(Debug, Subcommand)]
pub enum ScenarioAction {
    /// Evaluate configured scenarios into a perfgate.scenario.v1 receipt.
    Evaluate(ScenarioEvaluateArgs),
}

#[derive(Debug, Subcommand)]
pub enum TradeoffAction {
    /// Evaluate configured tradeoff rules into a perfgate.tradeoff.v1 receipt.
    Evaluate(TradeoffEvaluateArgs),
}

#[derive(Debug, Subcommand)]
pub enum DecisionAction {
    /// Suggest whether structured decisions are useful for the current evidence.
    Suggest(DecisionSuggestArgs),
    /// Evaluate configured scenarios and tradeoffs, then render decision markdown.
    Evaluate(DecisionEvaluateArgs),
    /// Export indexed decision evidence as one portable JSON bundle.
    Bundle(DecisionBundleArgs),
    /// Upload a perfgate.tradeoff.v1 receipt to the baseline server decision ledger.
    Upload(DecisionUploadArgs),
    /// List stored decision receipts from the baseline server.
    History(DecisionHistoryArgs),
    /// Show the latest stored decision receipt from the baseline server.
    Latest(DecisionLatestArgs),
    /// Summarize accepted tradeoff debt from the baseline server decision ledger.
    Debt(DecisionDebtArgs),
    /// Export stored decision records as JSONL or JSON.
    Export(DecisionExportArgs),
    /// Prune old decision records from the baseline server ledger.
    Prune(DecisionPruneArgs),
}

#[derive(Debug, Args)]
pub struct ScenarioEvaluateArgs {
    /// Path to perfgate.toml.
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Evaluate only one configured scenario by name.
    #[arg(long)]
    pub scenario: Option<String>,

    /// Name to use for the combined weighted workload receipt.
    #[arg(long)]
    pub workload_name: Option<String>,

    /// Override artifact directory used for default compare receipt lookup.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Output scenario receipt path.
    #[arg(long, default_value = "scenario.json")]
    pub out: PathBuf,

    /// Pretty-print JSON.
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct TradeoffEvaluateArgs {
    /// Path to perfgate.toml.
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Path to a perfgate.scenario.v1 receipt.
    #[arg(long)]
    pub scenario: PathBuf,

    /// Output tradeoff receipt path.
    #[arg(long, default_value = "tradeoff.json")]
    pub out: PathBuf,

    /// Pretty-print JSON.
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct DecisionSuggestArgs {
    /// Path to perfgate.toml.
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Override artifact directory used for compare lookup.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DecisionEvaluateArgs {
    /// Path to perfgate.toml.
    #[arg(long, default_value = "perfgate.toml")]
    pub config: PathBuf,

    /// Evaluate only one configured scenario by name.
    #[arg(long)]
    pub scenario: Option<String>,

    /// Name to use for the combined weighted workload receipt.
    #[arg(long)]
    pub workload_name: Option<String>,

    /// Override artifact directory used for compare lookup and default outputs.
    #[arg(long)]
    pub out_dir: Option<PathBuf>,

    /// Output scenario receipt path.
    #[arg(long)]
    pub scenario_out: Option<PathBuf>,

    /// Output tradeoff receipt path.
    #[arg(long)]
    pub tradeoff_out: Option<PathBuf>,

    /// Output decision markdown path.
    #[arg(long)]
    pub decision_out: Option<PathBuf>,

    /// Output decision artifact index path.
    #[arg(long)]
    pub index_out: Option<PathBuf>,

    /// Pretty-print JSON receipts.
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct DecisionBundleArgs {
    /// Path to a perfgate.decision_index.v1 receipt.
    #[arg(long, default_value = "artifacts/perfgate/decision.index.json")]
    pub index: PathBuf,

    /// Output perfgate.decision_bundle.v1 receipt path.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Git reference associated with this bundle. Defaults to the current branch when available.
    #[arg(long)]
    pub git_ref: Option<String>,

    /// Git commit SHA associated with this bundle. Defaults to the current commit when available.
    #[arg(long)]
    pub git_sha: Option<String>,

    /// Pretty-print JSON.
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct DecisionUploadArgs {
    /// Path to a perfgate.tradeoff.v1 receipt.
    #[arg(long)]
    pub file: PathBuf,

    /// Optional perfgate.scenario.v1 receipt to store with the decision.
    #[arg(long = "scenario-receipt")]
    pub scenario_receipt: Option<PathBuf>,

    /// Optional perfgate.decision_index.v1 receipt to store with the decision.
    #[arg(long)]
    pub index: Option<PathBuf>,

    /// Project name (uses --project flag or PERFGATE_PROJECT if not specified).
    #[arg(long)]
    pub project: Option<String>,

    /// Git reference associated with this decision.
    #[arg(long)]
    pub git_ref: Option<String>,

    /// Git commit SHA associated with this decision.
    #[arg(long)]
    pub git_sha: Option<String>,
}

#[derive(Debug, Args)]
pub struct DecisionHistoryArgs {
    /// Project name (uses --project flag or PERFGATE_PROJECT if not specified).
    #[arg(long)]
    pub project: Option<String>,

    /// Optional scenario name to filter by.
    #[arg(long)]
    pub scenario: Option<String>,

    /// Optional decision metric status to filter by (pass|warn|fail|skip).
    #[arg(long, value_parser = parse_metric_status)]
    pub status: Option<MetricStatus>,

    /// Optional final policy verdict to filter by (pass|warn|fail|skip).
    #[arg(long, value_parser = parse_verdict_status)]
    pub verdict: Option<VerdictStatus>,

    /// Optional review-required filter.
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub review_required: Option<bool>,

    /// Optional accepted-tradeoff filter.
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub accepted: Option<bool>,

    /// Optional accepted tradeoff rule name to filter by.
    #[arg(long)]
    pub rule: Option<String>,

    /// Maximum number of records to list.
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

#[derive(Debug, Args)]
pub struct DecisionLatestArgs {
    /// Project name (uses --project flag or PERFGATE_PROJECT if not specified).
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub struct DecisionDebtArgs {
    /// Project name (uses --project flag or PERFGATE_PROJECT if not specified).
    #[arg(long)]
    pub project: Option<String>,

    /// Number of recent days to summarize. Use 0 to include all fetched records.
    #[arg(long, default_value_t = 30)]
    pub days: u32,

    /// Maximum number of decision records to fetch from the server.
    #[arg(long, default_value_t = 1000)]
    pub limit: u32,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DecisionExportFormat {
    Jsonl,
    Json,
}

#[derive(Debug, Args)]
pub struct DecisionExportArgs {
    /// Project name (uses --project flag or PERFGATE_PROJECT if not specified).
    #[arg(long)]
    pub project: Option<String>,

    /// Number of recent days to export. Use 0 to include all fetched records.
    #[arg(long, default_value_t = 90)]
    pub days: u32,

    /// Maximum number of decision records to fetch from the server.
    #[arg(long, default_value_t = 1000)]
    pub limit: u32,

    /// Output format.
    #[arg(long, default_value = "jsonl")]
    format: DecisionExportFormat,

    /// Output file. Defaults to stdout.
    #[arg(long)]
    pub out: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DecisionPruneArgs {
    /// Project name (uses --project flag or PERFGATE_PROJECT if not specified).
    #[arg(long)]
    pub project: Option<String>,

    /// Retention age such as 90d, 12w, or 365d.
    #[arg(long)]
    pub older_than: String,

    /// Report matching decisions without deleting them.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,

    /// Confirm deletion. Required unless --dry-run is set.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct IngestProbesArgs {
    /// Path to the probe JSONL file.
    #[arg(long)]
    pub file: PathBuf,

    /// Optional benchmark name to attach to the probe receipt.
    #[arg(long)]
    pub bench: Option<String>,

    /// Optional scenario name to attach to the probe receipt.
    #[arg(long)]
    pub scenario: Option<String>,

    /// Output file path.
    #[arg(long, default_value = "probe.json")]
    pub out: PathBuf,

    /// Pretty-print JSON.
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Args)]
pub struct ProbeCompareArgs {
    /// Baseline perfgate.probe.v1 receipt.
    #[arg(long)]
    pub baseline: PathBuf,

    /// Current perfgate.probe.v1 receipt.
    #[arg(long)]
    pub current: PathBuf,

    /// Output probe compare receipt path.
    #[arg(long, default_value = "probe-compare.json")]
    pub out: PathBuf,

    /// Pretty-print JSON.
    #[arg(long, default_value_t = false)]
    pub pretty: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InitPreset {
    /// Balanced accuracy and speed (repeat=7, warmup=1, threshold=20%)
    Standard,
    /// High accuracy, tight thresholds (repeat=10, warmup=2, threshold=10%)
    Release,
    /// Quick validation, wide thresholds (repeat=3, warmup=1, threshold=30%)
    #[value(name = "fast", alias = "tier1-fast")]
    Tier1Fast,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum InitCiPlatform {
    Github,
    Gitlab,
    Bitbucket,
    Circleci,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum BenchmarkSuggestionProfile {
    /// Infer a conservative suggestion profile from nearby repo files.
    Auto,
    /// Rust command-line app suggestions.
    RustCli,
    /// Rust workspace suggestions.
    RustWorkspace,
    /// Node.js benchmark command suggestions.
    Node,
    /// Language-neutral command benchmark suggestions.
    GenericCommand,
}

impl BenchmarkSuggestionProfile {
    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::RustCli => "rust-cli",
            Self::RustWorkspace => "rust-workspace",
            Self::Node => "node",
            Self::GenericCommand => "generic-command",
        }
    }
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Directory to scan for benchmarks (default: current directory).
    #[arg(long, default_value = ".")]
    pub dir: PathBuf,

    /// Output path for the generated config file.
    #[arg(long, default_value = "perfgate.toml")]
    pub output: PathBuf,

    /// Budget profile.
    #[arg(long = "profile", visible_alias = "preset", default_value = "standard")]
    pub preset: InitPreset,

    /// Also generate a CI workflow file.
    #[arg(long)]
    pub ci: Option<InitCiPlatform>,

    /// Append commented benchmark templates to the generated config.
    #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "auto")]
    pub suggest_benches: Option<BenchmarkSuggestionProfile>,

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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum KeyRole {
    Viewer,
    Contributor,
    Promoter,
    Admin,
}

impl From<KeyRole> for Role {
    fn from(role: KeyRole) -> Self {
        match role {
            KeyRole::Viewer => Self::Viewer,
            KeyRole::Contributor => Self::Contributor,
            KeyRole::Promoter => Self::Promoter,
            KeyRole::Admin => Self::Admin,
        }
    }
}

/// Subcommands for baseline service administration.
#[derive(Debug, Subcommand)]
enum AdminAction {
    /// Manage API keys.
    Keys {
        #[command(subcommand)]
        action: KeyAction,
    },
}

/// Subcommands for API key lifecycle management.
#[derive(Debug, Subcommand)]
enum KeyAction {
    /// Create a new API key. The plaintext key is printed once.
    Create {
        /// Project this key is scoped to.
        #[arg(long)]
        project: String,

        /// Role to grant.
        #[arg(long)]
        role: KeyRole,

        /// Human-readable key description.
        #[arg(long)]
        description: Option<String>,

        /// Optional benchmark glob/regex pattern enforced by the server.
        #[arg(long)]
        pattern: Option<String>,
    },

    /// List API keys. Key material is redacted.
    List {
        /// Filter by project.
        #[arg(long)]
        project: Option<String>,

        /// Include revoked keys.
        #[arg(long, default_value_t = false)]
        include_revoked: bool,
    },

    /// Revoke an API key by ID.
    Revoke {
        /// Key ID to revoke.
        key_id: String,
    },

    /// Create a replacement key with the same scope, then revoke the old key.
    Rotate {
        /// Key ID to rotate.
        key_id: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AuditActionFilter {
    Create,
    Update,
    Delete,
    Promote,
}

impl AuditActionFilter {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Promote => "promote",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AuditResourceFilter {
    Baseline,
    Key,
    Verdict,
    Decision,
}

impl AuditResourceFilter {
    fn as_wire(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Key => "key",
            Self::Verdict => "verdict",
            Self::Decision => "decision",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AuditExportFormat {
    Jsonl,
    Json,
}

/// Common filters for baseline service audit queries.
#[derive(Debug, Clone, Args)]
struct AuditQueryArgs {
    /// Filter by project. Defaults to --project or PERFGATE_PROJECT when set.
    #[arg(long)]
    project: Option<String>,

    /// Filter by action.
    #[arg(long)]
    action: Option<AuditActionFilter>,

    /// Filter by affected resource type.
    #[arg(long)]
    resource_type: Option<AuditResourceFilter>,

    /// Filter by actor identity, such as an API key ID or OIDC subject.
    #[arg(long)]
    actor: Option<String>,

    /// Maximum number of events to return.
    #[arg(long, default_value_t = 50)]
    limit: u32,

    /// Pagination offset.
    #[arg(long, default_value_t = 0)]
    offset: u64,
}

/// Subcommands for baseline service audit visibility.
#[derive(Debug, Subcommand)]
enum AuditActionCli {
    /// List audit events in a terminal table.
    List {
        #[command(flatten)]
        query: AuditQueryArgs,
    },

    /// Export audit events as JSONL or JSON.
    Export {
        #[command(flatten)]
        query: AuditQueryArgs,

        /// Output format.
        #[arg(long, default_value = "jsonl")]
        format: AuditExportFormat,
    },
}

/// Subcommands for baseline management.
#[derive(Debug, Subcommand)]
enum BaselineAction {
    /// Show local baseline coverage from the config file.
    Status {
        /// Path to the config file (TOML or JSON)
        #[arg(long, default_value = "perfgate.toml")]
        config: PathBuf,

        /// Limit status to one configured benchmark
        #[arg(long)]
        bench: Option<String>,
    },

    /// Create the local baseline directory declared by the config file.
    Init {
        /// Path to the config file (TOML or JSON)
        #[arg(long, default_value = "perfgate.toml")]
        config: PathBuf,
    },

    /// Promote the latest local check artifact into the configured baseline path.
    Promote {
        /// Path to the config file (TOML or JSON)
        #[arg(long, default_value = "perfgate.toml")]
        config: PathBuf,

        /// Benchmark name to promote
        #[arg(long, conflicts_with = "all", required_unless_present = "all")]
        bench: Option<String>,

        /// Promote every configured benchmark from its latest local check artifact
        #[arg(long, conflicts_with_all = ["bench", "current", "to"], default_value_t = false)]
        all: bool,

        /// Path or cloud URI to the run receipt. Defaults to [defaults].out_dir/<bench>/run.json.
        #[arg(long, conflicts_with = "all")]
        current: Option<PathBuf>,

        /// Path or cloud URI where the baseline should be written. Defaults to the configured baseline path.
        #[arg(long, conflicts_with = "all")]
        to: Option<PathBuf>,

        /// Strip run-specific fields (run_id, timestamps) for stable baselines
        #[arg(long, default_value_t = false)]
        normalize: bool,

        /// Replace an existing baseline.
        #[arg(long, visible_alias = "yes", default_value_t = false)]
        force: bool,

        /// Pretty-print JSON
        #[arg(long, default_value_t = false)]
        pretty: bool,
    },

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

    /// Show benchmarks with historically noisy verdicts.
    Flaky {
        /// Optional benchmark name to filter by
        #[arg(long)]
        benchmark: Option<String>,

        /// Project name (uses --project flag or PERFGATE_PROJECT if not specified)
        #[arg(long)]
        project: Option<String>,

        /// Maximum number of recent verdict records to inspect
        #[arg(long, default_value_t = 200)]
        limit: u32,

        /// Minimum flakiness score to show (0.0-1.0)
        #[arg(long, default_value_t = 0.5, value_parser = parse_flakiness_score)]
        min_score: f64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorStatus {
    Ok,
    Warn,
    Fail,
}

impl DoctorStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Fail => "FAIL",
        }
    }
}

#[derive(Debug)]
struct DoctorCheck {
    status: DoctorStatus,
    name: &'static str,
    detail: String,
}

impl DoctorCheck {
    fn ok(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: DoctorStatus::Ok,
            name,
            detail: detail.into(),
        }
    }

    fn warn(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: DoctorStatus::Warn,
            name,
            detail: detail.into(),
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: DoctorStatus::Fail,
            name,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdoptionState {
    NoConfig,
    ConfiguredNoBenches,
    BenchesNoBaselines,
    ReadyLocal,
    ReadyCi,
    DecisionCandidate,
    LedgerConfigured,
}

impl AdoptionState {
    fn status(self) -> &'static str {
        match self {
            Self::NoConfig => "no_config",
            Self::ConfiguredNoBenches => "configured_no_benches",
            Self::BenchesNoBaselines => "benches_no_baselines",
            Self::ReadyLocal => "ready_local",
            Self::ReadyCi => "ready_ci",
            Self::DecisionCandidate => "decision_candidate",
            Self::LedgerConfigured => "ledger_configured",
        }
    }

    fn meaning(self) -> &'static str {
        match self {
            Self::NoConfig => "No perfgate config was found for this repo.",
            Self::ConfiguredNoBenches => {
                "Config exists, but no runnable benchmarks are configured yet."
            }
            Self::BenchesNoBaselines => {
                "Benchmarks are configured, but setup is incomplete because baselines are missing."
            }
            Self::ReadyLocal => {
                "Local config and baselines are ready for a required-baseline check."
            }
            Self::ReadyCi => {
                "Local baselines and the generated GitHub Action workflow are present."
            }
            Self::DecisionCandidate => {
                "Structured decision config is present; use it when reviewers need tradeoff evidence."
            }
            Self::LedgerConfigured => {
                "Server ledger settings are configured; local receipts remain the correctness contract."
            }
        }
    }

    fn next(self, config_path: &Path) -> Vec<String> {
        let config = config_path.display();
        match self {
            Self::NoConfig => vec!["perfgate init --ci github --profile standard".to_string()],
            Self::ConfiguredNoBenches => vec![
                format!("edit {config} and add a reviewed [[bench]] command"),
                format!("perfgate doctor --config {config}"),
            ],
            Self::BenchesNoBaselines => vec![
                format!("perfgate check --config {config} --all"),
                format!("perfgate baseline promote --config {config} --all"),
            ],
            Self::ReadyLocal => vec![format!(
                "perfgate check --config {config} --all --require-baseline"
            )],
            Self::ReadyCi => vec![format!(
                "perfgate check --config {config} --all --require-baseline"
            )],
            Self::DecisionCandidate => vec![
                format!("perfgate decision evaluate --config {config}"),
                "perfgate decision bundle --index artifacts/perfgate/decision.index.json"
                    .to_string(),
            ],
            Self::LedgerConfigured => vec![
                "perfgate decision history".to_string(),
                format!("perfgate check --config {config} --all --require-baseline"),
            ],
        }
    }

    fn do_not(self) -> &'static str {
        match self {
            Self::NoConfig => "do not copy another repo's baselines before initializing this repo",
            Self::ConfiguredNoBenches => {
                "do not promote a baseline until the benchmark command measures the workload you care about"
            }
            Self::BenchesNoBaselines => "do not loosen thresholds to fix missing baseline setup",
            Self::ReadyLocal => "do not enable required CI before committing reviewed baselines",
            Self::ReadyCi => "do not debug CI before trying the local reproduction command",
            Self::DecisionCandidate => {
                "do not make structured decisions mandatory for simple local gates"
            }
            Self::LedgerConfigured => {
                "do not treat server ledger upload as local correctness unless policy makes it blocking"
            }
        }
    }
}

fn print_adoption_state(state: AdoptionState, config_path: &Path) {
    println!();
    println!("State: {}", state.status());
    println!("Meaning: {}", state.meaning());
    println!("Next:");
    for command in state.next(config_path) {
        println!("  {command}");
    }
    println!("Do not:");
    println!("  {}", state.do_not());
}

fn classify_adoption_state(
    config: Option<&ConfigFile>,
    config_path: &Path,
    server_flags: &ServerFlags,
) -> AdoptionState {
    let Some(config) = config else {
        return AdoptionState::NoConfig;
    };

    if config.benches.is_empty() {
        return AdoptionState::ConfiguredNoBenches;
    }

    if !local_baselines_ready(config, server_flags) {
        return AdoptionState::BenchesNoBaselines;
    }

    if server_flags
        .resolve(&config.baseline_server)
        .is_configured()
    {
        return AdoptionState::LedgerConfigured;
    }

    if !config.scenarios.is_empty() || !config.tradeoffs.is_empty() {
        return AdoptionState::DecisionCandidate;
    }

    let project_root = doctor_project_root(config_path);
    if project_root
        .join(ci_workflow_path(CiPlatform::GitHub))
        .exists()
    {
        return AdoptionState::ReadyCi;
    }

    AdoptionState::ReadyLocal
}

fn doctor_project_root(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn local_baselines_ready(config: &ConfigFile, server_flags: &ServerFlags) -> bool {
    if config.benches.is_empty() {
        return false;
    }

    if server_flags
        .resolve(&config.baseline_server)
        .is_configured()
    {
        return true;
    }

    let mut local = 0usize;
    let mut found = 0usize;
    let mut remote = 0usize;
    for bench in &config.benches {
        let path = resolve_baseline_path(&None, &bench.name, config);
        let path_text = path.to_string_lossy();
        if is_remote_storage_uri(&path_text) {
            remote += 1;
            continue;
        }
        local += 1;
        if path.exists() {
            found += 1;
        }
    }

    (local == 0 && remote > 0) || (local > 0 && found == local)
}

#[derive(Debug, Clone, Copy)]
struct CalibrationSuggestion {
    fail_threshold: f64,
    warn_factor: f64,
    noise_threshold: f64,
    noise_policy: perfgate_types::NoisePolicy,
    recommended_repeat: usize,
    suggest_paired: bool,
}

fn execute_calibrate(args: CalibrateArgs) -> anyhow::Result<()> {
    let config = load_config_file(&args.config)?;
    config
        .validate()
        .map_err(ConfigValidationError::ConfigFile)?;
    let bench = config
        .benches
        .iter()
        .find(|bench| bench.name == args.bench)
        .ok_or_else(|| {
            ConfigValidationError::BenchName(format!("bench '{}' not found in config", args.bench))
        })?;

    let out_dir = resolve_configured_out_dir(args.out_dir.as_ref(), Some(&config));
    let run_path = args
        .run
        .clone()
        .or_else(|| find_calibration_run_path(&out_dir, &args.bench));
    let run_receipt = run_path
        .as_ref()
        .filter(|path| path.exists())
        .map(|path| read_json::<RunReceipt>(path))
        .transpose()?;

    let baseline_path = resolve_baseline_path(&args.baseline, &args.bench, &config);
    let baseline_receipt = load_optional_baseline_receipt(&baseline_path)?;
    let evidence_receipt = run_receipt.as_ref().or(baseline_receipt.as_ref());
    let cv = run_receipt
        .as_ref()
        .and_then(|receipt| receipt.stats.wall_ms.cv())
        .or_else(|| {
            baseline_receipt
                .as_ref()
                .and_then(|receipt| receipt.stats.wall_ms.cv())
        });
    let sample_count = evidence_receipt
        .map(measured_sample_count)
        .unwrap_or_default();
    let configured_threshold = configured_wall_threshold(&config, bench);
    let suggestion = suggest_calibration(cv, sample_count, configured_threshold);

    println!("perfgate calibrate");
    println!();
    println!("Bench: {}", args.bench);
    if sample_count == 0 {
        println!("Samples: unavailable");
    } else {
        println!(
            "Samples: {sample_count} measured sample{}",
            plural(sample_count)
        );
    }
    println!(
        "CV: {}",
        cv.map(format_percent)
            .unwrap_or_else(|| "unavailable".to_string())
    );
    println!(
        "Host class: {}",
        evidence_receipt
            .map(host_class)
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!();
    println!("Evidence:");
    if let Some(path) = run_path.as_ref().filter(|path| path.exists()) {
        println!("  run: {}", path.display());
    } else if let Some(path) = run_path.as_ref() {
        println!("  run: missing ({})", path.display());
    } else {
        println!(
            "  run: missing (expected {})",
            out_dir.join(RUN_RECEIPT_FILE).display()
        );
    }
    if baseline_receipt.is_some() {
        println!("  baseline: {}", baseline_path.display());
    } else {
        println!("  baseline: missing ({})", baseline_path.display());
    }
    println!();
    println!(
        "Suggested fail threshold: {}",
        format_percent(suggestion.fail_threshold)
    );
    println!(
        "Suggested warn threshold: {}",
        format_percent(suggestion.fail_threshold * suggestion.warn_factor)
    );
    println!(
        "Suggested noise threshold: {}",
        format_percent(suggestion.noise_threshold)
    );
    println!(
        "Suggested noise policy: {}",
        suggestion.noise_policy.as_str()
    );
    println!(
        "Repeat guidance: collect at least {} measured samples before tightening.",
        suggestion.recommended_repeat
    );
    if suggestion.suggest_paired {
        println!("Paired mode: recommended before making this gate blocking.");
    } else {
        println!("Paired mode: not required yet; use it if reviewers see inconsistent results.");
    }
    println!();
    println!("Suggested config patch:");
    println!("  threshold = {:.2}", suggestion.fail_threshold);
    println!("  warn_factor = {:.2}", suggestion.warn_factor);
    println!("  noise_threshold = {:.2}", suggestion.noise_threshold);
    println!("  noise_policy = \"{}\"", suggestion.noise_policy.as_str());
    println!();
    println!("Next:");
    if run_receipt.is_none() {
        println!(
            "  {}",
            check_command(&args.config, Some(&args.bench), false)
        );
    }
    println!("  {}", check_command(&args.config, Some(&args.bench), true));
    if suggestion.suggest_paired {
        println!("  {}", paired_command(Some(&args.bench)));
    }
    println!("Do not:");
    println!("  do not auto-edit thresholds from this advisory output; review the benchmark first");
    println!();
    println!("Advisory only: no config was written.");

    Ok(())
}

fn find_calibration_run_path(out_dir: &Path, bench_name: &str) -> Option<PathBuf> {
    [
        out_dir.join(bench_name).join(RUN_RECEIPT_FILE),
        out_dir.join(RUN_RECEIPT_FILE),
    ]
    .into_iter()
    .find(|path| path.exists())
}

fn configured_wall_threshold(config: &ConfigFile, bench: &perfgate_types::BenchConfigFile) -> f64 {
    bench
        .budgets
        .as_ref()
        .and_then(|budgets| budgets.get(&Metric::WallMs))
        .and_then(|budget| budget.threshold)
        .or(config.defaults.threshold)
        .unwrap_or(0.20)
}

fn suggest_calibration(
    cv: Option<f64>,
    sample_count: usize,
    configured_threshold: f64,
) -> CalibrationSuggestion {
    let fail_threshold = cv
        .map(|cv| {
            if cv <= 0.02 {
                0.05
            } else if cv <= 0.05 {
                0.10
            } else if cv <= 0.10 {
                0.15
            } else if cv <= 0.20 {
                0.20
            } else {
                configured_threshold.max(0.30)
            }
        })
        .unwrap_or(configured_threshold.max(0.20));
    let noise_threshold = cv.map(|cv| (cv * 2.0).clamp(0.05, 0.30)).unwrap_or(0.08);
    CalibrationSuggestion {
        fail_threshold,
        warn_factor: 0.50,
        noise_threshold,
        noise_policy: perfgate_types::NoisePolicy::Warn,
        recommended_repeat: sample_count.max(10),
        suggest_paired: cv.is_some_and(|cv| cv > 0.10),
    }
}

fn measured_sample_count(receipt: &RunReceipt) -> usize {
    receipt
        .samples
        .iter()
        .filter(|sample| !sample.warmup)
        .count()
}

fn host_class(receipt: &RunReceipt) -> String {
    format!("{}-{}", receipt.run.host.os, receipt.run.host.arch)
}

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn execute_doctor(args: DoctorArgs, server_flags: ServerFlags) -> anyhow::Result<()> {
    let mut checks = Vec::new();
    checks.push(DoctorCheck::ok(
        "version",
        env!("CARGO_PKG_VERSION").to_string(),
    ));
    checks.push(DoctorCheck::ok(
        "platform",
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
    ));

    let config = if args.config.exists() {
        match load_config_file(&args.config) {
            Ok(config) => {
                match config.validate() {
                    Ok(()) => checks.push(DoctorCheck::ok(
                        "config",
                        format!(
                            "{} found ({} benchmark{})",
                            args.config.display(),
                            config.benches.len(),
                            plural(config.benches.len())
                        ),
                    )),
                    Err(error) => checks.push(DoctorCheck::fail(
                        "config",
                        format!("{} is invalid: {error}", args.config.display()),
                    )),
                }
                Some(config)
            }
            Err(error) => {
                checks.push(DoctorCheck::fail(
                    "config",
                    format!("failed to load {}: {error}", args.config.display()),
                ));
                None
            }
        }
    } else {
        checks.push(DoctorCheck::fail(
            "config",
            format!(
                "{} not found; run `perfgate init` or pass --config",
                args.config.display()
            ),
        ));
        None
    };

    checks.push(doctor_git_check());
    checks.push(doctor_ci_check());

    if let Some(config) = &config {
        checks.push(doctor_benchmark_commands(config));
        checks.push(doctor_baselines(config, &server_flags));
        checks.push(doctor_server(config, &server_flags));
    } else {
        checks.push(DoctorCheck::warn(
            "benchmarks",
            "skipped because config could not be loaded",
        ));
        checks.push(DoctorCheck::warn(
            "baselines",
            "skipped because config could not be loaded",
        ));
        checks.push(doctor_server(&ConfigFile::default(), &server_flags));
    }

    let artifact_dir = resolve_configured_out_dir(args.out_dir.as_ref(), config.as_ref());
    checks.push(doctor_artifact_dir(&artifact_dir));
    let adoption_state = classify_adoption_state(config.as_ref(), &args.config, &server_flags);

    println!("perfgate doctor");
    println!();
    for check in &checks {
        println!(
            "{:<4} {:<18} {}",
            check.status.as_str(),
            check.name,
            check.detail
        );
    }
    print_adoption_state(adoption_state, &args.config);

    let failed = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Fail)
        .count();
    let warned = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Warn)
        .count();
    println!();
    println!(
        "Summary: {failed} failed, {warned} warning{}",
        plural(warned)
    );

    if args.strict && failed > 0 {
        anyhow::bail!("doctor found {failed} failed check{}", plural(failed));
    }

    Ok(())
}

fn plural(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn doctor_git_check() -> DoctorCheck {
    match run_git_capture(&["rev-parse", "--is-inside-work-tree"]) {
        Some(value) if value == "true" => {
            let branch = run_git_capture(&["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_else(|| "unknown".to_string());
            DoctorCheck::ok("git", format!("repository detected ({branch})"))
        }
        _ => DoctorCheck::ok("git", "not a git repository"),
    }
}

fn doctor_ci_check() -> DoctorCheck {
    match detect_ci_provider() {
        Some(provider) => DoctorCheck::ok("ci", format!("detected {provider}")),
        None => DoctorCheck::ok("ci", "not detected"),
    }
}

fn detect_ci_provider() -> Option<&'static str> {
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        Some("GitHub Actions")
    } else if std::env::var_os("GITLAB_CI").is_some() {
        Some("GitLab CI")
    } else if std::env::var_os("BITBUCKET_BUILD_NUMBER").is_some() {
        Some("Bitbucket Pipelines")
    } else if std::env::var_os("CIRCLECI").is_some() {
        Some("CircleCI")
    } else if std::env::var_os("CI").is_some() {
        Some("generic CI")
    } else {
        None
    }
}

fn doctor_benchmark_commands(config: &ConfigFile) -> DoctorCheck {
    if config.benches.is_empty() {
        return DoctorCheck::fail("benchmarks", "no [[bench]] entries configured");
    }

    let mut runnable = 0usize;
    let mut missing = Vec::new();
    for bench in &config.benches {
        match bench_command_runnable(bench) {
            Ok(true) => runnable += 1,
            Ok(false) => missing.push(bench.name.clone()),
            Err(error) => missing.push(format!("{} ({error})", bench.name)),
        }
    }

    if missing.is_empty() {
        DoctorCheck::ok(
            "benchmarks",
            format!(
                "{}/{} command{} runnable",
                runnable,
                config.benches.len(),
                plural(config.benches.len())
            ),
        )
    } else {
        DoctorCheck::fail(
            "benchmarks",
            format!(
                "{}/{} command{} runnable; not runnable: {}",
                runnable,
                config.benches.len(),
                plural(config.benches.len()),
                missing.join(", ")
            ),
        )
    }
}

fn bench_command_runnable(bench: &perfgate_types::BenchConfigFile) -> anyhow::Result<bool> {
    let Some(program) = bench.command.first() else {
        return Ok(false);
    };

    let cwd = resolve_bench_cwd(bench.cwd.as_deref());
    if !cwd.exists() {
        anyhow::bail!("cwd does not exist: {}", cwd.display());
    }

    Ok(program_is_runnable(program, &cwd))
}

fn resolve_bench_cwd(cwd: Option<&str>) -> PathBuf {
    match cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => PathBuf::from("."),
    }
}

fn program_is_runnable(program: &str, cwd: &Path) -> bool {
    let path = Path::new(program);
    if path.is_absolute() || program.contains('/') || program.contains('\\') {
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        return executable_candidate_exists(&candidate);
    }

    find_program_on_path(program).is_some()
}

fn find_program_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if executable_candidate_exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

fn executable_candidate_exists(path: &Path) -> bool {
    if path.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let Ok(metadata) = path.metadata() else {
                return false;
            };
            return metadata.permissions().mode() & 0o111 != 0;
        }

        #[cfg(not(unix))]
        {
            return true;
        }
    }

    #[cfg(windows)]
    {
        if path.extension().is_none() {
            let pathext =
                std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
            for ext in pathext.split(';').filter(|ext| !ext.is_empty()) {
                let ext = ext.trim_start_matches('.');
                if path.with_extension(ext).is_file() {
                    return true;
                }
            }
        }
    }

    false
}

fn doctor_baselines(config: &ConfigFile, server_flags: &ServerFlags) -> DoctorCheck {
    if config.benches.is_empty() {
        return DoctorCheck::warn("baselines", "skipped because no benchmarks are configured");
    }

    let server_config = server_flags.resolve(&config.baseline_server);
    if server_config.is_configured() {
        return DoctorCheck::ok("baselines", "baseline server configured");
    }

    let mut local = 0usize;
    let mut found = 0usize;
    let mut remote = 0usize;
    for bench in &config.benches {
        let path = resolve_baseline_path(&None, &bench.name, config);
        let path_text = path.to_string_lossy();
        if is_remote_storage_uri(&path_text) {
            remote += 1;
            continue;
        }
        local += 1;
        if path.exists() {
            found += 1;
        }
    }

    if local == 0 && remote > 0 {
        return DoctorCheck::ok(
            "baselines",
            format!(
                "{remote} remote baseline URI{} configured; not probed",
                plural(remote)
            ),
        );
    }

    if found == local {
        DoctorCheck::ok(
            "baselines",
            format!("{found}/{local} local baseline{} found", plural(local)),
        )
    } else {
        DoctorCheck::warn(
            "baselines",
            format!(
                "{found}/{local} local baseline{} found; inspect with `perfgate baseline status` and create missing baselines with `perfgate baseline promote --bench <bench>`",
                plural(local)
            ),
        )
    }
}

fn doctor_server(config: &ConfigFile, server_flags: &ServerFlags) -> DoctorCheck {
    let server_config = server_flags.resolve(&config.baseline_server);
    if !server_config.is_configured() {
        return DoctorCheck::ok("baseline server", "not configured");
    }

    let Some(url) = server_config.url.as_ref() else {
        return DoctorCheck::ok("baseline server", "not configured");
    };

    let mut client_config = ClientConfig::new(url)
        .with_timeout(Duration::from_secs(2))
        .with_retry(RetryConfig::new().with_max_retries(0));
    if let Some(api_key) = &server_config.api_key {
        client_config = client_config.with_api_key(api_key);
    }

    let client = match BaselineClient::new(client_config) {
        Ok(client) => client,
        Err(error) => {
            return DoctorCheck::fail("baseline server", format!("{url} invalid: {error}"));
        }
    };

    match with_tokio_runtime(async { client.health_check().await.map_err(anyhow::Error::from) }) {
        Ok(health) if health.status == "healthy" => {
            let project = server_config.project.as_deref().unwrap_or("not configured");
            DoctorCheck::ok(
                "baseline server",
                format!("{url} reachable (project: {project})"),
            )
        }
        Ok(health) => DoctorCheck::fail(
            "baseline server",
            format!("{url} returned unhealthy status: {}", health.status),
        ),
        Err(error) => {
            DoctorCheck::fail("baseline server", format!("{url} not reachable: {error:#}"))
        }
    }
}

fn doctor_artifact_dir(out_dir: &Path) -> DoctorCheck {
    match ensure_artifact_dir_writable(out_dir) {
        Ok(()) => DoctorCheck::ok(
            "artifact directory",
            format!("{} writable", out_dir.display()),
        ),
        Err(error) => DoctorCheck::fail(
            "artifact directory",
            format!("{} not writable: {error}", out_dir.display()),
        ),
    }
}

fn ensure_artifact_dir_writable(out_dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(out_dir)?;
    let probe = out_dir.join(".perfgate-doctor-write-test");
    fs::write(&probe, b"perfgate doctor\n")?;
    fs::remove_file(probe)?;
    Ok(())
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
                let client = server_config.require_client(BASELINE_SERVER_NOT_CONFIGURED)?;
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
                baseline_project,
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
            let baseline_selector =
                parse_baseline_selector(&baseline, &server_config, BASELINE_SERVER_NOT_CONFIGURED)?;
            let (baseline_receipt, baseline_ref) = match baseline_selector {
                BaselineSelector::Server {
                    benchmark,
                    explicit,
                } => {
                    let explicit_baseline_project = baseline_project.is_some();
                    let project = server_config.resolve_project(baseline_project)?;
                    let record = if explicit {
                        let client =
                            server_config.require_client(BASELINE_SERVER_NOT_CONFIGURED)?;
                        with_tokio_runtime(async {
                            let record: BaselineRecord = client
                                .get_latest_baseline(&project, &benchmark)
                                .await
                                .with_context(|| {
                                    format!(
                                        "Failed to fetch baseline '{benchmark}' from server (project: {project})"
                                    )
                                })?;
                            Ok::<BaselineRecord, anyhow::Error>(record)
                        })?
                    } else {
                        let client = server_config.require_fallback_client(
                            Some(Path::new(DEFAULT_FALLBACK_BASELINE_DIR)),
                            BASELINE_SERVER_NOT_CONFIGURED,
                        )?;
                        with_tokio_runtime(async {
                            let record: BaselineRecord = client
                                .get_latest_baseline(&project, &benchmark)
                                .await
                                .with_context(|| {
                                    format!(
                                        "Failed to fetch baseline '{benchmark}' from server (project: {project})"
                                    )
                                })?;
                            Ok::<BaselineRecord, anyhow::Error>(record)
                        })?
                    };

                    let receipt = record.receipt;
                    let ref_info = CompareRef {
                        path: Some(if explicit_baseline_project {
                            format!("@server:{project}/{benchmark}")
                        } else {
                            format!("@server:{benchmark}")
                        }),
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
            tradeoff,
            out,
            template,
        } => {
            let md = if let Some(compare) = compare {
                let compare_receipt: CompareReceipt = read_json(&compare)?;
                render_markdown_with_optional_template(&compare_receipt, template.as_deref())?
            } else if let Some(tradeoff) = tradeoff {
                if template.is_some() {
                    anyhow::bail!("--template is only supported with --compare");
                }
                let tradeoff_receipt: TradeoffReceipt = read_json(&tradeoff)?;
                render_tradeoff_markdown(&tradeoff_receipt)
            } else {
                anyhow::bail!("Either --compare or --tradeoff is required");
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
                let client = server_config.require_client(BASELINE_SERVER_NOT_CONFIGURED)?;
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

        Command::Calibrate(args) => execute_calibrate(*args),

        Command::Doctor(args) => execute_doctor(*args, server_flags),

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

        Command::Admin { action } => execute_admin_action(action, &server_flags),

        Command::Audit { action } => execute_audit_action(action, &server_flags),

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
            weight_mode,
            variance_floor,
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
            let (quorum, fail_if, variance_floor) = validate_aggregate_options(
                policy,
                weight_mode,
                quorum,
                fail_n,
                fail_m,
                variance_floor,
            )?;
            let weight_map = parse_weight_map(&weights)?;
            let outcome = usecase.execute(perfgate_app::AggregateRequest {
                files: resolved_files,
                policy,
                quorum,
                fail_if,
                weights: weight_map,
                weight_mode,
                variance_floor,
                runner_class,
                lane,
            })?;
            write_json(&out, &outcome.aggregate, pretty)?;
            match outcome.aggregate.verdict.status {
                MetricStatus::Fail => exit_with_code(2),
                MetricStatus::Pass | MetricStatus::Warn | MetricStatus::Skip => Ok(()),
            }
        }

        Command::Decision { action } => execute_decision_action(action, &server_flags),
        Command::Ledger { action } => execute_ledger_action(action, &server_flags),
        Command::Probe { action } => execute_probe_action(action),
        Command::Scenario { action } => execute_scenario_action(action),
        Command::Tradeoff { action } => execute_tradeoff_action(action),

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
            action,
            compare,
            baseline_lock,
            current_lock,
        } => {
            if let Some(action) = action {
                return execute_explain_action(action);
            }
            let Some(compare) = compare else {
                anyhow::bail!(
                    "use `perfgate explain --compare <compare.json>` or `perfgate explain artifacts --out-dir <dir>`"
                );
            };
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
                command,
                format,
                input,
                name,
                include_span,
                exclude_span,
                out,
                pretty,
            } = *args;

            if let Some(command) = command {
                return match command {
                    IngestCommand::Probes(args) => execute_ingest_probes(args),
                };
            }

            let format = format.ok_or_else(|| {
                anyhow::anyhow!("ingest requires --format unless using a subcommand")
            })?;
            let input = input.ok_or_else(|| {
                anyhow::anyhow!("ingest requires --input unless using a subcommand")
            })?;

            let format = IngestFormat::parse(&format).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown ingest format '{}'; supported: criterion, hyperfine, gobench, pytest, otel",
                    format
                )
            })?;

            let content = fs::read_to_string(&input)
                .with_context(|| format!("read input file {}", input.display()))?;

            let request = ingest::IngestRequest {
                format,
                input: content,
                name,
                include_spans: include_span,
                exclude_spans: exclude_span,
            };

            let receipt = ingest::ingest(&request)?;
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

fn execute_ingest_probes(args: IngestProbesArgs) -> anyhow::Result<()> {
    let content = fs::read_to_string(&args.file)
        .with_context(|| format!("read probe JSONL file {}", args.file.display()))?;
    let request = ingest::ProbeIngestRequest {
        input: content,
        bench: args.bench,
        scenario: args.scenario,
    };
    let receipt = ingest::ingest_probes_jsonl(&request)?;
    write_json(&args.out, &receipt, args.pretty)?;
    eprintln!(
        "Ingested probes {} -> {}",
        args.file.display(),
        args.out.display()
    );
    Ok(())
}

fn execute_probe_action(action: ProbeAction) -> anyhow::Result<()> {
    match action {
        ProbeAction::Init(args) => execute_probe_init(args),
        ProbeAction::Compare(args) => execute_probe_compare(args),
    }
}

fn execute_probe_init(args: ProbeInitArgs) -> anyhow::Result<()> {
    let template = probe_template(args.template);
    fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("create probe template directory {}", args.out_dir.display()))?;

    write_probe_template_file(
        &args.out_dir.join("README.md"),
        &render_probe_template_readme(&template, &args.out_dir),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("probes-baseline.jsonl"),
        &format!("{}\n", template.baseline_jsonl.join("\n")),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("probes-current.jsonl"),
        &format!("{}\n", template.current_jsonl.join("\n")),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("scenario.toml"),
        &render_probe_template_scenario(&template),
        args.force,
    )?;
    write_probe_template_file(
        &args.out_dir.join("tradeoff.toml"),
        &render_probe_template_tradeoff(&template),
        args.force,
    )?;

    eprintln!(
        "Probe starter template '{}' written to {}",
        template.slug,
        args.out_dir.display()
    );
    eprintln!("Review the generated snippets before adding them to perfgate.toml.");
    eprintln!(
        "Next: perfgate ingest probes --file {}/probes-current.jsonl --out artifacts/perfgate/probes-current.json",
        args.out_dir.display()
    );
    Ok(())
}

fn write_probe_template_file(path: &Path, content: &str, force: bool) -> anyhow::Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "{} already exists; pass --force to overwrite generated probe starter files",
            path.display()
        );
    }

    fs::write(path, content).with_context(|| format!("write probe template {}", path.display()))
}

#[derive(Debug, Clone, Copy)]
struct ProbeStarterTemplate {
    slug: &'static str,
    title: &'static str,
    bench: &'static str,
    scenario: &'static str,
    workload: &'static str,
    dominant_probe: &'static str,
    local_probe: &'static str,
    support_probe: &'static str,
    baseline_jsonl: &'static [&'static str],
    current_jsonl: &'static [&'static str],
}

fn probe_template(template: ProbeTemplate) -> ProbeStarterTemplate {
    match template {
        ProbeTemplate::Parser => ProbeStarterTemplate {
            slug: "parser",
            title: "Parser pipeline probes",
            bench: "parser",
            scenario: "large_file_parse",
            workload: "large-file parser throughput",
            dominant_probe: "parser.batch_loop",
            local_probe: "parser.tokenize",
            support_probe: "parser.ast_build",
            baseline_jsonl: &[
                r#"{"probe":"parser.tokenize","scope":"local","wall_ms":12.4,"items":10000}"#,
                r#"{"probe":"parser.ast_build","scope":"local","wall_ms":28.2,"items":10000}"#,
                r#"{"probe":"parser.batch_loop","scope":"dominant","wall_ms":44.8,"items":10000}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"parser.tokenize","scope":"local","wall_ms":12.8,"items":10000}"#,
                r#"{"probe":"parser.ast_build","scope":"local","wall_ms":27.5,"items":10000}"#,
                r#"{"probe":"parser.batch_loop","scope":"dominant","wall_ms":39.6,"items":10000}"#,
            ],
        },
        ProbeTemplate::Batch => ProbeStarterTemplate {
            slug: "batch",
            title: "Batch processing probes",
            bench: "batch",
            scenario: "batch_transform",
            workload: "batch transform throughput",
            dominant_probe: "batch.transform",
            local_probe: "batch.read_inputs",
            support_probe: "batch.write_outputs",
            baseline_jsonl: &[
                r#"{"probe":"batch.read_inputs","scope":"local","wall_ms":18.0,"items":5000}"#,
                r#"{"probe":"batch.transform","scope":"dominant","wall_ms":92.0,"items":5000}"#,
                r#"{"probe":"batch.write_outputs","scope":"local","wall_ms":26.0,"items":5000}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"batch.read_inputs","scope":"local","wall_ms":18.4,"items":5000}"#,
                r#"{"probe":"batch.transform","scope":"dominant","wall_ms":82.0,"items":5000}"#,
                r#"{"probe":"batch.write_outputs","scope":"local","wall_ms":25.2,"items":5000}"#,
            ],
        },
        ProbeTemplate::Cli => ProbeStarterTemplate {
            slug: "cli",
            title: "CLI workflow probes",
            bench: "cli-command",
            scenario: "cli_user_path",
            workload: "CLI command response path",
            dominant_probe: "cli.execute",
            local_probe: "cli.parse_args",
            support_probe: "cli.render_output",
            baseline_jsonl: &[
                r#"{"probe":"cli.parse_args","scope":"local","wall_ms":3.2,"items":1}"#,
                r#"{"probe":"cli.execute","scope":"dominant","wall_ms":42.0,"items":1}"#,
                r#"{"probe":"cli.render_output","scope":"local","wall_ms":5.8,"items":1}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"cli.parse_args","scope":"local","wall_ms":3.3,"items":1}"#,
                r#"{"probe":"cli.execute","scope":"dominant","wall_ms":37.8,"items":1}"#,
                r#"{"probe":"cli.render_output","scope":"local","wall_ms":5.9,"items":1}"#,
            ],
        },
        ProbeTemplate::Server => ProbeStarterTemplate {
            slug: "server",
            title: "Server request probes",
            bench: "server-request",
            scenario: "server_local_request",
            workload: "controlled local server request path",
            dominant_probe: "server.handle_request",
            local_probe: "server.decode_request",
            support_probe: "server.encode_response",
            baseline_jsonl: &[
                r#"{"probe":"server.decode_request","scope":"local","wall_ms":4.4,"items":100}"#,
                r#"{"probe":"server.handle_request","scope":"dominant","wall_ms":31.0,"items":100}"#,
                r#"{"probe":"server.encode_response","scope":"local","wall_ms":6.5,"items":100}"#,
            ],
            current_jsonl: &[
                r#"{"probe":"server.decode_request","scope":"local","wall_ms":4.5,"items":100}"#,
                r#"{"probe":"server.handle_request","scope":"dominant","wall_ms":27.0,"items":100}"#,
                r#"{"probe":"server.encode_response","scope":"local","wall_ms":6.7,"items":100}"#,
            ],
        },
    }
}

fn render_probe_template_readme(template: &ProbeStarterTemplate, out_dir: &Path) -> String {
    let out_dir = out_dir.display();
    format!(
        r#"# {title}

This starter shows a small probe set for {workload}. Review and edit the probe
names before using them in durable decision policy.

Generated files:

- `probes-baseline.jsonl`: reviewed baseline probe events
- `probes-current.jsonl`: current-run probe events for local experimentation
- `scenario.toml`: scenario snippet to copy into `perfgate.toml`
- `tradeoff.toml`: tradeoff snippet to copy into `perfgate.toml`

Next:

```bash
perfgate ingest probes --file {out_dir}/probes-baseline.jsonl --out baselines/probes.json
perfgate ingest probes --file {out_dir}/probes-current.jsonl --out artifacts/perfgate/probes-current.json
perfgate probe compare --baseline baselines/probes.json --current artifacts/perfgate/probes-current.json --out artifacts/perfgate/probe-compare.json
perfgate decision suggest --config perfgate.toml
```

Do not:

- keep probes that no reviewer can act on;
- use generated sample values as release proof;
- turn a temporary debugging span into a durable probe id.
"#,
        title = template.title,
        workload = template.workload,
        out_dir = out_dir
    )
}

fn render_probe_template_scenario(template: &ProbeStarterTemplate) -> String {
    format!(
        r#"# Copy into perfgate.toml after reviewing paths and names.
[[scenario]]
name = "{scenario}"
bench = "{bench}"
weight = 1.0
probe_baseline = "baselines/probes.json"
probe_current = "artifacts/perfgate/probes-current.json"
probe_compare = "artifacts/perfgate/probe-compare.json"
"#,
        scenario = template.scenario,
        bench = template.bench
    )
}

fn render_probe_template_tradeoff(template: &ProbeStarterTemplate) -> String {
    format!(
        r#"# Copy into perfgate.toml only when reviewers need a bounded tradeoff.
[[tradeoff]]
name = "{slug}-dominant-probe-improvement"
if_failed = "wall_ms"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
probe = "{dominant_probe}"
min_improvement_ratio = 1.10

[[tradeoff.allow]]
metric = "wall_ms"
probe = "{local_probe}"
max_regression = 0.03

# Supporting probe to keep visible in reviews: {support_probe}
"#,
        slug = template.slug,
        dominant_probe = template.dominant_probe,
        local_probe = template.local_probe,
        support_probe = template.support_probe
    )
}

fn execute_probe_compare(args: ProbeCompareArgs) -> anyhow::Result<()> {
    let baseline: ProbeReceipt = read_json(&args.baseline)
        .with_context(|| format!("read baseline probe receipt {}", args.baseline.display()))?;
    let current: ProbeReceipt = read_json(&args.current)
        .with_context(|| format!("read current probe receipt {}", args.current.display()))?;

    let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
        baseline_ref: CompareRef {
            path: Some(args.baseline.display().to_string()),
            run_id: Some(baseline.run.id.clone()),
        },
        current_ref: CompareRef {
            path: Some(args.current.display().to_string()),
            run_id: Some(current.run.id.clone()),
        },
        baseline,
        current,
        tool: tool_info(),
    })?;

    write_json(&args.out, &outcome.receipt, args.pretty)?;
    eprintln!("Probe compare receipt written to {}", args.out.display());
    Ok(())
}

fn execute_scenario_action(action: ScenarioAction) -> anyhow::Result<()> {
    match action {
        ScenarioAction::Evaluate(args) => execute_scenario_evaluate(args),
    }
}

fn execute_scenario_evaluate(args: ScenarioEvaluateArgs) -> anyhow::Result<()> {
    let config = load_validated_config(&args.config)?;
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| resolve_configured_out_dir(None, Some(&config)));
    let outcome = evaluate_configured_scenarios(
        config,
        &args.config,
        args.scenario.as_deref(),
        args.workload_name,
        &out_dir,
    )?;

    write_json(&args.out, &outcome.receipt, args.pretty)?;
    eprintln!("Scenario receipt written to {}", args.out.display());

    match outcome.receipt.verdict.status {
        VerdictStatus::Fail => exit_with_code(2),
        VerdictStatus::Pass | VerdictStatus::Warn | VerdictStatus::Skip => Ok(()),
    }
}

fn load_validated_config(config_path: &Path) -> anyhow::Result<ConfigFile> {
    let config = load_config_file(config_path)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    config
        .validate()
        .map_err(|error| anyhow::anyhow!("{} is invalid: {error}", config_path.display()))?;
    Ok(config)
}

fn evaluate_configured_scenarios(
    config: ConfigFile,
    config_path: &Path,
    scenario: Option<&str>,
    workload_name: Option<String>,
    out_dir: &Path,
) -> anyhow::Result<perfgate_app::ScenarioEvaluateOutcome> {
    if config.scenarios.is_empty() {
        anyhow::bail!(
            "no [[scenario]] entries configured in {}",
            config_path.display()
        );
    }

    let selected: Vec<_> = select_configured_scenarios(&config, scenario)?
        .into_iter()
        .cloned()
        .collect();

    let mut inputs = Vec::new();
    for scenario in selected {
        let compare_path = scenario_compare_path(&scenario, out_dir);
        let compare: CompareReceipt = read_json_from_location(&compare_path)
            .with_context(|| format!("read scenario compare {}", compare_path.display()))?;
        let run_id = compare.current_ref.run_id.clone();
        let (probe_compare_ref, probe_compare, probe_compare_warning) =
            load_scenario_probe_compare(&scenario)?;
        inputs.push(ScenarioEvaluateInput {
            config: scenario,
            compare_ref: CompareRef {
                path: Some(compare_path.display().to_string()),
                run_id,
            },
            compare,
            probe_compare_ref,
            probe_compare,
            probe_compare_warning,
        });
    }

    ScenarioUseCase::evaluate(ScenarioEvaluateRequest {
        config,
        inputs,
        workload_name,
        tool: tool_info(),
    })
}

fn run_configured_probe_compares(
    config: &ConfigFile,
    scenario: Option<&str>,
    pretty: bool,
) -> anyhow::Result<()> {
    for scenario in select_configured_scenarios(config, scenario)? {
        let Some((baseline_path, current_path, out_path)) =
            configured_probe_compare_paths(scenario)?
        else {
            continue;
        };

        let baseline: ProbeReceipt =
            read_json_from_location(&baseline_path).with_context(|| {
                format!(
                    "read scenario '{}' baseline probe receipt {}",
                    scenario.name,
                    baseline_path.display()
                )
            })?;
        let current: ProbeReceipt = read_json_from_location(&current_path).with_context(|| {
            format!(
                "read scenario '{}' current probe receipt {}",
                scenario.name,
                current_path.display()
            )
        })?;

        let outcome = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline_ref: CompareRef {
                path: Some(baseline_path.display().to_string()),
                run_id: Some(baseline.run.id.clone()),
            },
            current_ref: CompareRef {
                path: Some(current_path.display().to_string()),
                run_id: Some(current.run.id.clone()),
            },
            baseline,
            current,
            tool: tool_info(),
        })?;

        write_json_to_location(&out_path, &outcome.receipt, pretty)?;
        eprintln!("Probe compare receipt written to {}", out_path.display());
    }

    Ok(())
}

fn configured_probe_compare_paths(
    scenario: &ScenarioConfigFile,
) -> anyhow::Result<Option<(PathBuf, PathBuf, PathBuf)>> {
    let has_probe_inputs = scenario.probe_baseline.is_some() || scenario.probe_current.is_some();
    if !has_probe_inputs {
        return Ok(None);
    }

    let Some(baseline) = scenario.probe_baseline.as_deref() else {
        anyhow::bail!(
            "scenario '{}' probe comparison requires probe_baseline, probe_current, and probe_compare",
            scenario.name
        );
    };
    let Some(current) = scenario.probe_current.as_deref() else {
        anyhow::bail!(
            "scenario '{}' probe comparison requires probe_baseline, probe_current, and probe_compare",
            scenario.name
        );
    };
    let Some(out) = scenario.probe_compare.as_deref() else {
        anyhow::bail!(
            "scenario '{}' probe comparison requires probe_baseline, probe_current, and probe_compare",
            scenario.name
        );
    };

    Ok(Some((
        PathBuf::from(baseline),
        PathBuf::from(current),
        PathBuf::from(out),
    )))
}

fn select_configured_scenarios<'a>(
    config: &'a ConfigFile,
    scenario: Option<&str>,
) -> anyhow::Result<Vec<&'a ScenarioConfigFile>> {
    if let Some(name) = scenario {
        let selected: Vec<_> = config
            .scenarios
            .iter()
            .filter(|candidate| candidate.name == name)
            .collect();
        if selected.is_empty() {
            anyhow::bail!("scenario '{}' is not defined in the config file", name);
        }
        return Ok(selected);
    }

    Ok(config.scenarios.iter().collect())
}

fn scenario_compare_path(scenario: &ScenarioConfigFile, out_dir: &Path) -> PathBuf {
    scenario
        .compare
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| out_dir.join(&scenario.bench).join(COMPARE_RECEIPT_FILE))
}

fn load_scenario_probe_compare(
    scenario: &ScenarioConfigFile,
) -> anyhow::Result<(
    Option<CompareRef>,
    Option<ProbeCompareReceipt>,
    Option<String>,
)> {
    let Some(path) = scenario.probe_compare.as_deref().map(PathBuf::from) else {
        return Ok((None, None, None));
    };

    let compare_ref = CompareRef {
        path: Some(path.display().to_string()),
        run_id: None,
    };
    if !location_exists(&path)? {
        return Ok((
            Some(compare_ref),
            None,
            Some(format!(
                "probe evidence missing for scenario '{}' at {}",
                scenario.name,
                path.display()
            )),
        ));
    }

    let receipt: ProbeCompareReceipt = read_json_from_location(&path)
        .with_context(|| format!("read scenario probe compare {}", path.display()))?;
    let compare_ref = CompareRef {
        path: Some(path.display().to_string()),
        run_id: Some(receipt.run.id.clone()),
    };
    Ok((Some(compare_ref), Some(receipt), None))
}

fn execute_tradeoff_action(action: TradeoffAction) -> anyhow::Result<()> {
    match action {
        TradeoffAction::Evaluate(args) => execute_tradeoff_evaluate(args),
    }
}

fn execute_tradeoff_evaluate(args: TradeoffEvaluateArgs) -> anyhow::Result<()> {
    let config = load_validated_config(&args.config)?;
    let scenario: ScenarioReceipt = read_json(&args.scenario)
        .with_context(|| format!("read scenario receipt {}", args.scenario.display()))?;
    let outcome = evaluate_configured_tradeoffs(&config, &args.config, scenario)?;

    write_json(&args.out, &outcome.receipt, args.pretty)?;
    eprintln!("Tradeoff receipt written to {}", args.out.display());

    match outcome.receipt.verdict.status {
        VerdictStatus::Fail => exit_with_code(2),
        VerdictStatus::Pass | VerdictStatus::Warn | VerdictStatus::Skip => Ok(()),
    }
}

fn evaluate_configured_tradeoffs(
    config: &ConfigFile,
    config_path: &Path,
    mut scenario: ScenarioReceipt,
) -> anyhow::Result<perfgate_app::TradeoffEvaluateOutcome> {
    if config.tradeoffs.is_empty() {
        anyhow::bail!(
            "no [[tradeoff]] entries configured in {}",
            config_path.display()
        );
    }
    let (probe_compares, probe_warnings) = load_tradeoff_probe_compares(&scenario)?;
    scenario.warnings.extend(probe_warnings);

    TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
        scenario,
        probe_compares,
        rules: config.tradeoffs.clone(),
        decision_policy: config.decision_policy.clone(),
        tool: tool_info(),
    })
}

fn load_tradeoff_probe_compares(
    scenario: &ScenarioReceipt,
) -> anyhow::Result<(Vec<ProbeCompareReceipt>, Vec<String>)> {
    let mut paths = BTreeSet::new();
    let mut receipts = Vec::new();
    let mut warnings = Vec::new();

    for component in &scenario.components {
        let Some(reference) = &component.probe_compare_ref else {
            continue;
        };
        let Some(path) = reference.path.as_ref() else {
            warnings.push(format!(
                "scenario component '{}' probe compare reference has no path",
                component.name
            ));
            continue;
        };
        if !paths.insert(path.clone()) {
            continue;
        }

        let path = PathBuf::from(path);
        if !location_exists(&path)? {
            warnings.push(format!(
                "probe compare evidence missing at {}",
                path.display()
            ));
            continue;
        }

        let receipt: ProbeCompareReceipt = read_json_from_location(&path)
            .with_context(|| format!("read tradeoff probe compare {}", path.display()))?;
        receipts.push(receipt);
    }

    Ok((receipts, warnings))
}

fn execute_decision_action(
    action: DecisionAction,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    match action {
        DecisionAction::Suggest(args) => execute_decision_suggest(args),
        DecisionAction::Evaluate(args) => execute_decision_evaluate(args),
        DecisionAction::Bundle(args) => execute_decision_bundle(args),
        DecisionAction::Upload(args) => execute_decision_upload(args, server_flags),
        DecisionAction::History(args) => execute_decision_history(args, server_flags),
        DecisionAction::Latest(args) => execute_decision_latest(args, server_flags),
        DecisionAction::Debt(args) => execute_decision_debt(args, server_flags),
        DecisionAction::Export(args) => execute_decision_export(args, server_flags),
        DecisionAction::Prune(args) => execute_decision_prune(args, server_flags),
    }
}

fn execute_ledger_action(action: LedgerAction, server_flags: &ServerFlags) -> anyhow::Result<()> {
    match action {
        LedgerAction::Doctor(args) => execute_ledger_doctor(args, server_flags),
    }
}

fn execute_ledger_doctor(args: LedgerDoctorArgs, server_flags: &ServerFlags) -> anyhow::Result<()> {
    let config = if args.config.exists() {
        load_config_file(&args.config)
            .with_context(|| format!("load ledger doctor config {}", args.config.display()))?
    } else {
        ConfigFile::default()
    };
    let server_config = server_flags.resolve(&config.baseline_server);

    println!("perfgate ledger doctor");
    print_ledger_readiness_line(
        "Local receipts",
        match ensure_artifact_dir_writable(&args.out_dir) {
            Ok(()) => format!("ready ({} writable)", args.out_dir.display()),
            Err(error) => format!(
                "not ready ({} not writable: {error})",
                args.out_dir.display()
            ),
        },
    );

    if !server_config.is_configured() {
        print_ledger_readiness_line("Server URL", "missing");
        print_ledger_readiness_line("API key", "not configured");
        print_ledger_readiness_line("Project", "not configured");
        print_ledger_readiness_line(
            "Upload mode",
            "local receipts only; server ledger is optional team history",
        );
        print_ledger_readiness_line("History", "not checked; no server URL configured");
        print_ledger_readiness_line("Export", "not checked; no server URL configured");
        print_ledger_readiness_line("Prune dry-run", "not checked; no server URL configured");
        print_ledger_next(&[
            "You do not need server mode yet.",
            "Use `perfgate decision bundle` when decision evidence needs to travel.",
        ]);
        print_ledger_do_not();
        return Ok(());
    }

    let url = server_config
        .url
        .as_deref()
        .expect("checked by is_configured");
    print_ledger_readiness_line("Server URL", format!("configured ({url})"));
    print_ledger_readiness_line(
        "API key",
        if server_config.api_key.is_some() {
            "present"
        } else {
            "missing; okay only for local unauthenticated server mode"
        },
    );
    let project = server_config.project.as_deref();
    print_ledger_readiness_line("Project", project.unwrap_or("missing"));
    print_ledger_readiness_line(
        "Upload mode",
        if server_config.fallback_to_local {
            "advisory (fallback_to_local = true); local receipts remain primary"
        } else {
            "server operations are explicit; local receipts remain primary"
        },
    );

    if args.offline {
        print_ledger_readiness_line("Health", "not checked (--offline)");
        print_ledger_readiness_line("History", "not checked (--offline)");
        print_ledger_readiness_line("Export", "not checked (--offline)");
        print_ledger_readiness_line("Prune dry-run", "not checked (--offline)");
        print_ledger_next(&["Run without `--offline` when you want reachability checks."]);
        print_ledger_do_not();
        return Ok(());
    }

    let client = match ledger_doctor_client(&server_config) {
        Ok(client) => client,
        Err(error) => {
            print_ledger_readiness_line("Health", format!("not checkable: {error:#}"));
            print_ledger_readiness_line("History", "not checked; client setup failed");
            print_ledger_readiness_line("Export", "not checked; client setup failed");
            print_ledger_readiness_line("Prune dry-run", "not checked; client setup failed");
            print_ledger_next(&["Fix the server URL or use local receipts only."]);
            print_ledger_do_not();
            return Ok(());
        }
    };

    match with_tokio_runtime(async { client.health_check().await.map_err(anyhow::Error::from) }) {
        Ok(health) if health.status == "healthy" => {
            print_ledger_readiness_line("Health", "reachable");
        }
        Ok(health) => {
            print_ledger_readiness_line(
                "Health",
                format!("unhealthy response ({})", health.status),
            );
        }
        Err(error) => {
            print_ledger_readiness_line("Health", format!("unreachable: {error:#}"));
            print_ledger_readiness_line("History", "not checked; health failed");
            print_ledger_readiness_line("Export", "not checked; health failed");
            print_ledger_readiness_line("Prune dry-run", "not checked; health failed");
            print_ledger_next(&[
                "Keep local receipts and retry ledger checks after the server is healthy.",
            ]);
            print_ledger_do_not();
            return Ok(());
        }
    }

    let Some(project) = project else {
        print_ledger_readiness_line("History", "not checked; project missing");
        print_ledger_readiness_line("Export", "not checked; project missing");
        print_ledger_readiness_line("Prune dry-run", "not checked; project missing");
        print_ledger_next(&[
            "Set `--project`, `PERFGATE_PROJECT`, or `[baseline_server].project`.",
        ]);
        print_ledger_do_not();
        return Ok(());
    };

    let history_result = with_tokio_runtime(async {
        client
            .list_decisions(project, &ListDecisionsQuery::new().with_limit(1))
            .await
            .map_err(anyhow::Error::from)
    });
    match history_result {
        Ok(response) => {
            print_ledger_readiness_line(
                "History",
                format!(
                    "reachable ({} decision record(s) visible)",
                    response.pagination.total
                ),
            );
            print_ledger_readiness_line("Export", "available through decision export");
        }
        Err(error) => {
            print_ledger_readiness_line("History", format!("not reachable: {error:#}"));
            print_ledger_readiness_line("Export", "not available until history is reachable");
        }
    }

    let prune_result = with_tokio_runtime(async {
        client
            .prune_decisions(
                project,
                &PruneDecisionsRequest {
                    older_than: chrono::Utc::now(),
                    dry_run: true,
                },
            )
            .await
            .map_err(anyhow::Error::from)
    });
    match prune_result {
        Ok(response) => {
            print_ledger_readiness_line(
                "Prune dry-run",
                format!(
                    "available ({} record(s) matched; dry-run deleted {})",
                    response.matched, response.deleted
                ),
            );
        }
        Err(error) => {
            print_ledger_readiness_line("Prune dry-run", format!("not available: {error:#}"));
        }
    }

    print_ledger_next(&[
        "Use `perfgate decision upload` only after local decision receipts are reviewed.",
        "Use `perfgate decision history`, `export`, and `prune --dry-run` for team operations.",
    ]);
    print_ledger_do_not();
    Ok(())
}

fn ledger_doctor_client(server_config: &ResolvedServerConfig) -> anyhow::Result<BaselineClient> {
    let url = server_config
        .url
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!(BASELINE_SERVER_NOT_CONFIGURED))?;
    let mut client_config = ClientConfig::new(url)
        .with_timeout(Duration::from_secs(2))
        .with_retry(RetryConfig::new().with_max_retries(0));
    if let Some(api_key) = &server_config.api_key {
        client_config = client_config.with_api_key(api_key);
    }
    BaselineClient::new(client_config)
        .with_context(|| format!("create ledger doctor client for {url}"))
}

fn print_ledger_readiness_line(label: &str, value: impl AsRef<str>) {
    println!("{label}: {}", value.as_ref());
}

fn print_ledger_next(lines: &[&str]) {
    println!();
    println!("Next:");
    for line in lines {
        println!("  {line}");
    }
}

fn print_ledger_do_not() {
    println!();
    println!("Do not:");
    println!("  make the server ledger part of local correctness.");
    println!("  rerun benchmarks only to repair optional ledger uploads.");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecisionReadiness {
    RunLocalGateFirst,
    SimpleGateEnough,
    PairedModeRecommended,
    StructuredDecisionCandidate,
    StructuredDecisionReady,
    ReadyToBundle,
}

impl DecisionReadiness {
    fn status(self) -> &'static str {
        match self {
            Self::RunLocalGateFirst => "run_local_gate_first",
            Self::SimpleGateEnough => "simple_gate_enough",
            Self::PairedModeRecommended => "paired_mode_recommended",
            Self::StructuredDecisionCandidate => "structured_decision_candidate",
            Self::StructuredDecisionReady => "structured_decision_ready",
            Self::ReadyToBundle => "ready_to_bundle",
        }
    }

    fn meaning(self) -> &'static str {
        match self {
            Self::RunLocalGateFirst => {
                "No compare receipts were found yet; run the local gate before making a decision."
            }
            Self::SimpleGateEnough => {
                "The current evidence does not require structured decision ceremony."
            }
            Self::PairedModeRecommended => {
                "The current evidence looks noisy; paired mode is a better next step than decision policy."
            }
            Self::StructuredDecisionCandidate => {
                "Structured decisions may help, but scenario/tradeoff/probe evidence is not ready yet."
            }
            Self::StructuredDecisionReady => {
                "Scenario and tradeoff policy are configured; decision evaluation is useful now."
            }
            Self::ReadyToBundle => {
                "A decision index already exists; the evidence can be exported as a portable bundle."
            }
        }
    }
}

#[derive(Debug)]
struct DecisionReadinessEvidence {
    compare_found: usize,
    compare_missing: usize,
    has_regression: bool,
    has_improvement: bool,
    high_noise: bool,
    has_probe_config: bool,
    probe_receipts_found: usize,
    decision_index_exists: bool,
}

fn execute_decision_suggest(args: DecisionSuggestArgs) -> anyhow::Result<()> {
    let config = load_validated_config(&args.config)?;
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(|| resolve_configured_out_dir(None, Some(&config)));
    let evidence = collect_decision_readiness_evidence(&config, &out_dir)?;
    let readiness = classify_decision_readiness(&config, &evidence);
    let gaps = decision_readiness_gaps(&config, &evidence);

    println!("perfgate decision suggest");
    println!();
    println!("Status: {}", readiness.status());
    println!("Meaning: {}", readiness.meaning());
    println!();
    println!("Evidence:");
    println!("  benches: {}", config.benches.len());
    println!("  compare receipts found: {}", evidence.compare_found);
    println!("  compare receipts missing: {}", evidence.compare_missing);
    println!("  scenarios: {}", config.scenarios.len());
    println!("  tradeoff rules: {}", config.tradeoffs.len());
    println!(
        "  probe evidence: {}",
        if evidence.has_probe_config {
            format!(
                "configured, {} receipt{}",
                evidence.probe_receipts_found,
                plural(evidence.probe_receipts_found)
            )
        } else {
            "not configured".to_string()
        }
    );
    println!(
        "  decision index: {}",
        if evidence.decision_index_exists {
            out_dir.join("decision.index.json").display().to_string()
        } else {
            "missing".to_string()
        }
    );
    println!();
    println!("Structured decisions may help if:");
    println!("  - one benchmark regressed while another improved");
    println!("  - reviewers need to accept a bounded tradeoff");
    println!("  - probe or scenario evidence explains where work moved");
    if !gaps.is_empty() {
        println!();
        println!("Not ready yet:");
        for gap in &gaps {
            println!("  - {gap}");
        }
    }
    println!();
    println!("Next:");
    for command in decision_readiness_next_commands(readiness, &args.config, &out_dir) {
        println!("  {command}");
    }
    println!("Do not:");
    println!("  do not make structured decisions mandatory for simple local gates");

    Ok(())
}

fn collect_decision_readiness_evidence(
    config: &ConfigFile,
    out_dir: &Path,
) -> anyhow::Result<DecisionReadinessEvidence> {
    let mut compare_found = 0usize;
    let mut compare_missing = 0usize;
    let mut has_regression = false;
    let mut has_improvement = false;
    let mut high_noise = false;

    for (bench_name, path) in decision_compare_paths(config, out_dir) {
        if !path.exists() {
            compare_missing += 1;
            continue;
        }
        compare_found += 1;
        let compare: CompareReceipt = read_json(&path).with_context(|| {
            format!("read compare receipt for {bench_name}: {}", path.display())
        })?;
        has_regression |= is_regression(compare.verdict.status);
        has_improvement |= compare.deltas.values().any(|delta| delta.pct < 0.0);
        high_noise |= compare
            .deltas
            .values()
            .any(|delta| delta.cv.is_some_and(|cv| cv > 0.10));
    }

    let probe_paths = configured_decision_probe_paths(config);
    let has_probe_config = !probe_paths.is_empty();
    let probe_receipts_found = probe_paths.iter().filter(|path| path.exists()).count();

    Ok(DecisionReadinessEvidence {
        compare_found,
        compare_missing,
        has_regression,
        has_improvement,
        high_noise,
        has_probe_config,
        probe_receipts_found,
        decision_index_exists: out_dir.join("decision.index.json").exists(),
    })
}

fn decision_compare_paths(config: &ConfigFile, out_dir: &Path) -> Vec<(String, PathBuf)> {
    let single_compare = out_dir.join(COMPARE_RECEIPT_FILE);
    config
        .benches
        .iter()
        .map(|bench| {
            let per_bench = out_dir.join(&bench.name).join(COMPARE_RECEIPT_FILE);
            let path = if config.benches.len() == 1 && single_compare.exists() {
                single_compare.clone()
            } else {
                per_bench
            };
            (bench.name.clone(), path)
        })
        .collect()
}

fn configured_decision_probe_paths(config: &ConfigFile) -> Vec<PathBuf> {
    config
        .scenarios
        .iter()
        .flat_map(|scenario| {
            [
                scenario.probe_baseline.as_deref(),
                scenario.probe_current.as_deref(),
                scenario.probe_compare.as_deref(),
            ]
        })
        .flatten()
        .map(PathBuf::from)
        .collect()
}

fn classify_decision_readiness(
    config: &ConfigFile,
    evidence: &DecisionReadinessEvidence,
) -> DecisionReadiness {
    if evidence.decision_index_exists {
        return DecisionReadiness::ReadyToBundle;
    }
    if evidence.compare_found == 0 {
        return DecisionReadiness::RunLocalGateFirst;
    }
    if evidence.high_noise {
        return DecisionReadiness::PairedModeRecommended;
    }
    if !config.scenarios.is_empty() && !config.tradeoffs.is_empty() {
        return DecisionReadiness::StructuredDecisionReady;
    }
    if evidence.has_regression || evidence.has_improvement {
        return DecisionReadiness::StructuredDecisionCandidate;
    }
    DecisionReadiness::SimpleGateEnough
}

fn decision_readiness_gaps(
    config: &ConfigFile,
    evidence: &DecisionReadinessEvidence,
) -> Vec<&'static str> {
    let mut gaps = Vec::new();
    if evidence.compare_found == 0 {
        gaps.push("no compare receipts found; run `perfgate check` first");
    }
    if config.scenarios.is_empty() {
        gaps.push("no scenario weights configured");
    }
    if config.tradeoffs.is_empty() {
        gaps.push("no tradeoff rules configured");
    }
    if !evidence.has_probe_config {
        gaps.push("no probe evidence configured");
    } else if evidence.probe_receipts_found == 0 {
        gaps.push("configured probe evidence was not found on disk");
    }
    gaps
}

fn decision_readiness_next_commands(
    readiness: DecisionReadiness,
    config_path: &Path,
    out_dir: &Path,
) -> Vec<String> {
    match readiness {
        DecisionReadiness::RunLocalGateFirst => vec![format!(
            "perfgate check --config {} --all --require-baseline",
            shell_path(config_path)
        )],
        DecisionReadiness::SimpleGateEnough => vec![format!(
            "perfgate check --config {} --all --require-baseline",
            shell_path(config_path)
        )],
        DecisionReadiness::PairedModeRecommended => vec![
            "perfgate paired --name <bench> --baseline-cmd \"<baseline-cmd>\" --current-cmd \"<current-cmd>\" --repeat 10 --out artifacts/perfgate/<bench>/paired.json".to_string(),
            format!("perfgate calibrate --config {} --bench <bench>", shell_path(config_path)),
        ],
        DecisionReadiness::StructuredDecisionCandidate => vec![
            "add scenario weights and tradeoff rules for the review question".to_string(),
            "add probe evidence only if reviewers need to know where work moved".to_string(),
        ],
        DecisionReadiness::StructuredDecisionReady => vec![
            format!("perfgate decision evaluate --config {}", shell_path(config_path)),
            format!(
                "perfgate decision bundle --index {}",
                out_dir.join("decision.index.json").display()
            ),
        ],
        DecisionReadiness::ReadyToBundle => vec![format!(
            "perfgate decision bundle --index {}",
            out_dir.join("decision.index.json").display()
        )],
    }
}

fn execute_decision_bundle(args: DecisionBundleArgs) -> anyhow::Result<()> {
    let index: DecisionArtifactIndex = read_json(&args.index).with_context(|| {
        format!(
            "Failed to read decision artifact index from {}",
            args.index.display()
        )
    })?;
    if index.schema != DECISION_INDEX_SCHEMA_V1 {
        anyhow::bail!(
            "decision artifact index must use schema '{}', got '{}'",
            DECISION_INDEX_SCHEMA_V1,
            index.schema
        );
    }

    let tradeoff_path = resolve_index_artifact_path(&args.index, &index.tradeoff);
    let tradeoff: TradeoffReceipt = read_json(&tradeoff_path).with_context(|| {
        format!(
            "Failed to read tradeoff receipt from {}",
            tradeoff_path.display()
        )
    })?;

    let artifacts = build_decision_bundle_artifacts(&args.index, &index)?;
    let git = git_metadata();
    let metadata = DecisionBundleMetadata {
        index_path: artifact_path_string(&args.index),
        git_ref: args
            .git_ref
            .or_else(|| git.as_ref().and_then(|metadata| metadata.branch.clone())),
        git_sha: args
            .git_sha
            .or_else(|| git.as_ref().and_then(|metadata| metadata.sha.clone())),
    };
    let bundle = DecisionBundleReceipt {
        schema: DECISION_BUNDLE_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        run: tradeoff.run,
        metadata,
        index,
        artifacts,
    };

    let out = args.out.unwrap_or_else(|| {
        args.index
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.join("decision-bundle.json"))
            .unwrap_or_else(|| PathBuf::from("decision-bundle.json"))
    });
    write_json(&out, &bundle, args.pretty)?;
    eprintln!("Decision bundle written to {}", out.display());

    Ok(())
}

fn build_decision_bundle_artifacts(
    index_path: &Path,
    index: &DecisionArtifactIndex,
) -> anyhow::Result<Vec<DecisionBundleArtifact>> {
    let mut artifacts = Vec::new();
    let mut seen = BTreeSet::new();

    push_decision_bundle_artifact(
        &mut artifacts,
        &mut seen,
        index_path,
        &artifact_path_string(index_path),
        DecisionBundleArtifactKind::DecisionIndex,
    )?;
    push_decision_bundle_artifact(
        &mut artifacts,
        &mut seen,
        index_path,
        &index.scenario,
        DecisionBundleArtifactKind::Scenario,
    )?;
    push_decision_bundle_artifact(
        &mut artifacts,
        &mut seen,
        index_path,
        &index.tradeoff,
        DecisionBundleArtifactKind::Tradeoff,
    )?;
    push_decision_bundle_artifact(
        &mut artifacts,
        &mut seen,
        index_path,
        &index.decision,
        DecisionBundleArtifactKind::DecisionMarkdown,
    )?;
    for path in &index.probe_compares {
        push_decision_bundle_artifact(
            &mut artifacts,
            &mut seen,
            index_path,
            path,
            DecisionBundleArtifactKind::ProbeCompare,
        )?;
    }
    for path in &index.compare_receipts {
        push_decision_bundle_artifact(
            &mut artifacts,
            &mut seen,
            index_path,
            path,
            DecisionBundleArtifactKind::CompareReceipt,
        )?;
    }

    Ok(artifacts)
}

fn push_decision_bundle_artifact(
    artifacts: &mut Vec<DecisionBundleArtifact>,
    seen: &mut BTreeSet<String>,
    index_path: &Path,
    artifact_path: &str,
    kind: DecisionBundleArtifactKind,
) -> anyhow::Result<()> {
    let normalized = normalize_artifact_path(artifact_path);
    if !seen.insert(normalized.clone()) {
        return Ok(());
    }

    let resolved = if matches!(kind, DecisionBundleArtifactKind::DecisionIndex) {
        index_path.to_path_buf()
    } else {
        resolve_index_artifact_path(index_path, artifact_path)
    };
    let bytes = fs::read(&resolved)
        .with_context(|| format!("read decision bundle artifact {}", resolved.display()))?;
    let sha256 = sha256_hex(&bytes);

    let (media_type, schema, content) = match kind {
        DecisionBundleArtifactKind::DecisionMarkdown => {
            let text = String::from_utf8(bytes).with_context(|| {
                format!(
                    "decision markdown artifact must be UTF-8: {}",
                    resolved.display()
                )
            })?;
            (
                "text/markdown; charset=utf-8".to_string(),
                None,
                DecisionBundleArtifactContent::Text { value: text },
            )
        }
        DecisionBundleArtifactKind::DecisionIndex
        | DecisionBundleArtifactKind::Scenario
        | DecisionBundleArtifactKind::Tradeoff
        | DecisionBundleArtifactKind::ProbeCompare
        | DecisionBundleArtifactKind::CompareReceipt => {
            let value: JsonValue = serde_json::from_slice(&bytes).with_context(|| {
                format!(
                    "decision bundle artifact must be JSON: {}",
                    resolved.display()
                )
            })?;
            let schema = value
                .get("schema")
                .or_else(|| value.get("report_type"))
                .and_then(|schema| schema.as_str())
                .map(str::to_string);
            (
                "application/json".to_string(),
                schema,
                DecisionBundleArtifactContent::Json { value },
            )
        }
    };

    artifacts.push(DecisionBundleArtifact {
        path: normalized,
        kind,
        media_type,
        sha256,
        schema,
        content,
    });

    Ok(())
}

fn resolve_index_artifact_path(index_path: &Path, artifact_path: &str) -> PathBuf {
    let path = PathBuf::from(artifact_path);
    if path.is_absolute() || path.exists() {
        return path;
    }

    index_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join(artifact_path))
        .filter(|candidate| candidate.exists())
        .unwrap_or(path)
}

fn execute_decision_upload(
    args: DecisionUploadArgs,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let project = server_config.resolve_project(args.project)?;

    let tradeoff: TradeoffReceipt = read_json(&args.file).with_context(|| {
        format!(
            "Failed to read tradeoff receipt from {}",
            args.file.display()
        )
    })?;
    let scenario: Option<ScenarioReceipt> = args
        .scenario_receipt
        .as_ref()
        .map(|path| {
            read_json(path)
                .with_context(|| format!("Failed to read scenario receipt from {}", path.display()))
        })
        .transpose()?;
    let artifact_index: Option<DecisionArtifactIndex> = args
        .index
        .as_ref()
        .map(|path| {
            read_json(path).with_context(|| {
                format!(
                    "Failed to read decision artifact index from {}",
                    path.display()
                )
            })
        })
        .transpose()?;

    let request = UploadDecisionRequest {
        tradeoff,
        scenario,
        artifact_index,
        git_ref: args.git_ref,
        git_sha: args.git_sha,
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let record = client
            .upload_decision(&project, &request)
            .await
            .with_context(|| format!("Failed to upload decision for project '{}'", project))?;
        print_decision_record("Uploaded decision", &record);
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn execute_decision_history(
    args: DecisionHistoryArgs,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let project = server_config.resolve_project(args.project)?;

    let mut query = ListDecisionsQuery::new().with_limit(args.limit);
    if let Some(scenario) = args.scenario {
        query = query.with_scenario(scenario);
    }
    if let Some(status) = args.status {
        query = query.with_status(status);
    }
    if let Some(verdict) = args.verdict {
        query = query.with_verdict(verdict);
    }
    if let Some(review_required) = args.review_required {
        query = query.with_review_required(review_required);
    }
    if let Some(accepted) = args.accepted {
        query = query.with_accepted(accepted);
    }
    if let Some(rule) = args.rule {
        query = query.with_rule(rule);
    }

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let response = client
            .list_decisions(&project, &query)
            .await
            .with_context(|| format!("Failed to list decisions for project '{}'", project))?;

        if response.decisions.is_empty() {
            println!("No decisions found for project '{}'.", project);
        } else {
            println!(
                "Decision history for {} ({} of {}):",
                project,
                response.decisions.len(),
                response.pagination.total
            );
            for record in &response.decisions {
                print_decision_record("  Decision", record);
            }
        }

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn execute_decision_latest(
    args: DecisionLatestArgs,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let project = server_config.resolve_project(args.project)?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let record = client
            .latest_decision(&project)
            .await
            .with_context(|| format!("Failed to get latest decision for project '{}'", project))?;
        print_decision_record("Latest decision", &record);
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn execute_decision_debt(args: DecisionDebtArgs, server_flags: &ServerFlags) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let project = server_config.resolve_project(args.project)?;
    let cutoff = decision_debt_cutoff(args.days)?;
    let query = ListDecisionsQuery::new().with_limit(args.limit);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let response = client
            .list_decisions(&project, &query)
            .await
            .with_context(|| format!("Failed to list decisions for project '{}'", project))?;

        let records: Vec<_> = response
            .decisions
            .iter()
            .filter(|record| {
                cutoff
                    .map(|cutoff| record.created_at.timestamp() >= cutoff)
                    .unwrap_or(true)
            })
            .collect();

        if response.pagination.has_more {
            eprintln!(
                "Warning: scanned {} of {} decisions; increase --limit for a fuller debt summary.",
                response.decisions.len(),
                response.pagination.total
            );
        }

        print_decision_debt_summary(&project, args.days, &records);
        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn execute_decision_export(
    args: DecisionExportArgs,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let project = server_config.resolve_project(args.project)?;
    let cutoff = decision_debt_cutoff(args.days)?;
    let query = ListDecisionsQuery::new().with_limit(args.limit);

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let response = client
            .list_decisions(&project, &query)
            .await
            .with_context(|| format!("Failed to export decisions for project '{}'", project))?;

        let records: Vec<_> = response
            .decisions
            .iter()
            .filter(|record| {
                cutoff
                    .map(|cutoff| record.created_at.timestamp() >= cutoff)
                    .unwrap_or(true)
            })
            .collect();

        if response.pagination.has_more {
            eprintln!(
                "Warning: exported {} of {} fetched decisions; increase --limit for a fuller export.",
                response.decisions.len(),
                response.pagination.total
            );
        }

        let rendered = render_decision_export(&project, args.days, &records, args.format)?;
        if let Some(out) = args.out {
            if let Some(parent) = out.parent()
                && !parent.as_os_str().is_empty()
            {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create {}", parent.display()))?;
            }
            atomic_write(&out, rendered.as_bytes())?;
            eprintln!(
                "Exported {} decision record(s) to {}",
                records.len(),
                out.display()
            );
        } else {
            print!("{rendered}");
        }

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn execute_decision_prune(
    args: DecisionPruneArgs,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let project = server_config.resolve_project(args.project)?;
    let older_than = decision_prune_cutoff(&args.older_than)?;

    if !args.dry_run && !args.force {
        eprintln!(
            "Warning: This will permanently delete decision records older than '{}' from project '{}'.",
            args.older_than, project
        );
        eprintln!("Use --dry-run to preview or --force to confirm deletion.");
        anyhow::bail!("Decision prune not confirmed. Use --force to proceed.");
    }

    let request = PruneDecisionsRequest {
        older_than,
        dry_run: args.dry_run,
    };

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let response = client
            .prune_decisions(&project, &request)
            .await
            .with_context(|| format!("Failed to prune decisions for project '{}'", project))?;

        if response.dry_run {
            println!(
                "Decision prune dry run for {}: {} record(s) older than {} would be deleted.",
                response.project,
                response.matched,
                response.older_than.to_rfc3339()
            );
        } else {
            println!(
                "Pruned {} decision record(s) from {} older than {}.",
                response.deleted,
                response.project,
                response.older_than.to_rfc3339()
            );
        }

        if !response.decision_ids.is_empty() {
            println!("decision_ids={}", response.decision_ids.join(","));
        }

        Ok::<(), anyhow::Error>(())
    })?;

    Ok(())
}

fn render_decision_export(
    project: &str,
    days: u32,
    records: &[&perfgate_client::DecisionRecord],
    format: DecisionExportFormat,
) -> anyhow::Result<String> {
    match format {
        DecisionExportFormat::Jsonl => {
            let mut out = String::new();
            for record in records {
                out.push_str(&serde_json::to_string(record)?);
                out.push('\n');
            }
            Ok(out)
        }
        DecisionExportFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
            "project": project,
            "days": days,
            "exported": records.len(),
            "decisions": records,
        }))
        .map(|mut json| {
            json.push('\n');
            json
        })
        .map_err(Into::into),
    }
}

fn decision_debt_cutoff(days: u32) -> anyhow::Result<Option<i64>> {
    if days == 0 {
        return Ok(None);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX_EPOCH")?
        .as_secs() as i64;
    Ok(Some(now - i64::from(days) * 86_400))
}

fn decision_prune_cutoff(older_than: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    let duration = parse_retention_duration(older_than)?;
    let chrono_duration = chrono::Duration::from_std(duration)
        .with_context(|| format!("retention duration is too large: {older_than}"))?;
    Ok(chrono::Utc::now() - chrono_duration)
}

fn parse_retention_duration(input: &str) -> anyhow::Result<Duration> {
    let trimmed = input.trim();
    if let Some(days) = trimmed.strip_suffix('d') {
        let days: u64 = days
            .parse()
            .with_context(|| format!("invalid retention duration: {input}"))?;
        return Ok(Duration::from_secs(days.saturating_mul(86_400)));
    }
    if let Some(weeks) = trimmed.strip_suffix('w') {
        let weeks: u64 = weeks
            .parse()
            .with_context(|| format!("invalid retention duration: {input}"))?;
        return Ok(Duration::from_secs(weeks.saturating_mul(7 * 86_400)));
    }
    parse_duration(trimmed)
}

#[derive(Default)]
struct DecisionDebtAreaSummary {
    accepted_count: u32,
    review_required_count: u32,
    rule_counts: BTreeMap<String, u32>,
    max_cap_used: Option<f64>,
    max_accepted_delta: Option<DecisionDebtMetricDelta>,
}

#[derive(Clone)]
struct DecisionDebtMetricDelta {
    metric: String,
    regression: f64,
}

fn print_decision_debt_summary(
    project: &str,
    days: u32,
    records: &[&perfgate_client::DecisionRecord],
) {
    let mut areas: BTreeMap<String, DecisionDebtAreaSummary> = BTreeMap::new();
    let mut accepted_total = 0_u32;
    let mut review_required_total = 0_u32;

    for record in records {
        if record.accepted_rules.is_empty() {
            continue;
        }

        let area_name = record
            .scenario
            .clone()
            .unwrap_or_else(|| "unspecified".to_string());
        let area = areas.entry(area_name).or_default();
        area.accepted_count += 1;
        accepted_total += 1;

        if record.review_required {
            area.review_required_count += 1;
            review_required_total += 1;
        }

        for rule in &record.accepted_rules {
            *area.rule_counts.entry(rule.clone()).or_insert(0) += 1;
        }

        if let Some(cap_used) = decision_record_max_cap_used(record) {
            area.max_cap_used = Some(area.max_cap_used.map_or(cap_used, |current| {
                if cap_used > current {
                    cap_used
                } else {
                    current
                }
            }));
        }

        if let Some(delta) = decision_record_max_accepted_delta(record) {
            area.max_accepted_delta = Some(area.max_accepted_delta.as_ref().map_or_else(
                || delta.clone(),
                |current| max_metric_delta(current, &delta),
            ));
        }
    }

    let window = if days == 0 {
        "all fetched records".to_string()
    } else {
        format!("last {days} days")
    };

    println!(
        "Decision debt for {} ({}, {} records scanned):",
        project,
        window,
        records.len()
    );
    println!("Accepted tradeoff records: {accepted_total}");
    println!("Review-required accepted records: {review_required_total}");

    if areas.is_empty() {
        println!("\nNo accepted tradeoffs found.");
        return;
    }

    println!();
    println!(
        "{:<24} {:>5} {:>6} {:>8} {:>14} {:>11}  common rule",
        "area", "count", "review", "cap used", "accepted delta", "budget used"
    );

    for (area, summary) in areas {
        println!(
            "{:<24} {:>5} {:>6} {:>8} {:>14} {:>11}  {}",
            area,
            summary.accepted_count,
            summary.review_required_count,
            format_cap_used(summary.max_cap_used),
            format_accepted_delta(summary.max_accepted_delta.as_ref()),
            format_budget_headroom_used(None),
            most_common_rule(&summary.rule_counts)
        );
    }
}

fn decision_record_max_cap_used(record: &perfgate_client::DecisionRecord) -> Option<f64> {
    record
        .tradeoff_receipt
        .rules
        .iter()
        .filter(|rule| rule.accepted)
        .flat_map(|rule| rule.allowances.iter())
        .filter_map(|allowance| {
            if allowance.max_regression <= 0.0 {
                return None;
            }
            let observed = allowance.observed_regression?;
            Some((observed.max(0.0) / allowance.max_regression).max(0.0))
        })
        .reduce(f64::max)
}

fn decision_record_max_accepted_delta(
    record: &perfgate_client::DecisionRecord,
) -> Option<DecisionDebtMetricDelta> {
    record
        .tradeoff_receipt
        .rules
        .iter()
        .filter(|rule| rule.accepted)
        .filter_map(|rule| {
            let configured = record
                .tradeoff_receipt
                .configured_rules
                .iter()
                .find(|configured| configured.name == rule.name)?;
            let metric = configured.if_failed.as_str();
            let delta = record.tradeoff_receipt.weighted_deltas.get(metric)?;
            if delta.regression <= 0.0 {
                return None;
            }
            Some(DecisionDebtMetricDelta {
                metric: metric.to_string(),
                regression: delta.regression,
            })
        })
        .reduce(|current, next| max_metric_delta(&current, &next))
}

fn max_metric_delta(
    left: &DecisionDebtMetricDelta,
    right: &DecisionDebtMetricDelta,
) -> DecisionDebtMetricDelta {
    if right.regression > left.regression {
        right.clone()
    } else {
        left.clone()
    }
}

fn format_cap_used(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.0}%", value * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_accepted_delta(value: Option<&DecisionDebtMetricDelta>) -> String {
    value
        .map(|delta| format!("{} +{:.1}%", delta.metric, delta.regression * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_budget_headroom_used(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.0}%", value * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn most_common_rule(rule_counts: &BTreeMap<String, u32>) -> String {
    rule_counts
        .iter()
        .max_by(|(left_rule, left_count), (right_rule, right_count)| {
            left_count
                .cmp(right_count)
                .then_with(|| right_rule.cmp(left_rule))
        })
        .map(|(rule, count)| format!("{rule} ({count})"))
        .unwrap_or_else(|| "none".to_string())
}

fn print_decision_record(label: &str, record: &perfgate_client::DecisionRecord) {
    let scenario = record.scenario.as_deref().unwrap_or("unspecified");
    let git_ref = record.git_ref.as_deref().unwrap_or("unknown");
    let review = if record.review_required {
        "review-required"
    } else {
        "no-review"
    };
    let accepted_rules = if record.accepted_rules.is_empty() {
        "none".to_string()
    } else {
        record.accepted_rules.join(",")
    };

    println!(
        "{} {} scenario={} status={} verdict={} {} git_ref={} accepted_rules={} created_at={}",
        label,
        record.id,
        scenario,
        record.status.as_str(),
        record.verdict.as_str(),
        review,
        git_ref,
        accepted_rules,
        record.created_at
    );
}

fn execute_decision_evaluate(args: DecisionEvaluateArgs) -> anyhow::Result<()> {
    let config = load_validated_config(&args.config)?;
    let out_dir = resolve_configured_out_dir(args.out_dir.as_ref(), Some(&config));
    let scenario_out = args
        .scenario_out
        .clone()
        .unwrap_or_else(|| out_dir.join("scenario.json"));
    let tradeoff_out = args
        .tradeoff_out
        .clone()
        .unwrap_or_else(|| out_dir.join("tradeoff.json"));
    let decision_out = args
        .decision_out
        .clone()
        .unwrap_or_else(|| out_dir.join("decision.md"));
    let index_out = args
        .index_out
        .clone()
        .unwrap_or_else(|| out_dir.join("decision.index.json"));

    run_configured_probe_compares(&config, args.scenario.as_deref(), args.pretty)?;

    let scenario_outcome = evaluate_configured_scenarios(
        config.clone(),
        &args.config,
        args.scenario.as_deref(),
        args.workload_name,
        &out_dir,
    )?;
    let scenario_receipt = scenario_outcome.receipt;
    write_json(&scenario_out, &scenario_receipt, args.pretty)?;
    eprintln!("Scenario receipt written to {}", scenario_out.display());

    let tradeoff_outcome =
        evaluate_configured_tradeoffs(&config, &args.config, scenario_receipt.clone())?;
    write_json(&tradeoff_out, &tradeoff_outcome.receipt, args.pretty)?;
    eprintln!("Tradeoff receipt written to {}", tradeoff_out.display());

    let markdown = render_tradeoff_markdown(&tradeoff_outcome.receipt);
    if let Some(parent) = decision_out
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }
    atomic_write(&decision_out, markdown.as_bytes())?;
    eprintln!("Decision markdown written to {}", decision_out.display());

    let index = build_decision_artifact_index(
        &scenario_out,
        &tradeoff_out,
        &decision_out,
        &scenario_receipt,
    );
    write_json(&index_out, &index, args.pretty)?;
    eprintln!("Decision artifact index written to {}", index_out.display());

    match tradeoff_outcome.receipt.verdict.status {
        VerdictStatus::Fail => exit_with_code(2),
        VerdictStatus::Pass | VerdictStatus::Warn | VerdictStatus::Skip => Ok(()),
    }
}

fn build_decision_artifact_index(
    scenario_out: &Path,
    tradeoff_out: &Path,
    decision_out: &Path,
    scenario: &ScenarioReceipt,
) -> DecisionArtifactIndex {
    let mut probe_compares = BTreeSet::new();
    let mut compare_receipts = BTreeSet::new();

    for component in &scenario.components {
        if let Some(reference) = &component.probe_compare_ref
            && let Some(path) = reference.path.as_ref()
        {
            probe_compares.insert(normalize_artifact_path(path));
        }
        if let Some(reference) = &component.compare_ref
            && let Some(path) = reference.path.as_ref()
        {
            compare_receipts.insert(normalize_artifact_path(path));
        }
    }

    DecisionArtifactIndex {
        schema: DECISION_INDEX_SCHEMA_V1.to_string(),
        scenario: artifact_path_string(scenario_out),
        tradeoff: artifact_path_string(tradeoff_out),
        decision: artifact_path_string(decision_out),
        probe_compares: probe_compares.into_iter().collect(),
        compare_receipts: compare_receipts.into_iter().collect(),
    }
}

fn artifact_path_string(path: &Path) -> String {
    normalize_artifact_path(&path.display().to_string())
}

fn normalize_artifact_path(path: &str) -> String {
    path.replace('\\', "/")
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
    let host = host_probe.probe(&HostProbeOptions {
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

fn execute_explain_action(action: ExplainAction) -> anyhow::Result<()> {
    match action {
        ExplainAction::Artifacts(args) => execute_explain_artifacts(args),
    }
}

fn execute_explain_artifacts(args: ExplainArtifactsArgs) -> anyhow::Result<()> {
    let mut known = Vec::new();
    let mut unknown = Vec::new();
    collect_artifact_files(&args.out_dir, &args.out_dir, &mut known, &mut unknown)?;

    println!("perfgate artifact explanation");
    println!();

    if !args.out_dir.exists() {
        println!("Status: no_artifacts");
        println!(
            "Meaning: {} does not exist yet; run a check or decision command first.",
            args.out_dir.display()
        );
        println!("Artifacts:");
        println!("  none");
        println!("Next:");
        println!("  perfgate check --config perfgate.toml --all");
        println!("Do not:");
        println!("  commit generated artifact directories before reviewing repo policy");
        return Ok(());
    }

    if known.is_empty() {
        println!("Status: no_known_artifacts");
        println!(
            "Meaning: {} exists, but no known perfgate receipt files were found.",
            args.out_dir.display()
        );
        println!("Artifacts:");
        if unknown.is_empty() {
            println!("  none");
        } else {
            for path in &unknown {
                println!("  {}  unrecognized file", path.display());
            }
        }
        println!("Next:");
        println!("  perfgate check --config perfgate.toml --all");
        println!("Do not:");
        println!("  infer a verdict from unknown files alone");
        return Ok(());
    }

    println!("Status: artifacts_found");
    println!("Meaning: known perfgate receipts or review artifacts are present.");
    println!("Artifacts:");
    for (path, role) in &known {
        println!("  {:<32} {}", path.display(), role);
    }
    if !unknown.is_empty() {
        println!("  unknown:");
        for path in &unknown {
            println!("    {}  unrecognized file", path.display());
        }
    }
    println!("Next:");
    for command in artifact_next_commands(&known) {
        println!("  {command}");
    }
    println!("Do not:");
    println!("  treat artifacts as durable baselines unless promoted intentionally");

    Ok(())
}

fn collect_artifact_files(
    root: &Path,
    dir: &Path,
    known: &mut Vec<(PathBuf, &'static str)>,
    unknown: &mut Vec<PathBuf>,
) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_artifact_files(root, &path, known, unknown)?;
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            unknown.push(relative);
            continue;
        };

        if let Some(role) = known_artifact_role(name) {
            known.push((relative, role));
        } else {
            unknown.push(relative);
        }
    }

    known.sort_by(|left, right| left.0.cmp(&right.0));
    unknown.sort();
    Ok(())
}

fn known_artifact_role(file_name: &str) -> Option<&'static str> {
    match file_name {
        RUN_RECEIPT_FILE => Some("raw measurement receipt"),
        COMPARE_RECEIPT_FILE => Some("baseline/current comparison receipt"),
        "report.json" => Some("machine-readable verdict summary"),
        "comment.md" => Some("PR-ready human summary"),
        "repair_context.json" => Some("local reproduction and repair hints"),
        "decision.md" => Some("human-readable performance decision"),
        "decision.index.json" => Some("index of decision evidence artifacts"),
        "decision-bundle.json" => Some("portable decision evidence bundle"),
        "probe-compare.json" => Some("named probe baseline/current comparison"),
        "scenario.json" => Some("weighted workload scenario receipt"),
        "tradeoff.json" => Some("tradeoff policy evaluation receipt"),
        _ => None,
    }
}

fn artifact_next_commands(known: &[(PathBuf, &'static str)]) -> Vec<String> {
    let has = |name: &str| {
        known
            .iter()
            .any(|(path, _)| path.file_name().and_then(|file| file.to_str()) == Some(name))
    };

    let mut commands = Vec::new();
    if has("comment.md") {
        commands.push("inspect artifacts/perfgate/comment.md or the per-bench comment.md".into());
    }
    if has("repair_context.json") {
        commands.push("inspect repair_context.json for local reproduction and repair hints".into());
    }
    if has("decision.index.json") {
        commands
            .push("perfgate decision bundle --index artifacts/perfgate/decision.index.json".into());
    }
    if has(COMPARE_RECEIPT_FILE) {
        commands.push("perfgate check --config perfgate.toml --all --require-baseline".into());
    }
    if commands.is_empty() {
        commands.push("perfgate check --config perfgate.toml --all".into());
    }
    commands
}

fn resolve_benchmark_suggestion_profile(
    requested: BenchmarkSuggestionProfile,
    scan_dir: &Path,
) -> BenchmarkSuggestionProfile {
    if requested != BenchmarkSuggestionProfile::Auto {
        return requested;
    }

    if scan_dir.join("package.json").exists() {
        return BenchmarkSuggestionProfile::Node;
    }

    let cargo_toml = scan_dir.join("Cargo.toml");
    if let Ok(content) = fs::read_to_string(cargo_toml) {
        if content.contains("[workspace]") {
            return BenchmarkSuggestionProfile::RustWorkspace;
        }
        return BenchmarkSuggestionProfile::RustCli;
    }

    BenchmarkSuggestionProfile::GenericCommand
}

fn render_benchmark_suggestions(profile: BenchmarkSuggestionProfile) -> String {
    match profile {
        BenchmarkSuggestionProfile::Auto => {
            render_benchmark_suggestions(BenchmarkSuggestionProfile::GenericCommand)
        }
        BenchmarkSuggestionProfile::RustCli => r#"
# Benchmark suggestions (rust-cli)
# Review and edit before committing. These are candidates, not policy.
#
# Fast first-hour check: low setup cost and useful for smoke gating.
# [[bench]]
# name = "cli-help"
# command = ["cargo", "run", "-q", "--", "--help"]
#
# Heavier check: keep advisory until calibrated.
# [[bench]]
# name = "cli-release-help"
# command = ["cargo", "run", "--release", "--", "--help"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::RustWorkspace => r#"
# Benchmark suggestions (rust-workspace)
# Review and edit before committing. These are candidates, not policy.
#
# Fast first-hour check: choose one small package or command with low setup cost.
# [[bench]]
# name = "workspace-smoke"
# command = ["cargo", "test", "-p", "your-package", "--no-fail-fast"]
#
# Heavier check: compile-heavy workspace tests should stay advisory until calibrated.
# [[bench]]
# name = "workspace-test"
# command = ["cargo", "test", "--workspace", "--no-fail-fast"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::Node => r#"
# Benchmark suggestions (node)
# Review and edit before committing. These are candidates, not policy.
#
# Fast first-hour check: a dedicated benchmark script with stable input.
# [[bench]]
# name = "node-bench"
# command = ["node", "scripts/bench.js"]
#
# Package-manager path: useful when `npm run bench` already exists.
# [[bench]]
# name = "npm-bench"
# command = ["npm", "run", "bench"]
"#
        .to_string(),
        BenchmarkSuggestionProfile::GenericCommand => r#"
# Benchmark suggestions (generic-command)
# Review and edit before committing. These are candidates, not policy.
#
# Fast first-hour check: a stable command that measures the workload directly.
# [[bench]]
# name = "command-smoke"
# command = ["./scripts/bench.sh"]
#
# Language-neutral example: replace this with your real benchmark command.
# [[bench]]
# name = "my-command"
# command = ["your-benchmark-command", "--flag"]
"#
        .to_string(),
    }
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
    let mut toml_content = render_config_toml(&config);
    let suggestion_profile = args
        .suggest_benches
        .map(|profile| resolve_benchmark_suggestion_profile(profile, &scan_dir));
    if let Some(profile) = suggestion_profile {
        toml_content.push_str(&render_benchmark_suggestions(profile));
    }

    fs::write(&args.output, &toml_content)
        .with_context(|| format!("write {}", args.output.display()))?;
    eprintln!("Wrote {}", args.output.display());
    if let Some(profile) = suggestion_profile {
        eprintln!(
            "Appended reviewable benchmark suggestions ({}) to {}.",
            profile.as_str(),
            args.output.display()
        );
        eprintln!("Review and edit suggestions before committing baselines.");
    }

    let baseline_dir = config
        .defaults
        .baseline_dir
        .as_deref()
        .unwrap_or(DEFAULT_FALLBACK_BASELINE_DIR);
    if !is_remote_storage_uri(baseline_dir) {
        let baseline_dir = PathBuf::from(baseline_dir);
        fs::create_dir_all(&baseline_dir)
            .with_context(|| format!("create {}", baseline_dir.display()))?;
        let gitkeep = baseline_dir.join(".gitkeep");
        if !gitkeep.exists() || args.yes {
            fs::write(&gitkeep, "").with_context(|| format!("write {}", gitkeep.display()))?;
            eprintln!("Wrote {}", gitkeep.display());
        }
    }

    // CI workflow
    let generated_workflow_path = if let Some(platform) = ci_platform {
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
        Some(workflow_path)
    } else {
        None
    };

    let setup_dir = PathBuf::from(".perfgate");
    fs::create_dir_all(&setup_dir).with_context(|| format!("create {}", setup_dir.display()))?;
    let setup_readme = setup_dir.join("README.md");
    if !setup_readme.exists() || args.yes {
        fs::write(
            &setup_readme,
            render_onboarding_readme(
                &args.output,
                generated_workflow_path.as_deref(),
                !benchmarks.is_empty(),
            ),
        )
        .with_context(|| format!("write {}", setup_readme.display()))?;
        eprintln!("Wrote {}", setup_readme.display());
    }

    eprintln!("\nNext:");
    if benchmarks.is_empty() {
        eprintln!(
            "  1. Add at least one [[bench]] entry to {}.",
            args.output.display()
        );
        eprintln!("     Example:");
        eprintln!("       [[bench]]");
        eprintln!("       name = \"my-command\"");
        eprintln!("       command = [\"your-benchmark-command\", \"--flag\"]");
        eprintln!("     Replace the command with what measures this repo, for example:");
        eprintln!("       command = [\"cargo\", \"bench\", \"--bench\", \"my-bench\"]");
        eprintln!("       command = [\"node\", \"scripts/bench.js\"]");
        eprintln!(
            "  2. Run: perfgate check --config {} --all",
            args.output.display()
        );
        eprintln!("  3. Promote a trusted first baseline:");
        eprintln!(
            "     perfgate baseline promote --config {} --all",
            args.output.display()
        );
        if let Some(workflow_path) = &generated_workflow_path {
            eprintln!(
                "  4. Commit {}, {}, baselines/.gitkeep, and .perfgate/README.md",
                args.output.display(),
                workflow_path.display()
            );
        } else {
            eprintln!(
                "  4. Commit {}, baselines/.gitkeep, and .perfgate/README.md",
                args.output.display()
            );
        }
        return Ok(());
    }
    eprintln!(
        "  1. Run: perfgate check --config {} --all",
        args.output.display()
    );
    eprintln!("  2. Promote a trusted first baseline:");
    eprintln!(
        "     perfgate baseline promote --config {} --all",
        args.output.display()
    );
    if let Some(workflow_path) = &generated_workflow_path {
        eprintln!(
            "  3. Commit {}, {}, baselines/.gitkeep, and .perfgate/README.md",
            args.output.display(),
            workflow_path.display()
        );
    } else {
        eprintln!(
            "  3. Commit {}, baselines/.gitkeep, and .perfgate/README.md",
            args.output.display()
        );
    }

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

fn serve_db_path(db: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    match db {
        Some(path) => Ok(path),
        None => Ok(default_data_dir()?.join("data.db")),
    }
}

fn serve_api_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/api/v1")
}

/// Start the local dashboard server.
fn execute_serve(args: ServeArgs) -> anyhow::Result<()> {
    let db_path = serve_db_path(args.db)?;

    if args.doctor {
        return execute_serve_doctor(args.port, &db_path);
    }

    // Ensure the parent directory exists for the database file.
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create database directory {}", parent.display()))?;
    }

    let bind_addr = format!("127.0.0.1:{}", args.port);
    let url = format!("http://127.0.0.1:{}", args.port);
    let api_url = serve_api_url(args.port);
    let health_url = format!("http://127.0.0.1:{}/health", args.port);

    eprintln!("perfgate serve");
    eprintln!("  database : {}", db_path.display());
    eprintln!("  dashboard: {url}");
    eprintln!("  api      : {api_url}");
    eprintln!("  health   : {health_url}");
    eprintln!("  auth     : disabled (local mode)");
    eprintln!("  upload   : PERFGATE_LOCAL_DB={api_url} perfgate run --local-db ...");
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

fn execute_serve_doctor(port: u16, db_path: &Path) -> anyhow::Result<()> {
    let bind_addr = format!("127.0.0.1:{port}");
    let url = format!("http://127.0.0.1:{port}");
    let api_url = serve_api_url(port);
    let health_url = format!("http://127.0.0.1:{port}/health");
    let checks = vec![
        serve_database_directory_check(db_path),
        serve_sqlite_check(db_path),
        serve_bind_check(&bind_addr),
    ];

    println!("perfgate serve doctor");
    println!();
    println!("{:<4} {:<18} {}", "INFO", "database", db_path.display());
    println!("{:<4} {:<18} {}", "INFO", "dashboard", url);
    println!("{:<4} {:<18} {}", "INFO", "api", api_url);
    println!("{:<4} {:<18} {}", "INFO", "health", health_url);
    for check in &checks {
        println!(
            "{:<4} {:<18} {}",
            check.status.as_str(),
            check.name,
            check.detail
        );
    }

    let failed = checks
        .iter()
        .filter(|check| check.status == DoctorStatus::Fail)
        .count();
    println!();
    println!("Summary: {failed} failed check{}", plural(failed));

    if failed > 0 {
        anyhow::bail!("serve doctor found {failed} failed check{}", plural(failed));
    }

    Ok(())
}

fn serve_database_directory_check(db_path: &Path) -> DoctorCheck {
    match ensure_database_directory_writable(db_path) {
        Ok(parent) => DoctorCheck::ok("database dir", format!("{} writable", parent.display())),
        Err(error) => DoctorCheck::fail("database dir", error.to_string()),
    }
}

fn ensure_database_directory_writable(db_path: &Path) -> anyhow::Result<PathBuf> {
    let parent = db_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("create database directory {}", parent.display()))?;

    let probe = parent.join(format!(".perfgate-serve-doctor-{}.tmp", std::process::id()));
    fs::write(&probe, b"perfgate serve doctor\n")
        .with_context(|| format!("write probe file {}", probe.display()))?;
    fs::remove_file(&probe).with_context(|| format!("remove probe file {}", probe.display()))?;
    Ok(parent.to_path_buf())
}

fn serve_sqlite_check(db_path: &Path) -> DoctorCheck {
    match perfgate_server::SqliteStore::new(db_path, None) {
        Ok(_) => DoctorCheck::ok("sqlite storage", "opened, initialized, and WAL configured"),
        Err(error) => DoctorCheck::fail(
            "sqlite storage",
            format!("{} not usable: {error}", db_path.display()),
        ),
    }
}

fn serve_bind_check(bind_addr: &str) -> DoctorCheck {
    match std::net::TcpListener::bind(bind_addr) {
        Ok(listener) => {
            drop(listener);
            DoctorCheck::ok("dashboard bind", format!("{bind_addr} available"))
        }
        Err(error) => DoctorCheck::fail(
            "dashboard bind",
            format!("{bind_addr} unavailable: {error}"),
        ),
    }
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
        tradeoff,
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
        github::render_comment(&receipt, &options)
    } else if let Some(report_path) = report {
        let report_receipt: PerfgateReport = read_json(&report_path)?;
        let options = CommentOptions {
            blame_text,
            explain_text: None,
        };
        github::render_comment_from_report(&report_receipt, &options)
    } else if let Some(tradeoff_path) = tradeoff {
        let tradeoff_receipt: TradeoffReceipt = read_json(&tradeoff_path)?;
        github::render_comment_from_tradeoff(&tradeoff_receipt)
    } else {
        anyhow::bail!("Either --compare, --report, or --tradeoff is required");
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

    let (owner, repo_name) = github::parse_github_repository(&repo_str).ok_or_else(|| {
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
                .and_then(|r| github::parse_pr_number_from_ref(&r))
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

/// Execute baseline service administration actions.
fn execute_admin_action(action: AdminAction, server_flags: &ServerFlags) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let rt = tokio::runtime::Runtime::new()?;

    match action {
        AdminAction::Keys { action } => match action {
            KeyAction::Create {
                project,
                role,
                description,
                pattern,
            } => {
                let role: Role = role.into();
                let description =
                    description.unwrap_or_else(|| format!("{} key for {}", role, project));
                let request = CreateKeyRequest {
                    description,
                    role,
                    project,
                    pattern,
                    expires_at: None,
                };

                rt.block_on(async {
                    let response = client.create_key(&request).await.with_context(|| {
                        format!(
                            "Failed to create API key (project: {}, role: {})",
                            request.project, request.role
                        )
                    })?;

                    eprintln!(
                        "Created API key {} for project {} with role {}",
                        response.id, response.project, response.role
                    );
                    eprintln!("Store this key now; it will not be shown again.");
                    println!("id\trole\tproject\tkey");
                    println!(
                        "{}\t{}\t{}\t{}",
                        response.id, response.role, response.project, response.key
                    );

                    Ok::<(), anyhow::Error>(())
                })?
            }
            KeyAction::List {
                project,
                include_revoked,
            } => {
                let project_filter = project.or_else(|| server_config.project.clone());

                rt.block_on(async {
                    let response = client
                        .list_keys()
                        .await
                        .context("Failed to list API keys")?;
                    let mut keys = response.keys;

                    if let Some(project) = project_filter {
                        keys.retain(|key| key.project == project);
                    }
                    if !include_revoked {
                        keys.retain(|key| key.revoked_at.is_none());
                    }

                    print_key_table(&keys);
                    Ok::<(), anyhow::Error>(())
                })?
            }
            KeyAction::Revoke { key_id } => rt.block_on(async {
                let response = client
                    .revoke_key(&key_id)
                    .await
                    .with_context(|| format!("Failed to revoke API key {}", key_id))?;

                eprintln!(
                    "Revoked API key {} at {}",
                    response.id,
                    response.revoked_at.to_rfc3339()
                );
                Ok::<(), anyhow::Error>(())
            })?,
            KeyAction::Rotate { key_id } => rt.block_on(async {
                let keys = client
                    .list_keys()
                    .await
                    .context("Failed to list API keys before rotation")?
                    .keys;
                let old_key = keys
                    .into_iter()
                    .find(|key| key.id == key_id)
                    .ok_or_else(|| anyhow::anyhow!("API key {} not found", key_id))?;
                if old_key.revoked_at.is_some() {
                    anyhow::bail!("API key {} is already revoked", key_id);
                }

                let request = CreateKeyRequest {
                    description: old_key.description.clone(),
                    role: old_key.role,
                    project: old_key.project.clone(),
                    pattern: old_key.pattern.clone(),
                    expires_at: old_key.expires_at,
                };
                let replacement = client
                    .create_key(&request)
                    .await
                    .with_context(|| format!("Failed to create replacement key for {}", key_id))?;

                if let Err(e) = client.revoke_key(&key_id).await {
                    println!("id\trole\tproject\tkey");
                    println!(
                        "{}\t{}\t{}\t{}",
                        replacement.id, replacement.role, replacement.project, replacement.key
                    );
                    anyhow::bail!(
                        "Created replacement key {} but failed to revoke old key {}: {}",
                        replacement.id,
                        key_id,
                        e
                    );
                }

                eprintln!("Rotated API key {} -> {}", key_id, replacement.id);
                eprintln!("Store this key now; it will not be shown again.");
                println!("old_id\tnew_id\trole\tproject\tkey");
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    key_id, replacement.id, replacement.role, replacement.project, replacement.key
                );
                Ok::<(), anyhow::Error>(())
            })?,
        },
    }

    Ok(())
}

fn print_key_table(keys: &[KeyEntry]) {
    println!("id\trole\tproject\tprefix\tstatus\tcreated_at\texpires_at\tdescription");
    for key in keys {
        let status = if key.revoked_at.is_some() {
            "revoked"
        } else {
            "active"
        };
        let expires_at = key
            .expires_at
            .as_ref()
            .map(|ts| ts.to_rfc3339())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            key.id,
            key.role,
            key.project,
            key.key_prefix,
            status,
            key.created_at.to_rfc3339(),
            expires_at,
            key.description
        );
    }
}

fn build_audit_query(
    args: AuditQueryArgs,
    server_config: &ResolvedServerConfig,
) -> ListAuditEventsQuery {
    ListAuditEventsQuery {
        project: args.project.or_else(|| server_config.project.clone()),
        action: args.action.map(|action| action.as_wire().to_string()),
        resource_type: args
            .resource_type
            .map(|resource_type| resource_type.as_wire().to_string()),
        actor: args.actor,
        limit: args.limit,
        offset: args.offset,
        ..ListAuditEventsQuery::default()
    }
}

fn execute_audit_action(action: AuditActionCli, server_flags: &ServerFlags) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;
    let rt = tokio::runtime::Runtime::new()?;

    match action {
        AuditActionCli::List { query } => {
            let query = build_audit_query(query, &server_config);

            rt.block_on(async {
                let response = client
                    .list_audit_events(&query)
                    .await
                    .context("Failed to list audit events")?;
                print_audit_table(&response);
                Ok::<(), anyhow::Error>(())
            })?
        }
        AuditActionCli::Export { query, format } => {
            let query = build_audit_query(query, &server_config);

            rt.block_on(async {
                let response = client
                    .list_audit_events(&query)
                    .await
                    .context("Failed to export audit events")?;

                match format {
                    AuditExportFormat::Jsonl => {
                        for event in &response.events {
                            println!("{}", serde_json::to_string(event)?);
                        }
                    }
                    AuditExportFormat::Json => {
                        println!("{}", serde_json::to_string_pretty(&response)?);
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?
        }
    }

    Ok(())
}

fn print_audit_table(response: &ListAuditEventsResponse) {
    if response.events.is_empty() {
        println!("No audit events found.");
        return;
    }

    println!(
        "Audit events ({} of {}):",
        response.events.len(),
        response.pagination.total
    );
    println!("id\ttimestamp\taction\tresource\tresource_id\tproject\tactor");
    for event in &response.events {
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            event.id,
            event.timestamp.to_rfc3339(),
            event.action,
            event.resource_type,
            event.resource_id,
            event.project,
            event.actor
        );
    }
}

fn load_validated_baseline_config(config_path: &Path) -> anyhow::Result<ConfigFile> {
    let config = load_config_file(config_path)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    config
        .validate()
        .map_err(|error| anyhow::anyhow!("{} is invalid: {error}", config_path.display()))?;
    Ok(config)
}

fn configured_baseline_benches(
    config: &ConfigFile,
    bench: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if let Some(bench) = bench {
        if config
            .benches
            .iter()
            .any(|candidate| candidate.name == bench)
        {
            return Ok(vec![bench.to_string()]);
        }

        anyhow::bail!("benchmark '{}' is not defined in the config file", bench);
    }

    Ok(config
        .benches
        .iter()
        .map(|bench| bench.name.clone())
        .collect())
}

fn check_run_receipt_candidates(config: &ConfigFile, bench: &str) -> Vec<PathBuf> {
    let out_dir = resolve_configured_out_dir(None, Some(config));
    vec![
        out_dir.join(bench).join(RUN_RECEIPT_FILE),
        out_dir.join(RUN_RECEIPT_FILE),
    ]
}

fn local_baseline_dirs(config: &ConfigFile) -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();

    if config.benches.is_empty() {
        let baseline_dir = config
            .defaults
            .baseline_dir
            .as_deref()
            .unwrap_or(DEFAULT_FALLBACK_BASELINE_DIR);
        if !is_remote_storage_uri(baseline_dir) {
            dirs.insert(PathBuf::from(baseline_dir));
        }
    } else {
        for bench in &config.benches {
            let path = resolve_baseline_path(&None, &bench.name, config);
            let path_text = path.to_string_lossy();
            if is_remote_storage_uri(&path_text) {
                continue;
            }
            if let Some(parent) = path.parent()
                && !parent.as_os_str().is_empty()
            {
                dirs.insert(parent.to_path_buf());
            }
        }
    }

    dirs.into_iter().collect()
}

fn execute_local_baseline_status(config_path: &Path, bench: Option<&str>) -> anyhow::Result<()> {
    let config = load_validated_baseline_config(config_path)?;
    let benches = configured_baseline_benches(&config, bench)?;

    println!("Baseline status ({})", config_path.display());
    if benches.is_empty() {
        println!("No benchmarks are configured.");
        return Ok(());
    }

    let mut found = 0usize;
    let mut missing = Vec::new();
    let mut remote = 0usize;

    for bench_name in &benches {
        let path = resolve_baseline_path(&None, bench_name, &config);
        let path_text = path.to_string_lossy();
        if is_remote_storage_uri(&path_text) {
            remote += 1;
            println!("  REMOTE  {bench_name} -> {path_text} (not probed)");
        } else if path.exists() {
            found += 1;
            println!("  FOUND   {bench_name} -> {}", path.display());
        } else {
            missing.push(bench_name.clone());
            println!("  MISSING {bench_name} -> {}", path.display());
        }
    }

    let local = found + missing.len();
    println!();
    println!(
        "Summary: {found}/{local} local baseline{} found",
        plural(local)
    );
    if remote > 0 {
        println!(
            "Remote baseline{} configured but not probed: {remote}",
            plural(remote)
        );
    }

    if !missing.is_empty() {
        println!();
        println!("Next:");
        if missing.len() == 1 {
            println!(
                "  1. Run: perfgate check --config {} --all",
                config_path.display()
            );
            if bench.is_some() {
                let bench_name = &missing[0];
                println!(
                    "  2. Promote: perfgate baseline promote --config {} --bench {}",
                    config_path.display(),
                    bench_name
                );
            } else {
                println!(
                    "  2. Promote: perfgate baseline promote --config {} --all",
                    config_path.display()
                );
            }
        } else {
            println!(
                "  1. Run: perfgate check --config {} --all",
                config_path.display()
            );
            println!(
                "  2. Promote missing baselines: perfgate baseline promote --config {} --all",
                config_path.display()
            );
        }
    }

    Ok(())
}

fn execute_local_baseline_init(config_path: &Path) -> anyhow::Result<()> {
    let config = load_validated_baseline_config(config_path)?;
    let dirs = local_baseline_dirs(&config);

    if dirs.is_empty() {
        println!("No local baseline directories to create; configured baselines are remote.");
        return Ok(());
    }

    for dir in &dirs {
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let gitkeep = dir.join(".gitkeep");
        if !gitkeep.exists() {
            fs::write(&gitkeep, "").with_context(|| format!("write {}", gitkeep.display()))?;
            println!("Wrote {}", gitkeep.display());
        } else {
            println!("Exists {}", gitkeep.display());
        }
    }

    println!();
    println!("Next:");
    println!(
        "  1. Run: perfgate check --config {} --all",
        config_path.display()
    );
    println!(
        "  2. Promote: perfgate baseline promote --config {} --all",
        config_path.display()
    );

    Ok(())
}

#[derive(Debug)]
struct LocalBaselinePromoteOptions {
    current: Option<PathBuf>,
    to: Option<PathBuf>,
    normalize: bool,
    force: bool,
    pretty: bool,
}

fn execute_local_baseline_promote(
    config_path: &Path,
    bench: Option<&str>,
    all: bool,
    options: LocalBaselinePromoteOptions,
) -> anyhow::Result<()> {
    let config = load_validated_baseline_config(config_path)?;
    if all {
        let benches = configured_baseline_benches(&config, None)?;
        if benches.is_empty() {
            anyhow::bail!("no benchmarks are configured in {}", config_path.display());
        }

        let mut promoted = 0usize;
        for bench in &benches {
            promote_one_local_baseline(config_path, &config, bench, None, None, &options)?;
            promoted += 1;
        }

        eprintln!();
        eprintln!(
            "Promoted {promoted} baseline{} from {}",
            plural(promoted),
            config_path.display()
        );
        return Ok(());
    }

    let bench = bench.ok_or_else(|| anyhow::anyhow!("--bench or --all is required"))?;
    configured_baseline_benches(&config, Some(bench))?;

    promote_one_local_baseline(
        config_path,
        &config,
        bench,
        options.current.clone(),
        options.to.clone(),
        &options,
    )
}

fn promote_one_local_baseline(
    config_path: &Path,
    config: &ConfigFile,
    bench: &str,
    current: Option<PathBuf>,
    to: Option<PathBuf>,
    options: &LocalBaselinePromoteOptions,
) -> anyhow::Result<()> {
    let current_path = if let Some(current) = current {
        current
    } else {
        let candidates = check_run_receipt_candidates(config, bench);
        let mut found = None;
        for candidate in &candidates {
            if location_exists(candidate)? {
                found = Some(candidate.clone());
                break;
            }
        }

        match found {
            Some(path) => path,
            None => {
                let searched = candidates
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" or ");
                anyhow::bail!(
                    "run receipt not found at {searched}; run `perfgate check --config {} --all` first, or pass --current",
                    config_path.display()
                );
            }
        }
    };

    let receipt: RunReceipt = read_json_from_location(&current_path)
        .with_context(|| format!("failed to read run receipt from {}", current_path.display()))?;
    if receipt.bench.name != bench {
        anyhow::bail!(
            "run receipt benchmark '{}' does not match --bench '{}'",
            receipt.bench.name,
            bench
        );
    }

    let baseline_path = to.unwrap_or_else(|| resolve_baseline_path(&None, bench, config));
    if !options.force && location_exists(&baseline_path)? {
        anyhow::bail!(
            "baseline already exists at {}; pass --force to replace it",
            baseline_path.display()
        );
    }

    let result = PromoteUseCase::execute(PromoteRequest {
        receipt,
        normalize: options.normalize,
    });
    write_json_to_location(&baseline_path, &result.receipt, options.pretty)?;

    eprintln!("Promoted baseline for {bench}");
    eprintln!("  current: {}", current_path.display());
    eprintln!("  baseline: {}", baseline_path.display());

    Ok(())
}

/// Execute baseline management actions.
fn execute_baseline_action(
    action: BaselineAction,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    match action {
        BaselineAction::Status { config, bench } => {
            execute_local_baseline_status(&config, bench.as_deref())
        }
        BaselineAction::Init { config } => execute_local_baseline_init(&config),
        BaselineAction::Promote {
            config,
            bench,
            all,
            current,
            to,
            normalize,
            force,
            pretty,
        } => execute_local_baseline_promote(
            &config,
            bench.as_deref(),
            all,
            LocalBaselinePromoteOptions {
                current,
                to,
                normalize,
                force,
                pretty,
            },
        ),
        remote_action => execute_remote_baseline_action(remote_action, server_flags),
    }
}

fn execute_remote_baseline_action(
    action: BaselineAction,
    server_flags: &ServerFlags,
) -> anyhow::Result<()> {
    let (server_config, _config_file) = resolve_server_config_from_path(server_flags, None)?;
    let client = server_config.require_fallback_client(None, BASELINE_SERVER_NOT_CONFIGURED)?;

    let rt = tokio::runtime::Runtime::new()?;

    match action {
        BaselineAction::Status { .. }
        | BaselineAction::Init { .. }
        | BaselineAction::Promote { .. } => {
            unreachable!("local baseline actions are handled before server dispatch");
        }
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

            rt.block_on(async {
                let version_str = match version.as_deref() {
                    Some(version) => version.to_string(),
                    None => {
                        client
                            .get_latest_baseline(&project, &benchmark)
                            .await
                            .with_context(|| {
                                format!(
                                    "Failed to resolve latest baseline version for {}",
                                    benchmark
                                )
                            })?
                            .version
                    }
                };

                client
                    .delete_baseline(&project, &benchmark, &version_str)
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
                        let cv_suffix = record
                            .wall_ms_cv
                            .map(|cv| format!(", wall_cv={:.2}", cv))
                            .unwrap_or_default();
                        let score_suffix = record
                            .flakiness_score
                            .map(|score| format!(", flakiness={:.2}", score))
                            .unwrap_or_default();
                        println!(
                            "  [{:?}] {} - {} ({}){}{}",
                            record.status,
                            record.benchmark,
                            record.created_at,
                            git_ref,
                            cv_suffix,
                            score_suffix
                        );
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?;
        }

        BaselineAction::Flaky {
            benchmark,
            project,
            limit,
            min_score,
        } => {
            let project = server_config.resolve_project(project)?;
            let mut query = ListVerdictsQuery::new().with_limit(limit);
            if let Some(bench) = benchmark {
                query = query.with_benchmark(bench);
            }

            rt.block_on(async {
                let response = client
                    .list_verdicts(&project, &query)
                    .await
                    .with_context(|| {
                        format!("Failed to get flakiness history for project {}", project)
                    })?;

                let mut latest_by_benchmark = std::collections::BTreeMap::new();
                for record in &response.verdicts {
                    latest_by_benchmark
                        .entry(record.benchmark.clone())
                        .or_insert(record);
                }

                let mut flaky: Vec<_> = latest_by_benchmark
                    .into_values()
                    .filter(|record| record.flakiness_score.unwrap_or(0.0) >= min_score)
                    .collect();
                flaky.sort_by(|left, right| {
                    right
                        .flakiness_score
                        .unwrap_or(0.0)
                        .total_cmp(&left.flakiness_score.unwrap_or(0.0))
                });

                if flaky.is_empty() {
                    println!(
                        "No flaky benchmarks found for project '{}' at score >= {:.2}.",
                        project, min_score
                    );
                } else {
                    println!(
                        "Flaky benchmarks for {} (score >= {:.2}):",
                        project, min_score
                    );
                    for record in flaky {
                        let cv = record
                            .wall_ms_cv
                            .map(|value| format!("{:.2}", value))
                            .unwrap_or_else(|| "n/a".to_string());
                        println!(
                            "  {:.2}  {}  latest_cv={}  {:?}  {}",
                            record.flakiness_score.unwrap_or(0.0),
                            record.benchmark,
                            cv,
                            record.status,
                            record.created_at
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
                wall_ms_cv: compare_receipt
                    .deltas
                    .get(&perfgate_types::Metric::WallMs)
                    .and_then(|delta| delta.cv),
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
    out_dir: Option<PathBuf>,
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

fn resolve_configured_out_dir(
    cli_out_dir: Option<&PathBuf>,
    config: Option<&ConfigFile>,
) -> PathBuf {
    if let Some(out_dir) = cli_out_dir {
        return out_dir.clone();
    }

    if let Some(out_dir) = config.and_then(|config| config.defaults.out_dir.as_ref()) {
        return PathBuf::from(out_dir);
    }

    PathBuf::from(DEFAULT_ARTIFACT_DIR)
}

/// Returns true if the verdict indicates a regression (warn or fail).
fn is_regression(status: VerdictStatus) -> bool {
    matches!(status, VerdictStatus::Warn | VerdictStatus::Fail)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureClass {
    SetupMissingConfig,
    SetupMissingBench,
    SetupCommandFailed,
    MissingBaseline,
    PerformanceRegression,
    HighNoise,
    UnsupportedMetric,
    HostMismatch,
    ReviewRequired,
    ServerUploadFailed,
}

impl FailureClass {
    fn status(self) -> &'static str {
        match self {
            Self::SetupMissingConfig => "setup_missing_config",
            Self::SetupMissingBench => "setup_missing_bench",
            Self::SetupCommandFailed => "setup_command_failed",
            Self::MissingBaseline => "missing_baseline",
            Self::PerformanceRegression => "performance_regression",
            Self::HighNoise => "high_noise",
            Self::UnsupportedMetric => "unsupported_metric",
            Self::HostMismatch => "host_mismatch",
            Self::ReviewRequired => "review_required",
            Self::ServerUploadFailed => "server_upload_failed",
        }
    }

    fn meaning(self) -> &'static str {
        match self {
            Self::SetupMissingConfig => {
                "perfgate could not read the config, so no performance decision was made."
            }
            Self::SetupMissingBench => {
                "The requested benchmark is missing or no runnable benchmarks are configured yet."
            }
            Self::SetupCommandFailed => {
                "The benchmark command failed before perfgate could make a performance decision."
            }
            Self::MissingBaseline => "Setup is incomplete; this is not a performance regression.",
            Self::PerformanceRegression => {
                "A configured benchmark exceeded its performance budget or warning threshold."
            }
            Self::HighNoise => {
                "The run is noisy enough that the result may need paired mode or calibration."
            }
            Self::UnsupportedMetric => "A requested metric is not available on this platform.",
            Self::HostMismatch => {
                "The baseline and current run were captured on different host fingerprints."
            }
            Self::ReviewRequired => {
                "The evidence needs human review before this result should be accepted."
            }
            Self::ServerUploadFailed => {
                "The optional server ledger upload failed; local receipts still record the result."
            }
        }
    }

    fn do_not(self) -> &'static str {
        match self {
            Self::SetupMissingConfig => "do not copy another repo's baselines to bypass setup",
            Self::SetupMissingBench => {
                "do not promote a baseline until the benchmark command is reviewed"
            }
            Self::SetupCommandFailed => {
                "do not loosen thresholds to fix a command that does not run"
            }
            Self::MissingBaseline => "do not loosen thresholds to fix a missing baseline",
            Self::PerformanceRegression => {
                "do not promote the current run as a baseline until the regression is understood"
            }
            Self::HighNoise => "do not treat noisy single-run evidence as release proof",
            Self::UnsupportedMetric => {
                "do not assume a missing platform metric invalidates every gate"
            }
            Self::HostMismatch => {
                "do not accept host-mismatched evidence without checking whether the hosts are comparable"
            }
            Self::ReviewRequired => "do not bypass required review by changing local thresholds",
            Self::ServerUploadFailed => {
                "do not rerun the benchmark just to repair an optional ledger upload"
            }
        }
    }

    fn artifacts(self, out_dir: Option<&Path>, compare_path: Option<&Path>) -> Vec<String> {
        let Some(out_dir) = out_dir else {
            return vec![
                "artifacts unavailable because setup failed before receipts were written"
                    .to_string(),
            ];
        };

        let mut artifacts = match self {
            Self::MissingBaseline => vec![
                out_dir.join(RUN_RECEIPT_FILE).display().to_string(),
                out_dir.join(REPORT_RECEIPT_FILE).display().to_string(),
                out_dir.join(COMMENT_MARKDOWN_FILE).display().to_string(),
                out_dir.join("repair_context.json").display().to_string(),
            ],
            Self::PerformanceRegression
            | Self::HighNoise
            | Self::HostMismatch
            | Self::ReviewRequired => vec![
                out_dir.join(RUN_RECEIPT_FILE).display().to_string(),
                compare_path
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| out_dir.join(COMPARE_RECEIPT_FILE).display().to_string()),
                out_dir.join(REPORT_RECEIPT_FILE).display().to_string(),
                out_dir.join(COMMENT_MARKDOWN_FILE).display().to_string(),
                out_dir.join("repair_context.json").display().to_string(),
            ],
            Self::ServerUploadFailed => vec![
                out_dir.join(RUN_RECEIPT_FILE).display().to_string(),
                out_dir.join(REPORT_RECEIPT_FILE).display().to_string(),
            ],
            Self::SetupMissingConfig
            | Self::SetupMissingBench
            | Self::SetupCommandFailed
            | Self::UnsupportedMetric => {
                vec!["artifacts unavailable or incomplete because setup failed".to_string()]
            }
        };
        artifacts.sort();
        artifacts.dedup();
        artifacts
    }

    fn next_commands(
        self,
        config_path: &Path,
        bench_name: Option<&str>,
        compare_path: Option<&Path>,
    ) -> Vec<String> {
        let config = shell_path(config_path);
        let check = check_command(config_path, bench_name, false);
        let check_required = check_command(config_path, bench_name, true);
        match self {
            Self::SetupMissingConfig => vec![
                "perfgate init --ci github --profile standard --suggest-benches".to_string(),
                format!("perfgate doctor --config {config}"),
            ],
            Self::SetupMissingBench => vec![
                format!("edit {config} and add a reviewed [[bench]] command"),
                format!("perfgate doctor --config {config}"),
            ],
            Self::SetupCommandFailed => vec![check],
            Self::MissingBaseline => vec![check, baseline_promote_command(config_path, bench_name)],
            Self::PerformanceRegression => {
                let mut commands = vec![check_required];
                if let Some(compare_path) = compare_path {
                    commands.push(format!(
                        "perfgate explain --compare {}",
                        shell_path(compare_path)
                    ));
                }
                commands
            }
            Self::HighNoise => vec![
                paired_command(bench_name),
                format!("review noise guidance before tightening {config}"),
            ],
            Self::UnsupportedMetric => vec![
                format!("review platform metric support before changing {config}"),
                check,
            ],
            Self::HostMismatch => vec![
                check_required,
                "rerun on the same runner class as the baseline".to_string(),
            ],
            Self::ReviewRequired => vec![
                "review artifacts/perfgate/decision.md or the Action summary".to_string(),
                "perfgate decision bundle --index artifacts/perfgate/decision.index.json"
                    .to_string(),
            ],
            Self::ServerUploadFailed => vec![
                "inspect server URL/API key/project settings".to_string(),
                "perfgate decision history".to_string(),
            ],
        }
    }
}

const REPORT_RECEIPT_FILE: &str = "report.json";
const COMMENT_MARKDOWN_FILE: &str = "comment.md";

fn shell_path(path: &Path) -> String {
    let value = path.display().to_string();
    if value.contains(' ') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value
    }
}

fn check_command(config_path: &Path, bench_name: Option<&str>, require_baseline: bool) -> String {
    let mut command = if let Some(bench_name) = bench_name {
        format!(
            "perfgate check --config {} --bench {}",
            shell_path(config_path),
            bench_name
        )
    } else {
        format!("perfgate check --config {} --all", shell_path(config_path))
    };
    if require_baseline {
        command.push_str(" --require-baseline");
    }
    command
}

fn baseline_promote_command(config_path: &Path, bench_name: Option<&str>) -> String {
    if let Some(bench_name) = bench_name {
        format!(
            "perfgate baseline promote --config {} --bench {}",
            shell_path(config_path),
            bench_name
        )
    } else {
        format!(
            "perfgate baseline promote --config {} --all",
            shell_path(config_path)
        )
    }
}

fn paired_command(bench_name: Option<&str>) -> String {
    let name = bench_name.unwrap_or("<bench>");
    format!(
        "perfgate paired --name {name} --baseline-cmd \"<baseline-cmd>\" --current-cmd \"<current-cmd>\" --repeat 10 --out artifacts/perfgate/{name}/paired.json"
    )
}

fn print_check_failure_guidance(
    class: FailureClass,
    config_path: &Path,
    bench_name: Option<&str>,
    out_dir: Option<&Path>,
    compare_path: Option<&Path>,
) {
    eprintln!();
    eprintln!("Status: {}", class.status());
    eprintln!("Meaning: {}", class.meaning());
    eprintln!("Artifacts:");
    for artifact in class.artifacts(out_dir, compare_path) {
        eprintln!("  {artifact}");
    }
    eprintln!("Next:");
    for command in class.next_commands(config_path, bench_name, compare_path) {
        eprintln!("  {command}");
    }
    eprintln!("Do not:");
    eprintln!("  {}", class.do_not());
}

fn classify_check_error(error: &anyhow::Error) -> FailureClass {
    if let Some(err) = error.downcast_ref::<PerfgateError>() {
        return match err {
            PerfgateError::Config(ConfigValidationError::BenchName(_)) => {
                FailureClass::SetupMissingBench
            }
            PerfgateError::Io(IoError::BaselineNotFound { .. }) => FailureClass::MissingBaseline,
            PerfgateError::Io(IoError::RunCommand { .. })
            | PerfgateError::Adapter(AdapterError::RunCommand { .. })
            | PerfgateError::Adapter(AdapterError::EmptyArgv)
            | PerfgateError::Adapter(AdapterError::Timeout) => FailureClass::SetupCommandFailed,
            PerfgateError::Adapter(AdapterError::TimeoutUnsupported) => {
                FailureClass::UnsupportedMetric
            }
            _ => FailureClass::SetupCommandFailed,
        };
    }

    let message = error.to_string().to_ascii_lowercase();
    if message.contains("no benchmarks")
        || message.contains("not found in config")
        || message.contains("either --bench or --all")
    {
        FailureClass::SetupMissingBench
    } else if message.contains("baseline") {
        FailureClass::MissingBaseline
    } else if message.contains("host mismatch") {
        FailureClass::HostMismatch
    } else if message.contains("not found") || message.contains("read ") {
        FailureClass::SetupMissingConfig
    } else {
        FailureClass::SetupCommandFailed
    }
}

fn emit_check_outcome_guidance(
    req: &CheckConfig,
    bench_name: &str,
    bench_out_dir: &Path,
    outcome: &CheckOutcome,
) {
    if outcome.compare_receipt.is_none() {
        print_check_failure_guidance(
            FailureClass::MissingBaseline,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            None,
        );
    }

    if let Some(compare) = &outcome.compare_receipt
        && is_regression(compare.verdict.status)
    {
        print_check_failure_guidance(
            FailureClass::PerformanceRegression,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }

    if outcome.suggest_paired
        || outcome
            .warnings
            .iter()
            .any(|warning| warning.contains("high noise"))
    {
        print_check_failure_guidance(
            FailureClass::HighNoise,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }

    if outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("host mismatch"))
    {
        print_check_failure_guidance(
            FailureClass::HostMismatch,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }

    if outcome
        .report
        .verdict
        .reasons
        .iter()
        .any(|reason| reason.contains("review_required") || reason.contains("review required"))
    {
        print_check_failure_guidance(
            FailureClass::ReviewRequired,
            &req.config_path,
            Some(bench_name),
            Some(bench_out_dir),
            outcome.compare_path.as_deref(),
        );
    }
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
            wall_ms_cv: compare_receipt
                .deltas
                .get(&perfgate_types::Metric::WallMs)
                .and_then(|delta| delta.cv),
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
        .inspect_err(|_| {
            print_check_failure_guidance(
                FailureClass::SetupMissingConfig,
                &req.config_path,
                req.bench.as_deref(),
                None,
                None,
            );
        })
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
    )
    .inspect_err(|error| {
        print_check_failure_guidance(
            classify_check_error(error),
            &req.config_path,
            req.bench.as_deref(),
            None,
            None,
        );
    })?;
    let bench_count = bench_names.len() as u32;

    let markdown_template_path = req.md_template.clone().or_else(|| {
        config_file
            .defaults
            .markdown_template
            .as_ref()
            .map(PathBuf::from)
    });
    let _markdown_template = load_template(markdown_template_path.as_deref())?;
    let github_output_path = resolve_github_output_path(req.output_github)?;
    let out_dir = resolve_configured_out_dir(req.out_dir.as_ref(), Some(&config_file));

    // Track aggregate exit code: fail (2) > warn-as-fail (3) > pass (0)
    let mut max_exit_code: i32 = 0;
    let mut all_warnings: Vec<String> = Vec::new();
    let mut total_pass: u32 = 0;
    let mut total_warn: u32 = 0;
    let mut total_fail: u32 = 0;

    for bench_name in &bench_names {
        // For --all mode, use per-bench subdirectories
        let bench_out_dir = if req.all {
            out_dir.join(bench_name)
        } else {
            out_dir.clone()
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

        let outcome = match usecase.execute(CheckRequest {
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
        }) {
            Ok(outcome) => outcome,
            Err(error) => {
                print_check_failure_guidance(
                    classify_check_error(&error),
                    &req.config_path,
                    Some(bench_name),
                    Some(&bench_out_dir),
                    None,
                );
                return Err(error);
            }
        };

        // Submit verdict to server if configured
        if let Some(compare) = &outcome.compare_receipt {
            submit_verdict_if_possible(&req.server_flags, &config_file, compare);
        }

        // Upload to local server if --local-db is set
        if req.local_db
            && let Err(e) = upload_to_local_db(bench_name, &outcome.run_receipt)
        {
            eprintln!("warning: local-db upload failed: {:#}", e);
            print_check_failure_guidance(
                FailureClass::ServerUploadFailed,
                &req.config_path,
                Some(bench_name),
                Some(&bench_out_dir),
                outcome.compare_path.as_deref(),
            );
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

        emit_check_outcome_guidance(&req, bench_name, &bench_out_dir, &outcome);

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
    let fallback_out_dir = resolve_configured_out_dir(req.out_dir.as_ref(), None);

    // Ensure base output directory exists (catastrophic failure if we can't)
    fs::create_dir_all(&fallback_out_dir)
        .with_context(|| format!("create output dir {}", fallback_out_dir.display()))?;

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
            let report_path = fallback_out_dir.join("report.json");
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
    let out_dir = resolve_configured_out_dir(req.out_dir.as_ref(), Some(&config_file));

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
    let report_path = out_dir.join("report.json");
    write_json(&report_path, &sensor_report, req.pretty)?;

    // Write combined markdown to out_dir root
    let md_dest = out_dir.join("comment.md");
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

fn parse_flakiness_score(s: &str) -> Result<f64, String> {
    let score: f64 = s
        .parse()
        .map_err(|_| "flakiness score must be a number".to_string())?;
    if !score.is_finite() || !(0.0..=1.0).contains(&score) {
        return Err("flakiness score must be between 0.0 and 1.0".to_string());
    }
    Ok(score)
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

fn parse_metric_status(s: &str) -> Result<MetricStatus, String> {
    match s.to_lowercase().as_str() {
        "pass" => Ok(MetricStatus::Pass),
        "warn" => Ok(MetricStatus::Warn),
        "fail" => Ok(MetricStatus::Fail),
        "skip" => Ok(MetricStatus::Skip),
        _ => Err(format!(
            "invalid metric status: {s} (expected pass|warn|fail|skip)"
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

fn parse_aggregate_weight_mode(s: &str) -> Result<AggregateWeightMode, String> {
    match s {
        "configured" => Ok(AggregateWeightMode::Configured),
        "inverse_variance" => Ok(AggregateWeightMode::InverseVariance),
        _ => Err(format!(
            "invalid aggregate weight mode: {s} (expected configured|inverse_variance)"
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
    weight_mode: AggregateWeightMode,
    quorum: Option<f64>,
    fail_n: Option<u32>,
    fail_m: Option<u32>,
    variance_floor: Option<f64>,
) -> anyhow::Result<(Option<f64>, Option<FailIfNOfM>, Option<f64>)> {
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

    if matches!(weight_mode, AggregateWeightMode::InverseVariance)
        && !matches!(policy, AggregationPolicy::Weighted)
    {
        anyhow::bail!("--weight-mode inverse_variance requires --policy weighted");
    }

    if let Some(variance_floor) = variance_floor {
        if !variance_floor.is_finite() || variance_floor <= 0.0 {
            anyhow::bail!(
                "--variance-floor must be a positive finite number, got {variance_floor}"
            );
        }
        if !matches!(weight_mode, AggregateWeightMode::InverseVariance) {
            anyhow::bail!("--variance-floor requires --weight-mode inverse_variance");
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
            Ok((quorum, Some(FailIfNOfM { n, m: fail_m }), variance_floor))
        }
        _ => {
            if fail_n.is_some() || fail_m.is_some() {
                anyhow::bail!("--fail-n and --fail-m require --policy fail_if_n_of_m");
            }
            Ok((quorum, None, variance_floor))
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
    fn parse_retention_duration_accepts_days_and_weeks() {
        assert_eq!(
            parse_retention_duration("365d").unwrap(),
            Duration::from_secs(365 * 86_400)
        );
        assert_eq!(
            parse_retention_duration("12w").unwrap(),
            Duration::from_secs(12 * 7 * 86_400)
        );
    }

    #[test]
    fn parse_retention_duration_accepts_humantime() {
        assert_eq!(
            parse_retention_duration("2h").unwrap(),
            Duration::from_secs(2 * 60 * 60)
        );
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
    fn parse_aggregate_weight_mode_accepts_inverse_variance() {
        let mode = parse_aggregate_weight_mode("inverse_variance").unwrap();
        assert_eq!(mode, AggregateWeightMode::InverseVariance);
    }

    #[test]
    fn validate_aggregate_options_rejects_inverse_variance_without_weighted_policy() {
        let err = validate_aggregate_options(
            AggregationPolicy::Majority,
            AggregateWeightMode::InverseVariance,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("requires --policy weighted"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn normalize_paired_cli_command_splits_single_shell_string() -> anyhow::Result<()> {
        let parsed =
            normalize_paired_cli_command(vec!["cmd /c exit 0".to_string()], "--baseline-cmd")?;
        assert_eq!(parsed, vec!["cmd", "/c", "exit", "0"]);
        Ok(())
    }

    #[test]
    fn normalize_paired_cli_command_keeps_argv_tokens() -> anyhow::Result<()> {
        let args = vec![
            "cmd".to_string(),
            "/c".to_string(),
            "exit".to_string(),
            "0".to_string(),
        ];
        let parsed = normalize_paired_cli_command(args.clone(), "--baseline-cmd")?;
        assert_eq!(parsed, args);
        Ok(())
    }

    #[test]
    fn normalize_paired_cli_command_keeps_single_token() -> anyhow::Result<()> {
        let args = vec!["true".to_string()];
        let parsed = normalize_paired_cli_command(args.clone(), "--baseline-cmd")?;
        assert_eq!(parsed, args);
        Ok(())
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
        assert!(
            msg.contains("parse JSON") || msg.contains("parse json"),
            "unexpected error: {}",
            msg
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
