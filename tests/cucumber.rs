//! BDD test runner using cucumber for perfgate CLI.
//!
//! This module sets up the cucumber test framework to execute Gherkin feature files
//! located in the `features/` directory.
//!
//! Step definitions cover:
//! - Given steps: fixture creation (baseline/current receipts)
//! - When steps: CLI command execution
//! - Then steps: exit code and output assertions

use assert_cmd::Command;
use cucumber::{World, given, then, when};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Re-export types we need for fixture creation
use perfgate_types::{
    BaselineServerConfig, BenchConfigFile, BenchMeta, BudgetOverride, COMPARE_SCHEMA_V1,
    CompareReceipt, CompareRef, ConfigFile, DefaultsConfig, Delta, HostInfo, Metric,
    MetricStatistic, MetricStatus, PAIRED_SCHEMA_V1, PairedRunReceipt, PerfgateReport,
    REPORT_SCHEMA_V1, RUN_SCHEMA_V1, ReportSummary, RunMeta, RunReceipt, Sample, SensorReport,
    Stats, ToolInfo, U64Summary, Verdict, VerdictCounts, VerdictStatus,
};

// Microcrate imports for direct testing
use perfgate_budget::{aggregate_verdict, evaluate_budget, reason_token as budget_reason_token};
use perfgate_error::{
    AdapterError, ConfigValidationError, IoError, PairedError, PerfgateError, StatsError,
    ValidationError,
};
use perfgate_export::{ExportFormat, ExportUseCase};
use perfgate_host_detect::detect_host_mismatch;
use perfgate_render::render_markdown;
use perfgate_sensor::SensorReportBuilder;
use perfgate_sha256::sha256_hex;
use perfgate_significance::compute_significance;
use perfgate_stats::summarize_u64;
use perfgate_validation::validate_bench_name as validate_bench_name_fn;

/// World struct that holds state across BDD scenario steps.
#[derive(Debug, Default, World)]
pub struct PerfgateWorld {
    /// Temporary directory for test artifacts
    temp_dir: Option<TempDir>,
    /// Path to baseline receipt file
    baseline_path: Option<PathBuf>,
    /// Path to current receipt file
    current_path: Option<PathBuf>,
    /// Path to compare receipt file
    compare_path: Option<PathBuf>,
    /// Path to output file
    output_path: Option<PathBuf>,
    /// Path to export file
    export_path: Option<PathBuf>,
    /// Path to second export file (for comparison)
    export_path2: Option<PathBuf>,
    /// Path to promoted baseline file
    promoted_baseline_path: Option<PathBuf>,
    /// Path to source run receipt for promote
    source_run_path: Option<PathBuf>,
    /// Custom run_id for the source receipt
    source_run_id: Option<String>,
    /// Custom started_at for the source receipt
    source_started_at: Option<String>,
    /// Custom bench name for the source receipt
    source_bench_name: Option<String>,
    /// Exit code from last command execution
    last_exit_code: Option<i32>,
    /// Stdout from last command execution
    last_stdout: String,
    /// Stderr from last command execution
    last_stderr: String,
    /// Additional CLI arguments to pass
    extra_args: Vec<String>,
    /// Baseline wall_ms median value
    baseline_wall_ms: Option<u64>,
    /// Current wall_ms median value
    current_wall_ms: Option<u64>,
    /// Path to report output file
    report_path: Option<PathBuf>,
    /// Path to second report file (for determinism comparison)
    report_path2: Option<PathBuf>,
    /// Path to markdown output file (for report command)
    md_output_path: Option<PathBuf>,
    /// Path to GitHub outputs file (for check --output-github)
    github_output_path: Option<PathBuf>,
    /// Path to config file (for check command)
    config_path: Option<PathBuf>,
    /// Path to artifacts directory (for check command)
    artifacts_dir: Option<PathBuf>,
    /// Config file being built
    config: Option<ConfigFile>,
    /// Path to markdown template file (for md --template)
    template_path: Option<PathBuf>,
    /// Microcrate test state: computed hash
    computed_hash: Option<String>,
    /// Microcrate test state: list of values for stats
    stats_values: Vec<u64>,
    /// Microcrate test state: computed median
    computed_median: Option<u64>,
    /// Microcrate test state: validation result
    validation_result: Option<Result<(), ValidationError>>,
    /// Microcrate test state: baseline host info
    baseline_host: Option<Box<HostInfo>>,
    /// Microcrate test state: current host info
    current_host: Option<Box<HostInfo>>,
    /// Microcrate test state: host mismatch result
    host_mismatch: Option<Box<perfgate_types::HostMismatchInfo>>,
    /// Microcrate test state: test run receipt for export
    test_run_receipt: Option<Box<RunReceipt>>,
    /// Microcrate test state: test compare receipt for render
    test_compare_receipt: Option<Box<CompareReceipt>>,
    /// Microcrate test state: test perfgate report for sensor
    test_perfgate_report: Option<Box<PerfgateReport>>,
    /// Microcrate test state: exported output
    exported_output: Option<String>,
    /// Microcrate test state: rendered markdown
    rendered_markdown: Option<String>,
    /// Microcrate test state: sensor report
    sensor_report: Option<Box<SensorReport>>,
    /// Microcrate test state: perfgate error
    perfgate_error: Option<perfgate_error::PerfgateError>,
    /// Microcrate test state: budget configuration
    test_budget: Option<perfgate_types::Budget>,
    /// Microcrate test state: budget result
    budget_result: Option<perfgate_budget::BudgetResult>,
    /// Microcrate test state: budget error
    budget_error: Option<String>,
    /// Microcrate test state: budget statuses for aggregation
    budget_statuses: Vec<perfgate_types::MetricStatus>,
    /// Microcrate test state: aggregated verdict
    aggregated_verdict: Option<perfgate_types::Verdict>,
    /// Microcrate test state: reason token
    reason_token: Option<String>,
    /// Microcrate test state: significance baseline samples
    significance_baseline: Vec<f64>,
    /// Microcrate test state: significance current samples
    significance_current: Vec<f64>,
    /// Microcrate test state: significance result
    significance_result: Option<perfgate_types::Significance>,
    /// Mock server for baseline service tests
    server: Option<wiremock::MockServer>,
    /// Role for auth tests
    current_role: Option<perfgate_auth::Role>,
    /// Generated API key
    last_api_key: Option<String>,
}

impl PerfgateWorld {
    /// Get or create the temporary directory for this scenario
    pub fn ensure_temp_dir(&mut self) {
        if self.temp_dir.is_none() {
            self.temp_dir = Some(TempDir::new().expect("Failed to create temp directory"));
        }
    }

    /// Get the path to the temporary directory
    pub fn temp_path(&self) -> PathBuf {
        self.temp_dir
            .as_ref()
            .expect("Temp dir not initialized")
            .path()
            .to_path_buf()
    }

    /// Create a minimal valid RunReceipt with specified wall_ms median
    pub fn create_run_receipt(&self, wall_ms_median: u64) -> RunReceipt {
        RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: format!("test-run-{}", wall_ms_median),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:01:00Z".to_string(),
                host: HostInfo {
                    os: std::env::consts::OS.to_string(),
                    arch: std::env::consts::ARCH.to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![Sample {
                wall_ms: wall_ms_median,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary::new(
                    wall_ms_median,
                    wall_ms_median.saturating_sub(10),
                    wall_ms_median.saturating_add(10),
                ),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    /// Create a RunReceipt from explicit wall_ms sample values
    pub fn create_run_receipt_from_samples(&self, wall_ms_values: &[u64]) -> RunReceipt {
        let mut sorted = wall_ms_values.to_vec();
        sorted.sort_unstable();
        let median = sorted[sorted.len() / 2];
        let min = *sorted.first().unwrap();
        let max = *sorted.last().unwrap();

        let samples = wall_ms_values
            .iter()
            .map(|&v| Sample {
                wall_ms: v,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            })
            .collect();

        RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: format!("test-run-{}", median),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:01:00Z".to_string(),
                host: HostInfo {
                    os: std::env::consts::OS.to_string(),
                    arch: std::env::consts::ARCH.to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: wall_ms_values.len() as u32,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples,
            stats: Stats {
                wall_ms: U64Summary::new(median, min, max),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    /// Create a minimal valid CompareReceipt with specified verdict
    pub fn create_compare_receipt(&self, verdict_status: VerdictStatus) -> CompareReceipt {
        let baseline_wall_ms = self.baseline_wall_ms.unwrap_or(1000);
        let current_wall_ms = self.current_wall_ms.unwrap_or(1000);

        let ratio = current_wall_ms as f64 / baseline_wall_ms as f64;
        let pct = (current_wall_ms as f64 - baseline_wall_ms as f64) / baseline_wall_ms as f64;
        let regression = if pct > 0.0 { pct } else { 0.0 };

        let metric_status = match verdict_status {
            VerdictStatus::Pass => MetricStatus::Pass,
            VerdictStatus::Warn => MetricStatus::Warn,
            VerdictStatus::Fail => MetricStatus::Fail,
            VerdictStatus::Skip => MetricStatus::Skip,
        };

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: baseline_wall_ms as f64,
                current: current_wall_ms as f64,
                ratio,
                pct,
                regression,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: metric_status,
            },
        );

        let reasons = match verdict_status {
            VerdictStatus::Pass | VerdictStatus::Skip => vec![],
            VerdictStatus::Warn => vec!["wall_ms_warn".to_string()],
            VerdictStatus::Fail => vec!["wall_ms_fail".to_string()],
        };

        let counts = match verdict_status {
            VerdictStatus::Pass => VerdictCounts {
                pass: 1,
                warn: 0,
                fail: 0,
                skip: 0,
            },
            VerdictStatus::Warn => VerdictCounts {
                pass: 0,
                warn: 1,
                fail: 0,
                skip: 0,
            },
            VerdictStatus::Fail => VerdictCounts {
                pass: 0,
                warn: 0,
                fail: 1,
                skip: 0,
            },
            VerdictStatus::Skip => VerdictCounts {
                pass: 0,
                warn: 0,
                fail: 0,
                skip: 1,
            },
        };

        CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baseline.json".to_string()),
                run_id: Some("baseline-run-id".to_string()),
            },
            current_ref: CompareRef {
                path: Some("current.json".to_string()),
                run_id: Some("current-run-id".to_string()),
            },
            budgets: BTreeMap::new(),
            deltas,
            verdict: Verdict {
                status: verdict_status,
                counts,
                reasons,
            },
        }
    }
}

// ============================================================================
// GIVEN STEPS - Fixture Creation
// ============================================================================

/// Initialize a temporary directory for test artifacts
#[given("a temporary directory for test artifacts")]
async fn given_temp_directory(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
}

/// Create a baseline receipt with specified wall_ms median
#[given(expr = "a baseline receipt with wall_ms median of {int}")]
async fn given_baseline_receipt(world: &mut PerfgateWorld, wall_ms: u64) {
    world.ensure_temp_dir();
    world.baseline_wall_ms = Some(wall_ms);
    let receipt = world.create_run_receipt(wall_ms);
    let baseline_path = world.temp_path().join("baseline.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize baseline");
    fs::write(&baseline_path, json).expect("Failed to write baseline receipt");
    world.baseline_path = Some(baseline_path);
}

/// Create a current receipt with specified wall_ms median
#[given(expr = "a current receipt with wall_ms median of {int}")]
async fn given_current_receipt(world: &mut PerfgateWorld, wall_ms: u64) {
    world.ensure_temp_dir();
    world.current_wall_ms = Some(wall_ms);
    let receipt = world.create_run_receipt(wall_ms);
    let current_path = world.temp_path().join("current.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize current");
    fs::write(&current_path, json).expect("Failed to write current receipt");
    world.current_path = Some(current_path);
}

/// Create a current receipt with specified wall_ms median and custom host os
#[given(expr = "a current receipt with wall_ms median of {int} and host os {string}")]
async fn given_current_receipt_with_host_os(
    world: &mut PerfgateWorld,
    wall_ms: u64,
    host_os: String,
) {
    world.ensure_temp_dir();
    world.current_wall_ms = Some(wall_ms);
    let mut receipt = world.create_run_receipt(wall_ms);
    receipt.run.host.os = host_os;
    let current_path = world.temp_path().join("current.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize current");
    fs::write(&current_path, json).expect("Failed to write current receipt");
    world.current_path = Some(current_path);
}

/// Create a current receipt with specified wall_ms median and custom bench name
#[given(expr = "a current receipt with wall_ms median of {int} and bench name {string}")]
async fn given_current_receipt_with_bench_name(
    world: &mut PerfgateWorld,
    wall_ms: u64,
    bench_name: String,
) {
    world.ensure_temp_dir();
    world.current_wall_ms = Some(wall_ms);
    let mut receipt = world.create_run_receipt(wall_ms);
    receipt.bench.name = bench_name;
    let current_path = world.temp_path().join("current.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize current");
    fs::write(&current_path, json).expect("Failed to write current receipt");
    world.current_path = Some(current_path);
}

/// Create a compare receipt with pass verdict
#[given("a compare receipt with pass verdict")]
async fn given_compare_receipt_pass(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    world.baseline_wall_ms = Some(1000);
    world.current_wall_ms = Some(900);
    let receipt = world.create_compare_receipt(VerdictStatus::Pass);
    let compare_path = world.temp_path().join("compare.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize compare");
    fs::write(&compare_path, json).expect("Failed to write compare receipt");
    world.compare_path = Some(compare_path);
}

/// Create a compare receipt with warn verdict
#[given("a compare receipt with warn verdict")]
async fn given_compare_receipt_warn(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    world.baseline_wall_ms = Some(1000);
    world.current_wall_ms = Some(1150);
    let receipt = world.create_compare_receipt(VerdictStatus::Warn);
    let compare_path = world.temp_path().join("compare.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize compare");
    fs::write(&compare_path, json).expect("Failed to write compare receipt");
    world.compare_path = Some(compare_path);
}

/// Create a compare receipt with fail verdict
#[given("a compare receipt with fail verdict")]
async fn given_compare_receipt_fail(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    world.baseline_wall_ms = Some(1000);
    world.current_wall_ms = Some(1500);
    let receipt = world.create_compare_receipt(VerdictStatus::Fail);
    let compare_path = world.temp_path().join("compare.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize compare");
    fs::write(&compare_path, json).expect("Failed to write compare receipt");
    world.compare_path = Some(compare_path);
}

/// Set the --fail-on-warn flag
#[given("the --fail-on-warn flag is set")]
async fn given_fail_on_warn_flag(world: &mut PerfgateWorld) {
    world.extra_args.push("--fail-on-warn".to_string());
}

/// Create a baseline receipt with explicit wall_ms sample values
#[given(expr = "a baseline receipt with wall_ms samples {string}")]
async fn given_baseline_receipt_with_samples(world: &mut PerfgateWorld, samples_str: String) {
    world.ensure_temp_dir();
    let values: Vec<u64> = samples_str
        .split(',')
        .map(|s| s.trim().parse().expect("Failed to parse sample"))
        .collect();
    let mut sorted = values.clone();
    sorted.sort_unstable();
    world.baseline_wall_ms = Some(sorted[sorted.len() / 2]);
    let receipt = world.create_run_receipt_from_samples(&values);
    let baseline_path = world.temp_path().join("baseline.json");
    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize baseline");
    fs::write(&baseline_path, json).expect("Failed to write baseline receipt");
    world.baseline_path = Some(baseline_path);
}

/// Create a current receipt with explicit wall_ms sample values
#[given(expr = "a current receipt with wall_ms samples {string}")]
async fn given_current_receipt_with_samples(world: &mut PerfgateWorld, samples_str: String) {
    world.ensure_temp_dir();
    let values: Vec<u64> = samples_str
        .split(',')
        .map(|s| s.trim().parse().expect("Failed to parse sample"))
        .collect();
    let mut sorted = values.clone();
    sorted.sort_unstable();
    world.current_wall_ms = Some(sorted[sorted.len() / 2]);
    let receipt = world.create_run_receipt_from_samples(&values);
    let current_path = world.temp_path().join("current.json");
    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize current");
    fs::write(&current_path, json).expect("Failed to write current receipt");
    world.current_path = Some(current_path);
}

/// Create a markdown template file
#[given(expr = "a markdown template file with content {string}")]
async fn given_markdown_template_file(world: &mut PerfgateWorld, content: String) {
    world.ensure_temp_dir();
    let template_path = world.temp_path().join("template.hbs");
    fs::write(&template_path, &content).expect("Failed to write template file");
    world.template_path = Some(template_path);
}

/// Create a baseline receipt with specified max_rss_kb
#[given(expr = "a baseline receipt with max_rss_kb median of {int}")]
async fn given_baseline_receipt_with_rss(world: &mut PerfgateWorld, max_rss_kb: u64) {
    world.ensure_temp_dir();
    let mut receipt = world.create_run_receipt(world.baseline_wall_ms.unwrap_or(1000));
    receipt.stats.max_rss_kb = Some(U64Summary::new(
        max_rss_kb,
        max_rss_kb.saturating_sub(100),
        max_rss_kb.saturating_add(100),
    ));
    let baseline_path = world.temp_path().join("baseline.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize baseline");
    fs::write(&baseline_path, json).expect("Failed to write baseline receipt");
    world.baseline_path = Some(baseline_path);
}

/// Create a current receipt with specified max_rss_kb
#[given(expr = "a current receipt with max_rss_kb median of {int}")]
async fn given_current_receipt_with_rss(world: &mut PerfgateWorld, max_rss_kb: u64) {
    world.ensure_temp_dir();
    let mut receipt = world.create_run_receipt(world.current_wall_ms.unwrap_or(1000));
    receipt.stats.max_rss_kb = Some(U64Summary::new(
        max_rss_kb,
        max_rss_kb.saturating_sub(100),
        max_rss_kb.saturating_add(100),
    ));
    let current_path = world.temp_path().join("current.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize current");
    fs::write(&current_path, json).expect("Failed to write current receipt");
    world.current_path = Some(current_path);
}

/// Set up a non-existent baseline file path (file is not created on disk)
#[given("a non-existent baseline file")]
async fn given_nonexistent_baseline(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    world.baseline_path = Some(world.temp_path().join("nonexistent-baseline.json"));
}

/// Set up a baseline file containing invalid JSON
#[given("an invalid JSON baseline file")]
async fn given_invalid_json_baseline(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let path = world.temp_path().join("invalid-baseline.json");
    fs::write(&path, "{ not valid json }").expect("Failed to write invalid JSON");
    world.baseline_path = Some(path);
}

/// Set up an empty baseline file
#[given("an empty baseline file")]
async fn given_empty_baseline(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let path = world.temp_path().join("empty-baseline.json");
    fs::write(&path, "").expect("Failed to write empty file");
    world.baseline_path = Some(path);
}

// ============================================================================
// WHEN STEPS - CLI Command Execution
// ============================================================================

/// Helper function to get the perfgate binary command
///
/// Note: Uses deprecated API because cargo_bin! macro requires compile-time
/// environment variables that aren't available for cross-crate testing.
#[allow(deprecated)]
fn perfgate_cmd() -> Command {
    Command::cargo_bin("perfgate").expect("Failed to find perfgate binary")
}

/// Run perfgate compare with specified threshold
#[when(expr = "I run perfgate compare with threshold {float}")]
async fn when_compare_with_threshold(world: &mut PerfgateWorld, threshold: f64) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let current = world.current_path.clone().expect("Current path not set");
    let output_path = world.temp_path().join("compare-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--threshold")
        .arg(threshold.to_string())
        .arg("--out")
        .arg(&output_path);

    for arg in &world.extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate compare");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate compare with threshold and warn-factor
#[when(expr = "I run perfgate compare with threshold {float} and warn-factor {float}")]
async fn when_compare_with_threshold_and_warn_factor(
    world: &mut PerfgateWorld,
    threshold: f64,
    warn_factor: f64,
) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let current = world.current_path.clone().expect("Current path not set");
    let output_path = world.temp_path().join("compare-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--threshold")
        .arg(threshold.to_string())
        .arg("--warn-factor")
        .arg(warn_factor.to_string())
        .arg("--out")
        .arg(&output_path);

    for arg in &world.extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate compare");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate compare with threshold and explicit host mismatch policy
#[when(expr = "I run perfgate compare with threshold {float} and host-mismatch policy {string}")]
async fn when_compare_with_threshold_and_host_mismatch_policy(
    world: &mut PerfgateWorld,
    threshold: f64,
    policy: String,
) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let current = world.current_path.clone().expect("Current path not set");
    let output_path = world.temp_path().join("compare-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--threshold")
        .arg(threshold.to_string())
        .arg("--host-mismatch")
        .arg(policy)
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate compare");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate compare with threshold and significance-alpha
#[when(expr = "I run perfgate compare with threshold {float} and significance-alpha {float}")]
async fn when_compare_with_significance_alpha(
    world: &mut PerfgateWorld,
    threshold: f64,
    alpha: f64,
) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let current = world.current_path.clone().expect("Current path not set");
    let output_path = world.temp_path().join("compare-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--threshold")
        .arg(threshold.to_string())
        .arg("--significance-alpha")
        .arg(alpha.to_string())
        .arg("--significance-min-samples")
        .arg("2")
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate compare");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate compare with threshold, significance-alpha and require-significance
#[when(
    expr = "I run perfgate compare with threshold {float} and significance-alpha {float} and require-significance"
)]
async fn when_compare_with_require_significance(
    world: &mut PerfgateWorld,
    threshold: f64,
    alpha: f64,
) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let current = world.current_path.clone().expect("Current path not set");
    let output_path = world.temp_path().join("compare-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--threshold")
        .arg(threshold.to_string())
        .arg("--significance-alpha")
        .arg(alpha.to_string())
        .arg("--significance-min-samples")
        .arg("2")
        .arg("--require-significance")
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate compare");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate md command
#[when("I run perfgate md")]
async fn when_md(world: &mut PerfgateWorld) {
    let compare = world.compare_path.clone().expect("Compare path not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("md").arg("--compare").arg(&compare);

    let output = cmd.output().expect("Failed to execute perfgate md");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate md command with output file
#[when("I run perfgate md with output file")]
async fn when_md_with_output(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let output_path = world.temp_path().join("output.md");

    let mut cmd = perfgate_cmd();
    cmd.arg("md")
        .arg("--compare")
        .arg(&compare)
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate md");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate md command with a custom template
#[when("I run perfgate md with template")]
async fn when_md_with_template(world: &mut PerfgateWorld) {
    let compare = world.compare_path.clone().expect("Compare path not set");
    let template = world.template_path.clone().expect("Template path not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("md")
        .arg("--compare")
        .arg(&compare)
        .arg("--template")
        .arg(&template);

    let output = cmd.output().expect("Failed to execute perfgate md");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate github-annotations command
#[when("I run perfgate github-annotations")]
async fn when_github_annotations(world: &mut PerfgateWorld) {
    let compare = world.compare_path.clone().expect("Compare path not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("github-annotations").arg("--compare").arg(&compare);

    let output = cmd
        .output()
        .expect("Failed to execute perfgate github-annotations");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate run command with a simple echo command
#[when(expr = "I run perfgate run with name {string} and command {string}")]
async fn when_run_with_name_and_command(world: &mut PerfgateWorld, name: String, command: String) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg(&name)
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--");

    for part in command.split_whitespace() {
        cmd.arg(part);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate run command with just a name (uses cross-platform success command)
#[when(expr = "I run perfgate run with name {string}")]
async fn when_run_with_name(world: &mut PerfgateWorld, name: String) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg(&name)
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Returns a cross-platform command that exits successfully.
/// On Unix: ["true"]
/// On Windows: ["cmd", "/c", "exit", "0"]
#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

/// Returns a cross-platform command that takes at least ~10ms.
/// Used for benchmarks that need to produce measurable wall_ms
/// (e.g., when regression detection against a 1ms baseline is needed).
#[cfg(unix)]
fn slow_command() -> Vec<&'static str> {
    vec!["sleep", "0.01"]
}

#[cfg(windows)]
fn slow_command() -> Vec<&'static str> {
    // Windows cmd.exe startup takes ~50ms, which is sufficient
    vec!["cmd", "/c", "exit", "0"]
}

/// Returns a cross-platform shell command string that exits successfully.
#[cfg(unix)]
fn success_shell_command() -> &'static str {
    "true"
}

#[cfg(windows)]
fn success_shell_command() -> &'static str {
    "cmd /c exit 0"
}

/// Returns a cross-platform shell command string that exits with failure.
#[cfg(unix)]
fn fail_shell_command() -> &'static str {
    "false"
}

#[cfg(windows)]
fn fail_shell_command() -> &'static str {
    "cmd /c exit 1"
}

/// Run perfgate run with repeat and warmup options
#[when(expr = "I run perfgate run with repeat {int} and warmup {int}")]
async fn when_run_with_repeat_and_warmup(world: &mut PerfgateWorld, repeat: u32, warmup: u32) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg(repeat.to_string())
        .arg("--warmup")
        .arg(warmup.to_string())
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate run with work units
#[when(expr = "I run perfgate run with work units {int}")]
async fn when_run_with_work_units(world: &mut PerfgateWorld, work_units: u64) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--work")
        .arg(work_units.to_string())
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate run with timeout
#[when(expr = "I run perfgate run with timeout {string}")]
async fn when_run_with_timeout(world: &mut PerfgateWorld, timeout: String) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--timeout")
        .arg(&timeout)
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate run with --output-cap-bytes
#[when(expr = "I run perfgate run with output-cap-bytes {int}")]
async fn when_run_with_output_cap_bytes(world: &mut PerfgateWorld, cap: usize) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--output-cap-bytes")
        .arg(cap.to_string())
        .arg("--");

    for arg in echo_command() {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate run with --env KEY=VALUE
#[when(expr = "I run perfgate run with env {string}")]
async fn when_run_with_env(world: &mut PerfgateWorld, env_pair: String) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--env")
        .arg(&env_pair)
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Returns a cross-platform command that produces output.
#[cfg(unix)]
fn echo_command() -> Vec<&'static str> {
    vec!["echo", "hello world from perfgate"]
}

#[cfg(windows)]
fn echo_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "echo", "hello world from perfgate"]
}

/// Returns a cross-platform command that produces large stdout output.
#[cfg(unix)]
fn large_stdout_command() -> Vec<&'static str> {
    vec!["seq", "1", "10000"]
}

#[cfg(windows)]
fn large_stdout_command() -> Vec<&'static str> {
    vec!["powershell", "-NoProfile", "-Command", "1..10000"]
}

/// Run perfgate run with a command that produces very large stdout
#[when("I run perfgate run with a large stdout command")]
async fn when_run_with_large_stdout_command(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--");

    for arg in large_stdout_command() {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate run with a command that does not exist
#[when("I run perfgate run with a non-existent command")]
async fn when_run_with_nonexistent_command(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("run-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--out")
        .arg(&output_path)
        .arg("--repeat")
        .arg("1")
        .arg("--")
        .arg("nonexistent_command_xyz_12345");

    let output = cmd.output().expect("Failed to execute perfgate run");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired with shell command strings.
#[when("I run perfgate paired with shell commands")]
async fn when_paired_with_shell_commands(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg("paired-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd")
        .arg(success_shell_command())
        .arg("--current-cmd")
        .arg(success_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired with repeat and warmup values.
#[when(expr = "I run perfgate paired with repeat {int} and warmup {int}")]
async fn when_paired_with_repeat_and_warmup(world: &mut PerfgateWorld, repeat: u32, warmup: u32) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg("paired-warmup-bench")
        .arg("--repeat")
        .arg(repeat.to_string())
        .arg("--warmup")
        .arg(warmup.to_string())
        .arg("--baseline-cmd")
        .arg(success_shell_command())
        .arg("--current-cmd")
        .arg(success_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired where baseline returns nonzero.
#[when("I run perfgate paired with a failing baseline command")]
async fn when_paired_with_failing_baseline(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg("paired-fail-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd")
        .arg(fail_shell_command())
        .arg("--current-cmd")
        .arg(success_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired with allow-nonzero and failing baseline.
#[when("I run perfgate paired with allow-nonzero and a failing baseline command")]
async fn when_paired_with_allow_nonzero_and_failing_baseline(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg("paired-allow-nonzero-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--allow-nonzero")
        .arg("--baseline-cmd")
        .arg(fail_shell_command())
        .arg("--current-cmd")
        .arg(success_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired with --pretty flag.
#[when("I run perfgate paired with pretty output")]
async fn when_paired_with_pretty(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg("paired-pretty-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--pretty")
        .arg("--baseline-cmd")
        .arg(success_shell_command())
        .arg("--current-cmd")
        .arg(success_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired with a failing current command.
#[when("I run perfgate paired with a failing current command")]
async fn when_paired_with_failing_current(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg("paired-fail-current-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd")
        .arg(success_shell_command())
        .arg("--current-cmd")
        .arg(fail_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired with a custom bench name.
#[when(expr = "I run perfgate paired with bench name {string}")]
async fn when_paired_with_custom_name(world: &mut PerfgateWorld, name: String) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg(&name)
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd")
        .arg(success_shell_command())
        .arg("--current-cmd")
        .arg(success_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

/// Run perfgate paired with --work units.
#[when(expr = "I run perfgate paired with work units {int}")]
async fn when_paired_with_work_units(world: &mut PerfgateWorld, work: u64) {
    world.ensure_temp_dir();
    let output_path = world.temp_path().join("paired-output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("paired")
        .arg("--name")
        .arg("paired-work-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--work")
        .arg(work.to_string())
        .arg("--baseline-cmd")
        .arg(success_shell_command())
        .arg("--current-cmd")
        .arg(success_shell_command())
        .arg("--out")
        .arg(&output_path);

    let output = cmd.output().expect("Failed to execute perfgate paired");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.output_path = Some(output_path);
}

// ============================================================================
// THEN STEPS - Assertions
// ============================================================================

/// Assert the exit code matches expected value
#[then(expr = "the exit code should be {int}")]
async fn then_exit_code(world: &mut PerfgateWorld, expected: i32) {
    let actual = world.last_exit_code.expect("No exit code recorded");
    assert_eq!(
        actual, expected,
        "Expected exit code {}, got {}. Stderr: {}",
        expected, actual, world.last_stderr
    );
}

/// Assert the verdict matches expected value
#[then(expr = "the verdict should be {word}")]
async fn then_verdict(world: &mut PerfgateWorld, expected: String) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: CompareReceipt =
        serde_json::from_str(&content).expect("Failed to parse compare receipt");

    let actual = match receipt.verdict.status {
        VerdictStatus::Pass => "pass",
        VerdictStatus::Warn => "warn",
        VerdictStatus::Fail => "fail",
        VerdictStatus::Skip => "skip",
    };

    assert_eq!(
        actual,
        expected.to_lowercase(),
        "Expected verdict '{}', got '{}'",
        expected,
        actual
    );
}

/// Assert the compare receipt contains wall_ms delta
#[then("the compare receipt should contain wall_ms delta")]
async fn then_compare_receipt_contains_wall_ms_delta(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: CompareReceipt =
        serde_json::from_str(&content).expect("Failed to parse compare receipt");

    assert!(
        receipt.deltas.contains_key(&Metric::WallMs),
        "Compare receipt should contain wall_ms delta"
    );
}

/// Assert the compare receipt wall_ms delta has significance metadata
#[then("the compare receipt wall_ms delta should have significance metadata")]
async fn then_compare_receipt_has_significance(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: CompareReceipt =
        serde_json::from_str(&content).expect("Failed to parse compare receipt");

    let delta = receipt
        .deltas
        .get(&Metric::WallMs)
        .expect("wall_ms delta should exist");
    assert!(
        delta.significance.is_some(),
        "wall_ms delta should have significance metadata, got None"
    );
}

/// Assert the run receipt sample stdout is at most N bytes
#[then(expr = "the run receipt sample stdout should be at most {int} bytes")]
async fn then_run_receipt_stdout_capped(world: &mut PerfgateWorld, max_bytes: usize) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run receipt");

    for sample in &receipt.samples {
        if let Some(ref stdout) = sample.stdout {
            assert!(
                stdout.len() <= max_bytes,
                "Sample stdout ({} bytes) exceeds cap of {} bytes",
                stdout.len(),
                max_bytes
            );
        }
    }
}

/// Assert the reasons include a specific token
#[then(expr = "the reasons should include token {string}")]
async fn then_reasons_include_token(world: &mut PerfgateWorld, token: String) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: CompareReceipt =
        serde_json::from_str(&content).expect("Failed to parse compare receipt");

    assert!(
        !receipt.verdict.reasons.is_empty(),
        "Verdict should have reason tokens"
    );

    assert!(
        receipt.verdict.reasons.iter().any(|r| r == &token),
        "Reasons should include token '{}': {:?}",
        token,
        receipt.verdict.reasons
    );
}

/// Assert stdout contains expected text
#[then(expr = "the stdout should contain {string}")]
async fn then_stdout_contains(world: &mut PerfgateWorld, expected: String) {
    assert!(
        world.last_stdout.contains(&expected),
        "Expected stdout to contain '{}', got: {}",
        expected,
        world.last_stdout
    );
}

/// Assert stderr contains expected text
#[then(expr = "the stderr should contain {string}")]
async fn then_stderr_contains(world: &mut PerfgateWorld, expected: String) {
    assert!(
        world.last_stderr.contains(&expected),
        "Expected stderr to contain '{}', got: {}",
        expected,
        world.last_stderr
    );
}

/// Assert stderr does not contain expected text
#[then(expr = "the stderr should not contain {string}")]
async fn then_stderr_not_contains(world: &mut PerfgateWorld, unexpected: String) {
    assert!(
        !world.last_stderr.contains(&unexpected),
        "Expected stderr to not contain '{}', got: {}",
        unexpected,
        world.last_stderr
    );
}

/// Assert stdout is empty
#[then("the stdout should be empty")]
async fn then_stdout_empty(world: &mut PerfgateWorld) {
    assert!(
        world.last_stdout.trim().is_empty(),
        "Expected stdout to be empty, got: {}",
        world.last_stdout
    );
}

/// Assert the output file exists
#[then("the output file should exist")]
async fn then_output_file_exists(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    assert!(
        output_path.exists(),
        "Output file should exist at {:?}",
        output_path
    );
}

/// Assert the output file does not exist
#[then("the output file should not exist")]
async fn then_output_file_not_exists(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    assert!(
        !output_path.exists(),
        "Output file should not exist at {:?}",
        output_path
    );
}

/// Assert the output file contains valid JSON
#[then("the output file should contain valid JSON")]
async fn then_output_file_valid_json(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let _: serde_json::Value =
        serde_json::from_str(&content).expect("Output file should contain valid JSON");
}

/// Assert the run receipt has expected number of samples
#[then(expr = "the run receipt should have {int} samples")]
async fn then_run_receipt_sample_count(world: &mut PerfgateWorld, expected: usize) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run receipt");

    assert_eq!(
        receipt.samples.len(),
        expected,
        "Expected {} samples, got {}",
        expected,
        receipt.samples.len()
    );
}

/// Assert the run receipt has warmup samples marked correctly
#[then(expr = "the run receipt should have {int} warmup samples")]
async fn then_run_receipt_warmup_count(world: &mut PerfgateWorld, expected: usize) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run receipt");

    let warmup_count = receipt.samples.iter().filter(|s| s.warmup).count();
    assert_eq!(
        warmup_count, expected,
        "Expected {} warmup samples, got {}",
        expected, warmup_count
    );
}

/// Assert the run receipt has throughput_per_s stats
#[then("the run receipt should have throughput_per_s stats")]
async fn then_run_receipt_has_throughput(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run receipt");

    assert!(
        receipt.stats.throughput_per_s.is_some(),
        "Run receipt should have throughput_per_s stats"
    );
}

/// Assert the markdown output contains expected text
#[then(expr = "the markdown should contain {string}")]
async fn then_markdown_contains(world: &mut PerfgateWorld, expected: String) {
    assert!(
        world.last_stdout.contains(&expected),
        "Expected markdown to contain '{}', got: {}",
        expected,
        world.last_stdout
    );
}

/// Assert the markdown output file contains expected content
#[then(expr = "the markdown file should contain {string}")]
async fn then_markdown_file_contains(world: &mut PerfgateWorld, expected: String) {
    // Check md_output_path first (for report command), then output_path (for md command)
    let md_path = world
        .md_output_path
        .as_ref()
        .or(world.output_path.as_ref())
        .expect("No markdown path set");
    let content = fs::read_to_string(md_path).expect("Failed to read markdown file");

    assert!(
        content.contains(&expected),
        "Expected markdown file to contain '{}', got: {}",
        expected,
        content
    );
}

/// Assert github-annotations output contains error annotation
#[then("the output should contain an error annotation")]
async fn then_output_contains_error_annotation(world: &mut PerfgateWorld) {
    assert!(
        world.last_stdout.contains("::error::"),
        "Expected output to contain '::error::', got: {}",
        world.last_stdout
    );
}

/// Assert github-annotations output contains warning annotation
#[then("the output should contain a warning annotation")]
async fn then_output_contains_warning_annotation(world: &mut PerfgateWorld) {
    assert!(
        world.last_stdout.contains("::warning::"),
        "Expected output to contain '::warning::', got: {}",
        world.last_stdout
    );
}

/// Assert github-annotations output contains no annotations
#[then("the output should contain no annotations")]
async fn then_output_contains_no_annotations(world: &mut PerfgateWorld) {
    assert!(
        !world.last_stdout.contains("::error::") && !world.last_stdout.contains("::warning::"),
        "Expected no annotations, got: {}",
        world.last_stdout
    );
}

/// Assert the annotation contains the bench name
#[then(expr = "the annotation should contain bench name {string}")]
async fn then_annotation_contains_bench_name(world: &mut PerfgateWorld, bench_name: String) {
    assert!(
        world.last_stdout.contains(&bench_name),
        "Expected annotation to contain bench name '{}', got: {}",
        bench_name,
        world.last_stdout
    );
}

/// Assert the run receipt has the correct bench name
#[then(expr = "the run receipt should have bench name {string}")]
async fn then_run_receipt_bench_name(world: &mut PerfgateWorld, expected: String) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run receipt");

    assert_eq!(
        receipt.bench.name, expected,
        "Expected bench name '{}', got '{}'",
        expected, receipt.bench.name
    );
}

/// Assert the run receipt has the correct schema version
#[then("the run receipt should have schema perfgate.run.v1")]
async fn then_run_receipt_schema(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run receipt");

    assert_eq!(
        receipt.schema, RUN_SCHEMA_V1,
        "Expected schema '{}', got '{}'",
        RUN_SCHEMA_V1, receipt.schema
    );
}

/// Assert the compare receipt has the correct schema version
#[then("the compare receipt should have schema perfgate.compare.v1")]
async fn then_compare_receipt_schema(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: CompareReceipt =
        serde_json::from_str(&content).expect("Failed to parse compare receipt");

    assert_eq!(
        receipt.schema, COMPARE_SCHEMA_V1,
        "Expected schema '{}', got '{}'",
        COMPARE_SCHEMA_V1, receipt.schema
    );
}

/// Assert the paired receipt has the correct schema version
#[then("the paired receipt should have schema perfgate.paired.v1")]
async fn then_paired_receipt_schema(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    assert_eq!(
        receipt.schema, PAIRED_SCHEMA_V1,
        "Expected schema '{}', got '{}'",
        PAIRED_SCHEMA_V1, receipt.schema
    );
}

/// Assert the paired receipt has the expected bench name
#[then(expr = "the paired receipt should have bench name {string}")]
async fn then_paired_receipt_bench_name(world: &mut PerfgateWorld, expected: String) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    assert_eq!(
        receipt.bench.name, expected,
        "Expected bench name '{}', got '{}'",
        expected, receipt.bench.name
    );
}

/// Assert the paired receipt has expected sample count
#[then(expr = "the paired receipt should have {int} samples")]
async fn then_paired_receipt_sample_count(world: &mut PerfgateWorld, expected: usize) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    assert_eq!(
        receipt.samples.len(),
        expected,
        "Expected {} samples, got {}",
        expected,
        receipt.samples.len()
    );
}

/// Assert the paired receipt has expected warmup sample count
#[then(expr = "the paired receipt should have {int} warmup samples")]
async fn then_paired_receipt_warmup_count(world: &mut PerfgateWorld, expected: usize) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    let warmup_count = receipt.samples.iter().filter(|s| s.warmup).count();
    assert_eq!(
        warmup_count, expected,
        "Expected {} warmup samples, got {}",
        expected, warmup_count
    );
}

/// Assert the paired receipt stats diff count matches measured (non-warmup) samples
#[then(expr = "the paired receipt stats diff count should be {int}")]
async fn then_paired_receipt_stats_diff_count(world: &mut PerfgateWorld, expected: u32) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    assert_eq!(
        receipt.stats.wall_diff_ms.count, expected,
        "Expected stats diff count {}, got {}",
        expected, receipt.stats.wall_diff_ms.count
    );
}

/// Assert the output file is pretty-printed (multi-line JSON)
#[then("the output file should be pretty-printed")]
async fn then_output_file_pretty_printed(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let line_count = content.lines().count();
    assert!(
        line_count > 1,
        "Expected pretty-printed JSON (multiple lines), got {} line(s)",
        line_count
    );
}

/// Assert the paired receipt contains stats with baseline and current wall_ms summaries
#[then("the paired receipt should contain stats with baseline and current wall_ms")]
async fn then_paired_receipt_has_wall_ms_stats(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    assert!(
        receipt.stats.baseline_wall_ms.median > 0 || receipt.stats.baseline_wall_ms.min == 0,
        "Expected baseline_wall_ms stats to be present"
    );
    assert!(
        receipt.stats.current_wall_ms.median > 0 || receipt.stats.current_wall_ms.min == 0,
        "Expected current_wall_ms stats to be present"
    );
    assert!(
        receipt.stats.wall_diff_ms.count > 0,
        "Expected wall_diff_ms count > 0"
    );
}

/// Assert the paired receipt contains run metadata (id, started_at, ended_at)
#[then("the paired receipt should contain run metadata")]
async fn then_paired_receipt_has_run_metadata(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    assert!(
        !receipt.run.id.is_empty(),
        "Expected run.id to be non-empty"
    );
    assert!(
        !receipt.run.started_at.is_empty(),
        "Expected run.started_at to be non-empty"
    );
    assert!(
        !receipt.run.ended_at.is_empty(),
        "Expected run.ended_at to be non-empty"
    );
}

/// Assert the paired receipt has throughput stats populated
#[then("the paired receipt should have throughput stats")]
async fn then_paired_receipt_has_throughput_stats(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: PairedRunReceipt =
        serde_json::from_str(&content).expect("Failed to parse paired receipt");

    assert!(
        receipt.stats.baseline_throughput_per_s.is_some(),
        "Expected baseline_throughput_per_s to be present when --work is specified"
    );
    assert!(
        receipt.stats.current_throughput_per_s.is_some(),
        "Expected current_throughput_per_s to be present when --work is specified"
    );
}

// ============================================================================
// EXPORT STEPS
// ============================================================================

/// Run perfgate export run to csv
#[when("I run perfgate export run to csv")]
async fn when_export_run_to_csv(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let export_path = world.temp_path().join("export.csv");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export run to jsonl
#[when("I run perfgate export run to jsonl")]
async fn when_export_run_to_jsonl(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let export_path = world.temp_path().join("export.jsonl");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("jsonl")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export run to html
#[when("I run perfgate export run to html")]
async fn when_export_run_to_html(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let export_path = world.temp_path().join("export.html");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("html")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export compare to csv
#[when("I run perfgate export compare to csv")]
async fn when_export_compare_to_csv(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let export_path = world.temp_path().join("export.csv");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export compare to jsonl
#[when("I run perfgate export compare to jsonl")]
async fn when_export_compare_to_jsonl(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let export_path = world.temp_path().join("export.jsonl");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare)
        .arg("--format")
        .arg("jsonl")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export compare to html
#[when("I run perfgate export compare to html")]
async fn when_export_compare_to_html(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let export_path = world.temp_path().join("export.html");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare)
        .arg("--format")
        .arg("html")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export compare to prometheus
#[when("I run perfgate export compare to prometheus")]
async fn when_export_compare_to_prometheus(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let export_path = world.temp_path().join("export.prom");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare)
        .arg("--format")
        .arg("prometheus")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export compare to csv twice for determinism check
#[when("I run perfgate export compare to csv twice")]
async fn when_export_compare_to_csv_twice(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let export_path1 = world.temp_path().join("export1.csv");
    let export_path2 = world.temp_path().join("export2.csv");

    // First export
    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path1);
    let _ = cmd.output().expect("Failed to execute perfgate export");

    // Second export
    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--compare")
        .arg(&compare)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path2);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path1);
    world.export_path2 = Some(export_path2);
}

/// Run perfgate export run with default format
#[when("I run perfgate export run with default format")]
async fn when_export_run_default_format(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let export_path = world.temp_path().join("export.csv");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export run with an invalid format string
#[when("I run perfgate export run with invalid format")]
async fn when_export_run_invalid_format(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let baseline = world.baseline_path.clone().expect("Baseline path not set");
    let export_path = world.temp_path().join("export.out");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("badformat")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Run perfgate export run with a non-existent input file
#[when("I run perfgate export run with a non-existent file")]
async fn when_export_run_nonexistent_file(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let export_path = world.temp_path().join("export.csv");

    let mut cmd = perfgate_cmd();
    cmd.arg("export")
        .arg("--run")
        .arg("nonexistent-run-receipt.json")
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    let output = cmd.output().expect("Failed to execute perfgate export");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.export_path = Some(export_path);
}

/// Assert the export file exists
#[then("the export file should exist")]
async fn then_export_file_exists(world: &mut PerfgateWorld) {
    let export_path = world.export_path.as_ref().expect("No export path set");
    assert!(
        export_path.exists(),
        "Export file should exist at {:?}",
        export_path
    );
}

/// Assert the export file contains expected text
#[then(expr = "the export file should contain {string}")]
async fn then_export_file_contains(world: &mut PerfgateWorld, expected: String) {
    let export_path = world.export_path.as_ref().expect("No export path set");
    let content = fs::read_to_string(export_path).expect("Failed to read export file");

    assert!(
        content.contains(&expected),
        "Expected export file to contain '{}', got: {}",
        expected,
        content
    );
}

/// Assert the export file is valid JSONL
#[then("the export file should be valid JSONL")]
async fn then_export_file_valid_jsonl(world: &mut PerfgateWorld) {
    let export_path = world.export_path.as_ref().expect("No export path set");
    let content = fs::read_to_string(export_path).expect("Failed to read export file");

    for (i, line) in content.trim().split('\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let _: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|_| panic!("Line {} should be valid JSON: {}", i + 1, line));
    }
}

/// Assert the two export files are identical
#[then("the two export files should be identical")]
async fn then_export_files_identical(world: &mut PerfgateWorld) {
    let export_path1 = world.export_path.as_ref().expect("No export path set");
    let export_path2 = world.export_path2.as_ref().expect("No export path 2 set");

    let content1 = fs::read_to_string(export_path1).expect("Failed to read export file 1");
    let content2 = fs::read_to_string(export_path2).expect("Failed to read export file 2");

    assert_eq!(
        content1, content2,
        "Export files should be identical for deterministic output"
    );
}

/// Assert metrics are sorted alphabetically in the export
#[then("the metrics should be sorted alphabetically")]
async fn then_metrics_sorted_alphabetically(world: &mut PerfgateWorld) {
    let export_path = world.export_path.as_ref().expect("No export path set");
    let content = fs::read_to_string(export_path).expect("Failed to read export file");

    // Check that max_rss_kb comes before wall_ms (alphabetical order)
    // Skip the header line
    let lines: Vec<&str> = content.trim().split('\n').collect();
    if lines.len() > 1 {
        // Extract metric names from data lines
        let mut metrics: Vec<String> = Vec::new();
        for line in lines.iter().skip(1) {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() > 1 {
                // The metric is the second column
                metrics.push(parts[1].trim_matches('"').to_string());
            }
        }

        // Check if sorted
        let mut sorted_metrics = metrics.clone();
        sorted_metrics.sort();

        assert_eq!(
            metrics, sorted_metrics,
            "Metrics should be sorted alphabetically. Got: {:?}",
            metrics
        );
    }
}

// ============================================================================
// PROMOTE STEPS
// ============================================================================

/// Create a run receipt with specified wall_ms median for promote
#[given(expr = "a run receipt with wall_ms median of {int}")]
async fn given_run_receipt_for_promote(world: &mut PerfgateWorld, wall_ms: u64) {
    world.ensure_temp_dir();
    world.baseline_wall_ms = Some(wall_ms);

    let mut receipt = world.create_run_receipt(wall_ms);

    // Apply any custom fields
    if let Some(run_id) = &world.source_run_id {
        receipt.run.id = run_id.clone();
    }
    if let Some(started_at) = &world.source_started_at {
        receipt.run.started_at = started_at.clone();
    }
    if let Some(bench_name) = &world.source_bench_name {
        receipt.bench.name = bench_name.clone();
    }

    let source_path = world.temp_path().join("source.json");
    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize source receipt");
    fs::write(&source_path, json).expect("Failed to write source receipt");
    world.source_run_path = Some(source_path);
}

/// Set custom run_id for the source receipt
#[given(expr = "the run receipt has run_id {string}")]
async fn given_run_receipt_has_run_id(world: &mut PerfgateWorld, run_id: String) {
    world.source_run_id = Some(run_id.clone());

    // If source already exists, update it
    if let Some(source_path) = &world.source_run_path {
        let content = fs::read_to_string(source_path).expect("Failed to read source");
        let mut receipt: RunReceipt =
            serde_json::from_str(&content).expect("Failed to parse source");
        receipt.run.id = run_id;
        let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize");
        fs::write(source_path, json).expect("Failed to write source");
    }
}

/// Set custom started_at for the source receipt
#[given(expr = "the run receipt has started_at {string}")]
async fn given_run_receipt_has_started_at(world: &mut PerfgateWorld, started_at: String) {
    world.source_started_at = Some(started_at.clone());

    // If source already exists, update it
    if let Some(source_path) = &world.source_run_path {
        let content = fs::read_to_string(source_path).expect("Failed to read source");
        let mut receipt: RunReceipt =
            serde_json::from_str(&content).expect("Failed to parse source");
        receipt.run.started_at = started_at;
        let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize");
        fs::write(source_path, json).expect("Failed to write source");
    }
}

/// Set custom bench name for the source receipt
#[given(expr = "the run receipt has bench name {string}")]
async fn given_run_receipt_has_bench_name(world: &mut PerfgateWorld, bench_name: String) {
    world.source_bench_name = Some(bench_name.clone());

    // If source already exists, update it
    if let Some(source_path) = &world.source_run_path {
        let content = fs::read_to_string(source_path).expect("Failed to read source");
        let mut receipt: RunReceipt =
            serde_json::from_str(&content).expect("Failed to parse source");
        receipt.bench.name = bench_name;
        let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize");
        fs::write(source_path, json).expect("Failed to write source");
    }
}

/// Set up a nonexistent source file
#[given("a nonexistent source file")]
async fn given_nonexistent_source_file(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    world.source_run_path = Some(world.temp_path().join("nonexistent.json"));
}

/// Set up an invalid JSON source file
#[given("an invalid JSON source file")]
async fn given_invalid_json_source_file(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let source_path = world.temp_path().join("invalid.json");
    fs::write(&source_path, "{ invalid json }").expect("Failed to write invalid JSON");
    world.source_run_path = Some(source_path);
}

/// Run perfgate promote (default, no normalize)
#[when("I run perfgate promote")]
async fn when_promote(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let source = world.source_run_path.clone().expect("Source path not set");
    let baseline_path = world.temp_path().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&source)
        .arg("--to")
        .arg(&baseline_path);

    let output = cmd.output().expect("Failed to execute perfgate promote");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.promoted_baseline_path = Some(baseline_path);
}

/// Run perfgate promote without normalize
#[when("I run perfgate promote without normalize")]
async fn when_promote_without_normalize(world: &mut PerfgateWorld) {
    when_promote(world).await;
}

/// Run perfgate promote with normalize flag
#[when("I run perfgate promote with normalize")]
async fn when_promote_with_normalize(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let source = world.source_run_path.clone().expect("Source path not set");
    let baseline_path = world.temp_path().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&source)
        .arg("--to")
        .arg(&baseline_path)
        .arg("--normalize");

    let output = cmd.output().expect("Failed to execute perfgate promote");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.promoted_baseline_path = Some(baseline_path);
}

/// Run perfgate promote with pretty flag
#[when("I run perfgate promote with pretty")]
async fn when_promote_with_pretty(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let source = world.source_run_path.clone().expect("Source path not set");
    let baseline_path = world.temp_path().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&source)
        .arg("--to")
        .arg(&baseline_path)
        .arg("--pretty");

    let output = cmd.output().expect("Failed to execute perfgate promote");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.promoted_baseline_path = Some(baseline_path);
}

/// Run perfgate promote with missing source
#[when("I run perfgate promote with missing source")]
async fn when_promote_with_missing_source(world: &mut PerfgateWorld) {
    when_promote(world).await;
}

/// Run perfgate promote with invalid source
#[when("I run perfgate promote with invalid source")]
async fn when_promote_with_invalid_source(world: &mut PerfgateWorld) {
    when_promote(world).await;
}

/// Assert the baseline file exists
#[then("the baseline file should exist")]
async fn then_baseline_file_exists(world: &mut PerfgateWorld) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    assert!(
        baseline_path.exists(),
        "Baseline file should exist at {:?}",
        baseline_path
    );
}

/// Assert the baseline file is valid JSON
#[then("the baseline file should be valid JSON")]
async fn then_baseline_file_valid_json(world: &mut PerfgateWorld) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    let content = fs::read_to_string(baseline_path).expect("Failed to read baseline file");
    let _: serde_json::Value =
        serde_json::from_str(&content).expect("Baseline file should be valid JSON");
}

/// Assert the baseline has the same wall_ms median
#[then(expr = "the baseline should have the same wall_ms median of {int}")]
async fn then_baseline_has_wall_ms_median(world: &mut PerfgateWorld, expected: u64) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    let content = fs::read_to_string(baseline_path).expect("Failed to read baseline");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse baseline");

    assert_eq!(
        receipt.stats.wall_ms.median, expected,
        "Expected wall_ms median {}, got {}",
        expected, receipt.stats.wall_ms.median
    );
}

/// Assert the baseline has specific run_id
#[then(expr = "the baseline should have run_id {string}")]
async fn then_baseline_has_run_id(world: &mut PerfgateWorld, expected: String) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    let content = fs::read_to_string(baseline_path).expect("Failed to read baseline");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse baseline");

    assert_eq!(
        receipt.run.id, expected,
        "Expected run_id '{}', got '{}'",
        expected, receipt.run.id
    );
}

/// Assert the baseline has specific started_at
#[then(expr = "the baseline should have started_at {string}")]
async fn then_baseline_has_started_at(world: &mut PerfgateWorld, expected: String) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    let content = fs::read_to_string(baseline_path).expect("Failed to read baseline");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse baseline");

    assert_eq!(
        receipt.run.started_at, expected,
        "Expected started_at '{}', got '{}'",
        expected, receipt.run.started_at
    );
}

/// Assert the baseline has specific bench name
#[then(expr = "the baseline should have bench name {string}")]
async fn then_baseline_has_bench_name(world: &mut PerfgateWorld, expected: String) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    let content = fs::read_to_string(baseline_path).expect("Failed to read baseline");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse baseline");

    assert_eq!(
        receipt.bench.name, expected,
        "Expected bench name '{}', got '{}'",
        expected, receipt.bench.name
    );
}

/// Assert the baseline preserves host os and arch
#[then("the baseline should preserve host os and arch")]
async fn then_baseline_preserves_host_info(world: &mut PerfgateWorld) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    let content = fs::read_to_string(baseline_path).expect("Failed to read baseline");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse baseline");

    // Host info should be preserved (linux/x86_64 from the default create_run_receipt)
    assert!(
        !receipt.run.host.os.is_empty(),
        "Host OS should be preserved"
    );
    assert!(
        !receipt.run.host.arch.is_empty(),
        "Host arch should be preserved"
    );
}

/// Assert no temporary files remain
#[then("no temporary files should remain")]
async fn then_no_temp_files_remain(world: &mut PerfgateWorld) {
    let temp_path = world.temp_path();
    let entries: Vec<_> = fs::read_dir(&temp_path)
        .expect("Failed to read temp dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with('.')
                && e.file_name().to_string_lossy().ends_with(".tmp")
        })
        .collect();

    assert!(
        entries.is_empty(),
        "No .tmp files should remain, found: {:?}",
        entries.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
}

/// Assert the baseline file is pretty-printed JSON
#[then("the baseline file should be pretty-printed JSON")]
async fn then_baseline_is_pretty_printed(world: &mut PerfgateWorld) {
    let baseline_path = world
        .promoted_baseline_path
        .as_ref()
        .expect("No baseline path set");
    let content = fs::read_to_string(baseline_path).expect("Failed to read baseline file");

    // Pretty-printed JSON should contain newlines
    assert!(
        content.contains('\n'),
        "Pretty-printed JSON should contain newlines"
    );

    // Pretty-printed JSON should have indentation
    assert!(
        content.contains("  "),
        "Pretty-printed JSON should have indentation"
    );
}

// ============================================================================
// REPORT STEPS
// ============================================================================

/// Run perfgate report command
#[when("I run perfgate report")]
async fn when_report(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let report_path = world.temp_path().join("report.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("report")
        .arg("--compare")
        .arg(&compare)
        .arg("--out")
        .arg(&report_path);

    let output = cmd.output().expect("Failed to execute perfgate report");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.report_path = Some(report_path);
}

/// Run perfgate report twice for determinism check
#[when("I run perfgate report twice")]
async fn when_report_twice(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let report_path1 = world.temp_path().join("report1.json");
    let report_path2 = world.temp_path().join("report2.json");

    // First report
    let mut cmd = perfgate_cmd();
    cmd.arg("report")
        .arg("--compare")
        .arg(&compare)
        .arg("--out")
        .arg(&report_path1);
    let _ = cmd.output().expect("Failed to execute perfgate report");

    // Second report
    let mut cmd = perfgate_cmd();
    cmd.arg("report")
        .arg("--compare")
        .arg(&compare)
        .arg("--out")
        .arg(&report_path2);

    let output = cmd.output().expect("Failed to execute perfgate report");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.report_path = Some(report_path1);
    world.report_path2 = Some(report_path2);
}

/// Run perfgate report with markdown output
#[when("I run perfgate report with markdown output")]
async fn when_report_with_markdown(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let report_path = world.temp_path().join("report.json");
    let md_path = world.temp_path().join("comment.md");

    let mut cmd = perfgate_cmd();
    cmd.arg("report")
        .arg("--compare")
        .arg(&compare)
        .arg("--out")
        .arg(&report_path)
        .arg("--md")
        .arg(&md_path);

    let output = cmd.output().expect("Failed to execute perfgate report");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.report_path = Some(report_path);
    world.md_output_path = Some(md_path);
}

/// Run perfgate report with pretty flag
#[when("I run perfgate report with pretty flag")]
async fn when_report_with_pretty(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();
    let compare = world.compare_path.clone().expect("Compare path not set");
    let report_path = world.temp_path().join("report.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("report")
        .arg("--compare")
        .arg(&compare)
        .arg("--out")
        .arg(&report_path)
        .arg("--pretty");

    let output = cmd.output().expect("Failed to execute perfgate report");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.report_path = Some(report_path);
}

/// Assert the report has the correct schema version
#[then("the report should have schema perfgate.report.v1")]
async fn then_report_schema(world: &mut PerfgateWorld) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");
    let report: PerfgateReport = serde_json::from_str(&content).expect("Failed to parse report");

    assert_eq!(
        report.report_type, REPORT_SCHEMA_V1,
        "Expected schema '{}', got '{}'",
        REPORT_SCHEMA_V1, report.report_type
    );
}

/// Assert the report verdict matches expected value
#[then(expr = "the report verdict should be {word}")]
async fn then_report_verdict(world: &mut PerfgateWorld, expected: String) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");
    let report: PerfgateReport = serde_json::from_str(&content).expect("Failed to parse report");

    let actual = match report.verdict.status {
        VerdictStatus::Pass => "pass",
        VerdictStatus::Warn => "warn",
        VerdictStatus::Fail => "fail",
        VerdictStatus::Skip => "skip",
    };

    assert_eq!(
        actual,
        expected.to_lowercase(),
        "Expected verdict '{}', got '{}'",
        expected,
        actual
    );
}

/// Assert the report has no findings
#[then("the report should have no findings")]
async fn then_report_no_findings(world: &mut PerfgateWorld) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");
    let report: PerfgateReport = serde_json::from_str(&content).expect("Failed to parse report");

    assert!(
        report.findings.is_empty(),
        "Expected no findings, got {} findings",
        report.findings.len()
    );
}

/// Assert the report has findings with a specific code
#[then(expr = "the report should have findings with code {word}")]
async fn then_report_findings_with_code(world: &mut PerfgateWorld, expected_code: String) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");
    let report: PerfgateReport = serde_json::from_str(&content).expect("Failed to parse report");

    let has_code = report.findings.iter().any(|f| f.code == expected_code);
    assert!(
        has_code,
        "Expected findings with code '{}', got findings: {:?}",
        expected_code,
        report.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );
}

/// Assert the report summary pass count
#[then(expr = "the report summary pass count should be {int}")]
async fn then_report_summary_pass_count(world: &mut PerfgateWorld, expected: u32) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");
    let report: PerfgateReport = serde_json::from_str(&content).expect("Failed to parse report");

    assert_eq!(
        report.summary.pass_count, expected,
        "Expected pass count {}, got {}",
        expected, report.summary.pass_count
    );
}

/// Assert the report summary warn count
#[then(expr = "the report summary warn count should be {int}")]
async fn then_report_summary_warn_count(world: &mut PerfgateWorld, expected: u32) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");
    let report: PerfgateReport = serde_json::from_str(&content).expect("Failed to parse report");

    assert_eq!(
        report.summary.warn_count, expected,
        "Expected warn count {}, got {}",
        expected, report.summary.warn_count
    );
}

/// Assert the report summary fail count
#[then(expr = "the report summary fail count should be {int}")]
async fn then_report_summary_fail_count(world: &mut PerfgateWorld, expected: u32) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");
    let report: PerfgateReport = serde_json::from_str(&content).expect("Failed to parse report");

    assert_eq!(
        report.summary.fail_count, expected,
        "Expected fail count {}, got {}",
        expected, report.summary.fail_count
    );
}

/// Assert both reports are identical (determinism)
#[then("both reports should be identical")]
async fn then_both_reports_identical(world: &mut PerfgateWorld) {
    let report_path1 = world.report_path.as_ref().expect("No report path set");
    let report_path2 = world.report_path2.as_ref().expect("No report path 2 set");

    let content1 = fs::read_to_string(report_path1).expect("Failed to read report 1");
    let content2 = fs::read_to_string(report_path2).expect("Failed to read report 2");

    assert_eq!(
        content1, content2,
        "Reports should be identical for deterministic output"
    );
}

/// Assert the report file exists
#[then("the report file should exist")]
async fn then_report_file_exists(world: &mut PerfgateWorld) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    assert!(
        report_path.exists(),
        "Report file should exist at {:?}",
        report_path
    );
}

/// Assert the markdown file exists
#[then("the markdown file should exist")]
async fn then_md_file_exists(world: &mut PerfgateWorld) {
    let md_path = world.md_output_path.as_ref().expect("No markdown path set");
    assert!(
        md_path.exists(),
        "Markdown file should exist at {:?}",
        md_path
    );
}

/// Assert the report's markdown file contains expected unquoted word (used with report command)
#[then(expr = "the report markdown contains {word}")]
async fn then_report_md_contains_word(world: &mut PerfgateWorld, expected: String) {
    let md_path = world.md_output_path.as_ref().expect("No markdown path set");
    let content = fs::read_to_string(md_path).expect("Failed to read markdown file");

    assert!(
        content.contains(&expected),
        "Expected markdown to contain '{}', got: {}",
        expected,
        content
    );
}

/// Assert the report file contains indented JSON (pretty printed)
#[then("the report file should contain indented JSON")]
async fn then_report_file_is_pretty_printed(world: &mut PerfgateWorld) {
    let report_path = world.report_path.as_ref().expect("No report path set");
    let content = fs::read_to_string(report_path).expect("Failed to read report file");

    // Pretty-printed JSON should contain newlines
    assert!(
        content.contains('\n'),
        "Pretty-printed JSON should contain newlines"
    );

    // Pretty-printed JSON should have indentation
    assert!(
        content.contains("  "),
        "Pretty-printed JSON should have indentation"
    );
}

// ============================================================================
// CHECK COMMAND STEPS
// ============================================================================

/// Create a config file with a bench definition
#[given(expr = "a config file with bench {string}")]
async fn given_config_file_with_bench(world: &mut PerfgateWorld, bench_name: String) {
    world.ensure_temp_dir();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches: vec![BenchConfigFile {
            name: bench_name,
            cwd: None,
            work: None,
            timeout: None,
            command: success_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        }],
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    // Create artifacts directory
    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    // Create baselines directory
    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config file with bench and custom threshold
#[given(expr = "a config file with bench {string} with threshold {float}")]
async fn given_config_file_with_bench_threshold(
    world: &mut PerfgateWorld,
    bench_name: String,
    threshold: f64,
) {
    world.ensure_temp_dir();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(threshold),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches: vec![BenchConfigFile {
            name: bench_name,
            cwd: None,
            work: None,
            timeout: None,
            command: success_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        }],
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    // Create artifacts directory
    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    // Create baselines directory
    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config file with bench, threshold and warn_factor
#[given(expr = "a config file with bench {string} with threshold {float} and warn_factor {float}")]
async fn given_config_file_with_bench_threshold_warn(
    world: &mut PerfgateWorld,
    bench_name: String,
    threshold: f64,
    warn_factor: f64,
) {
    world.ensure_temp_dir();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(threshold),
            warn_factor: Some(warn_factor),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches: vec![BenchConfigFile {
            name: bench_name,
            cwd: None,
            work: None,
            timeout: None,
            command: success_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        }],
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config file with defaults for repeat and warmup
#[given(expr = "a config file with defaults repeat {int} and warmup {int}")]
async fn given_config_file_with_defaults_repeat_warmup(
    world: &mut PerfgateWorld,
    repeat: u32,
    warmup: u32,
) {
    world.ensure_temp_dir();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(repeat),
            warmup: Some(warmup),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches: vec![],
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config file with defaults for repeat only
#[given(expr = "a config file with defaults repeat {int}")]
async fn given_config_file_with_defaults_repeat(world: &mut PerfgateWorld, repeat: u32) {
    world.ensure_temp_dir();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(repeat),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches: vec![],
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Add a bench to the config without explicit repeat or warmup
#[given(expr = "a bench {string} without explicit repeat or warmup")]
async fn given_bench_without_explicit_settings(world: &mut PerfgateWorld, bench_name: String) {
    let config = world.config.as_mut().expect("Config not initialized");

    config.benches.push(BenchConfigFile {
        name: bench_name,
        cwd: None,
        work: None,
        timeout: None,
        command: success_command().iter().map(|s| s.to_string()).collect(),
        repeat: None, // Explicitly not set
        warmup: None, // Explicitly not set
        metrics: None,
        budgets: None,
    });

    // Update the config file
    let config_path = world.config_path.as_ref().expect("Config path not set");
    let toml = toml::to_string_pretty(config).expect("Failed to serialize config");
    fs::write(config_path, toml).expect("Failed to write config file");
}

/// Add a bench to the config with explicit repeat
#[given(expr = "a bench {string} with repeat {int}")]
async fn given_bench_with_repeat(world: &mut PerfgateWorld, bench_name: String, repeat: u32) {
    let config = world.config.as_mut().expect("Config not initialized");

    config.benches.push(BenchConfigFile {
        name: bench_name,
        cwd: None,
        work: None,
        timeout: None,
        command: success_command().iter().map(|s| s.to_string()).collect(),
        repeat: Some(repeat),
        warmup: None,
        metrics: None,
        budgets: None,
    });

    // Update the config file
    let config_path = world.config_path.as_ref().expect("Config path not set");
    let toml = toml::to_string_pretty(config).expect("Failed to serialize config");
    fs::write(config_path, toml).expect("Failed to write config file");
}

/// Create a config file with bench and baseline_dir
#[given(expr = "a config file with bench {string} and baseline_dir {string}")]
async fn given_config_file_with_bench_baseline_dir(
    world: &mut PerfgateWorld,
    bench_name: String,
    baseline_dir: String,
) {
    world.ensure_temp_dir();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some(baseline_dir.clone()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches: vec![BenchConfigFile {
            name: bench_name,
            cwd: None,
            work: None,
            timeout: None,
            command: success_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        }],
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    // Create the custom baselines directory
    let baselines_dir = world.temp_path().join(&baseline_dir);
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config file with bench and baseline_pattern
#[given(expr = "a config file with bench {string} and baseline_pattern {string}")]
async fn given_config_file_with_bench_baseline_pattern(
    world: &mut PerfgateWorld,
    bench_name: String,
    baseline_pattern: String,
) {
    world.ensure_temp_dir();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: None,
            baseline_pattern: Some(baseline_pattern),
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches: vec![BenchConfigFile {
            name: bench_name,
            cwd: None,
            work: None,
            timeout: None,
            command: success_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        }],
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);
}

/// Create a malformed TOML config file (syntax error)
#[given("a malformed TOML config file")]
async fn given_malformed_toml_config(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();

    let config_path = world.temp_path().join("perfgate.toml");
    fs::write(&config_path, "[[bench\n  name = broken").expect("Failed to write config file");
    world.config_path = Some(config_path);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);
}

/// Create a config file with a bench missing the required command field
#[given(expr = "a config file with bench {string} missing the command field")]
async fn given_config_missing_command(world: &mut PerfgateWorld, bench_name: String) {
    world.ensure_temp_dir();

    let raw = format!("[[bench]]\nname = \"{}\"\n", bench_name);

    let config_path = world.temp_path().join("perfgate.toml");
    fs::write(&config_path, raw).expect("Failed to write config file");
    world.config_path = Some(config_path);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);
}

/// Create a config file with an invalid threshold type (string instead of float)
#[given("a config file with an invalid threshold type")]
async fn given_config_invalid_threshold(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();

    let raw = concat!(
        "[defaults]\n",
        "threshold = \"not_a_number\"\n",
        "\n",
        "[[bench]]\n",
        "name = \"bad-thresh\"\n",
        "command = [\"echo\", \"hello\"]\n",
    );

    let config_path = world.temp_path().join("perfgate.toml");
    fs::write(&config_path, raw).expect("Failed to write config file");
    world.config_path = Some(config_path);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);
}

/// Create a config file with no benchmarks defined
#[given("a config file with no benchmarks defined")]
async fn given_config_no_benchmarks(world: &mut PerfgateWorld) {
    world.ensure_temp_dir();

    let raw = "[defaults]\nrepeat = 1\n";

    let config_path = world.temp_path().join("perfgate.toml");
    fs::write(&config_path, raw).expect("Failed to write config file");
    world.config_path = Some(config_path);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);
}

/// Create a config file with an invalid metric name in budgets
#[given(expr = "a config file with bench {string} and invalid metric in budgets")]
async fn given_config_invalid_metric(world: &mut PerfgateWorld, bench_name: String) {
    world.ensure_temp_dir();

    let raw = format!(
        "[[bench]]\nname = \"{}\"\ncommand = [\"echo\", \"hello\"]\n\n[bench.budgets.invalid_metric]\nthreshold = 0.1\n",
        bench_name
    );

    let config_path = world.temp_path().join("perfgate.toml");
    fs::write(&config_path, raw).expect("Failed to write config file");
    world.config_path = Some(config_path);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);
}

/// Create a baseline receipt for a specific bench
#[given(expr = "a baseline receipt for bench {string} with wall_ms median of {int}")]
async fn given_baseline_receipt_for_bench(
    world: &mut PerfgateWorld,
    bench_name: String,
    wall_ms: u64,
) {
    world.ensure_temp_dir();

    // Create the baseline receipt
    let mut receipt = world.create_run_receipt(wall_ms);
    receipt.bench.name = bench_name.clone();

    // Save to baselines directory
    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");

    let baseline_path = baselines_dir.join(format!("{}.json", bench_name));
    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize baseline");
    fs::write(&baseline_path, json).expect("Failed to write baseline receipt");
    world.baseline_path = Some(baseline_path);
}

/// Create a baseline receipt at a specific path
#[given(expr = "a baseline receipt at {string} with wall_ms median of {int}")]
async fn given_baseline_receipt_at_path(world: &mut PerfgateWorld, path: String, wall_ms: u64) {
    world.ensure_temp_dir();

    let mut receipt = world.create_run_receipt(wall_ms);
    receipt.bench.name = "test-bench".to_string();

    // Create parent directories and save
    let full_path = world.temp_path().join(&path);
    if let Some(parent) = full_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent directories");
    }

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize baseline");
    fs::write(&full_path, json).expect("Failed to write baseline receipt");
    world.baseline_path = Some(full_path);
}

/// Placeholder step for expected current run values (not actually implemented - scenario is skipped)
#[given(expr = "a current run would have wall_ms median of {int}")]
async fn given_current_run_would_have(_world: &mut PerfgateWorld, _wall_ms: u64) {
    // This is a placeholder - actual performance depends on the command being run
    // In real tests, we can't control what the run produces
}

/// Create a config file with multiple bench definitions
#[given(expr = "a config file with benches {string}")]
async fn given_config_file_with_benches(world: &mut PerfgateWorld, bench_names_str: String) {
    world.ensure_temp_dir();

    let bench_names: Vec<&str> = bench_names_str.split(',').map(|s| s.trim()).collect();
    let benches = bench_names
        .iter()
        .map(|name| BenchConfigFile {
            name: name.to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: success_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        })
        .collect();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches,
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config with multiple benches and tight threshold (0.0) so any regression fails
#[given(expr = "a config file with benches {string} and tight threshold")]
async fn given_config_file_with_benches_tight(world: &mut PerfgateWorld, bench_names_str: String) {
    world.ensure_temp_dir();

    let bench_names: Vec<&str> = bench_names_str.split(',').map(|s| s.trim()).collect();
    let benches = bench_names
        .iter()
        .map(|name| BenchConfigFile {
            name: name.to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: slow_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        })
        .collect();

    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.0),
            warn_factor: Some(0.0),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches,
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config with multiple benches and lenient threshold so regressions only warn
#[given(expr = "a config file with benches {string} and lenient threshold")]
async fn given_config_file_with_benches_lenient(
    world: &mut PerfgateWorld,
    bench_names_str: String,
) {
    world.ensure_temp_dir();

    let bench_names: Vec<&str> = bench_names_str.split(',').map(|s| s.trim()).collect();
    let benches = bench_names
        .iter()
        .map(|name| BenchConfigFile {
            name: name.to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: slow_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        })
        .collect();

    // threshold=100000.0 (very lenient, no fail), warn_factor=0.0 (warn at any regression)
    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(100_000.0),
            warn_factor: Some(0.0),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches,
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Create a config with a tight-threshold bench and a lenient-threshold bench (mixed)
#[given(
    expr = "a config file with benches {string} and tight threshold and bench {string} with lenient threshold"
)]
async fn given_config_file_with_mixed_thresholds(
    world: &mut PerfgateWorld,
    tight_names_str: String,
    lenient_name: String,
) {
    world.ensure_temp_dir();

    let tight_names: Vec<&str> = tight_names_str.split(',').map(|s| s.trim()).collect();
    let mut benches: Vec<BenchConfigFile> = tight_names
        .iter()
        .map(|name| BenchConfigFile {
            name: name.to_string(),
            cwd: None,
            work: None,
            timeout: None,
            command: slow_command().iter().map(|s| s.to_string()).collect(),
            repeat: None,
            warmup: None,
            metrics: None,
            budgets: None,
        })
        .collect();

    // Lenient bench: override threshold to 100000.0 so it only warns, never fails
    let mut lenient_budgets = BTreeMap::new();
    lenient_budgets.insert(
        Metric::WallMs,
        BudgetOverride {
            noise_threshold: None,
            noise_policy: None,
            threshold: Some(100_000.0),
            direction: None,
            warn_factor: Some(0.0),
            statistic: None,
        },
    );
    benches.push(BenchConfigFile {
        name: lenient_name,
        cwd: None,
        work: None,
        timeout: None,
        command: slow_command().iter().map(|s| s.to_string()).collect(),
        repeat: None,
        warmup: None,
        metrics: None,
        budgets: Some(lenient_budgets),
    });

    // Default threshold=0.0 makes regressions fail unless overridden
    let config = ConfigFile {
        defaults: DefaultsConfig {
            noise_threshold: None,
            noise_policy: None,
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.0),
            warn_factor: Some(0.0),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
            baseline_pattern: None,
            markdown_template: None,
        },
        baseline_server: BaselineServerConfig::default(),
        benches,
    };

    let config_path = world.temp_path().join("perfgate.toml");
    let toml = toml::to_string_pretty(&config).expect("Failed to serialize config");
    fs::write(&config_path, toml).expect("Failed to write config file");
    world.config_path = Some(config_path);
    world.config = Some(config);

    let artifacts_dir = world.temp_path().join("artifacts");
    fs::create_dir_all(&artifacts_dir).expect("Failed to create artifacts dir");
    world.artifacts_dir = Some(artifacts_dir);

    let baselines_dir = world.temp_path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
}

/// Run perfgate check for all benches (--all)
#[when("I run perfgate check for all benches")]
async fn when_check_all_benches(world: &mut PerfgateWorld) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .current_dir(world.temp_path());

    let output = cmd
        .output()
        .expect("Failed to execute perfgate check --all");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check for all benches with --fail-on-warn
#[when("I run perfgate check for all benches with --fail-on-warn")]
async fn when_check_all_benches_with_fail_on_warn(world: &mut PerfgateWorld) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--fail-on-warn")
        .current_dir(world.temp_path());

    let output = cmd
        .output()
        .expect("Failed to execute perfgate check --all --fail-on-warn");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check for a specific bench
#[when(expr = "I run perfgate check for bench {string}")]
async fn when_check_for_bench(world: &mut PerfgateWorld, bench_name: String) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg(&bench_name)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .current_dir(world.temp_path());

    for arg in &world.extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check with --require-baseline
#[when(expr = "I run perfgate check for bench {string} with --require-baseline")]
async fn when_check_with_require_baseline(world: &mut PerfgateWorld, bench_name: String) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg(&bench_name)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--require-baseline")
        .current_dir(world.temp_path());

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check with --fail-on-warn
#[when(expr = "I run perfgate check for bench {string} with --fail-on-warn")]
async fn when_check_with_fail_on_warn(world: &mut PerfgateWorld, bench_name: String) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg(&bench_name)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--fail-on-warn")
        .current_dir(world.temp_path());

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check with explicit --baseline path
#[when(expr = "I run perfgate check for bench {string} with --baseline {string}")]
async fn when_check_with_baseline_path(
    world: &mut PerfgateWorld,
    bench_name: String,
    baseline_path: String,
) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");
    let full_baseline_path = world.temp_path().join(&baseline_path);

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg(&bench_name)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--baseline")
        .arg(&full_baseline_path)
        .current_dir(world.temp_path());

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check with --output-github
#[when(expr = "I run perfgate check for bench {string} with --output-github")]
async fn when_check_with_output_github(world: &mut PerfgateWorld, bench_name: String) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");
    let github_output_path = world.temp_path().join("github_output.txt");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg(&bench_name)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--output-github")
        .current_dir(world.temp_path())
        .env("GITHUB_OUTPUT", &github_output_path);

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
    world.github_output_path = Some(github_output_path);
}

/// Run perfgate check in cockpit mode for a specific bench
#[when(expr = "I run perfgate check for bench {string} in cockpit mode")]
async fn when_check_for_bench_in_cockpit_mode(world: &mut PerfgateWorld, bench_name: String) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg(&bench_name)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--mode")
        .arg("cockpit")
        .current_dir(world.temp_path());

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check for all benches in cockpit mode
#[when("I run perfgate check for all benches in cockpit mode")]
async fn when_check_all_in_cockpit_mode(world: &mut PerfgateWorld) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--mode")
        .arg("cockpit")
        .current_dir(world.temp_path());

    let output = cmd
        .output()
        .expect("Failed to execute perfgate check --all --mode cockpit");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check in cockpit mode with --require-baseline
#[when(expr = "I run perfgate check for bench {string} in cockpit mode with --require-baseline")]
async fn when_check_for_bench_in_cockpit_mode_require_baseline(
    world: &mut PerfgateWorld,
    bench_name: String,
) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg(&bench_name)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .arg("--mode")
        .arg("cockpit")
        .arg("--require-baseline")
        .current_dir(world.temp_path());

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Run perfgate check for all benches with --bench-regex
#[when(expr = "I run perfgate check for all benches with --bench-regex {string}")]
async fn when_check_all_with_bench_regex(world: &mut PerfgateWorld, regex: String) {
    let config_path = world.config_path.clone().expect("Config path not set");
    let artifacts_dir = world.artifacts_dir.clone().expect("Artifacts dir not set");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--bench-regex")
        .arg(&regex)
        .arg("--out-dir")
        .arg(&artifacts_dir)
        .current_dir(world.temp_path());

    let output = cmd.output().expect("Failed to execute perfgate check");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

/// Assert the run.json artifact exists
#[then("the run.json artifact should exist")]
async fn then_run_json_artifact_exists(world: &mut PerfgateWorld) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let run_path = artifacts_dir.join("run.json");
    assert!(run_path.exists(), "run.json should exist at {:?}", run_path);
}

/// Assert the compare.json artifact exists
#[then("the compare.json artifact should exist")]
async fn then_compare_json_artifact_exists(world: &mut PerfgateWorld) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let compare_path = artifacts_dir.join("compare.json");
    assert!(
        compare_path.exists(),
        "compare.json should exist at {:?}",
        compare_path
    );
}

/// Assert the compare.json artifact does not exist
#[then("the compare.json artifact should not exist")]
async fn then_compare_json_artifact_not_exists(world: &mut PerfgateWorld) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let compare_path = artifacts_dir.join("compare.json");
    assert!(
        !compare_path.exists(),
        "compare.json should not exist at {:?}",
        compare_path
    );
}

/// Assert the report.json artifact exists
#[then("the report.json artifact should exist")]
async fn then_report_json_artifact_exists(world: &mut PerfgateWorld) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let report_path = artifacts_dir.join("report.json");
    assert!(
        report_path.exists(),
        "report.json should exist at {:?}",
        report_path
    );
}

/// Assert the comment.md artifact exists
#[then("the comment.md artifact should exist")]
async fn then_comment_md_artifact_exists(world: &mut PerfgateWorld) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let comment_path = artifacts_dir.join("comment.md");
    assert!(
        comment_path.exists(),
        "comment.md should exist at {:?}",
        comment_path
    );
}

/// Assert the comment.md contains expected text
#[then(expr = "the comment.md should contain {string}")]
async fn then_comment_md_contains(world: &mut PerfgateWorld, expected: String) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let comment_path = artifacts_dir.join("comment.md");
    let content = fs::read_to_string(&comment_path).expect("Failed to read comment.md");

    assert!(
        content.to_lowercase().contains(&expected.to_lowercase()),
        "Expected comment.md to contain '{}', got: {}",
        expected,
        content
    );
}

/// Assert report.json has sensor.report.v1 schema (cockpit mode)
#[then("the report.json artifact should have schema sensor.report.v1")]
async fn then_report_json_artifact_has_sensor_schema(world: &mut PerfgateWorld) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let report_path = artifacts_dir.join("report.json");
    let content = fs::read_to_string(&report_path).expect("Failed to read report.json");
    let report: serde_json::Value =
        serde_json::from_str(&content).expect("Failed to parse report.json");

    assert_eq!(
        report["schema"].as_str(),
        Some("sensor.report.v1"),
        "Expected report schema sensor.report.v1, got: {}",
        content
    );
}

/// Assert an artifact file exists at a path relative to artifacts directory
#[then(expr = "the artifact file {string} should exist")]
async fn then_artifact_file_exists(world: &mut PerfgateWorld, relative_path: String) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let full_path = artifacts_dir.join(PathBuf::from(relative_path));
    assert!(
        full_path.exists(),
        "Artifact should exist at {:?}",
        full_path
    );
}

/// Assert an artifact file does NOT exist at a path relative to artifacts directory
#[then(expr = "the artifact file {string} should not exist")]
async fn then_artifact_file_not_exists(world: &mut PerfgateWorld, relative_path: String) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let full_path = artifacts_dir.join(PathBuf::from(relative_path));
    assert!(
        !full_path.exists(),
        "Artifact should NOT exist at {:?}",
        full_path
    );
}

/// Assert the GitHub output file exists
#[then("the github output file should exist")]
async fn then_github_output_file_exists(world: &mut PerfgateWorld) {
    let output_path = world
        .github_output_path
        .as_ref()
        .expect("GitHub output path not set");
    assert!(
        output_path.exists(),
        "GITHUB_OUTPUT file should exist at {:?}",
        output_path
    );
}

/// Assert the GitHub output file contains expected text
#[then(expr = "the github output should contain {string}")]
async fn then_github_output_contains(world: &mut PerfgateWorld, expected: String) {
    let output_path = world
        .github_output_path
        .as_ref()
        .expect("GitHub output path not set");
    let content = fs::read_to_string(output_path).expect("Failed to read GITHUB_OUTPUT");

    assert!(
        content.contains(&expected),
        "Expected GITHUB_OUTPUT to contain '{}', got: {}",
        expected,
        content
    );
}

/// Assert the run.json has a specific number of samples
#[then(expr = "the run.json should have {int} samples")]
async fn then_run_json_has_samples(world: &mut PerfgateWorld, expected: usize) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let run_path = artifacts_dir.join("run.json");
    let content = fs::read_to_string(&run_path).expect("Failed to read run.json");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run.json");

    assert_eq!(
        receipt.samples.len(),
        expected,
        "Expected {} samples, got {}",
        expected,
        receipt.samples.len()
    );
}

/// Assert the run.json has a specific number of warmup samples
#[then(expr = "the run.json should have {int} warmup samples")]
async fn then_run_json_has_warmup_samples(world: &mut PerfgateWorld, expected: usize) {
    let artifacts_dir = world.artifacts_dir.as_ref().expect("Artifacts dir not set");
    let run_path = artifacts_dir.join("run.json");
    let content = fs::read_to_string(&run_path).expect("Failed to read run.json");
    let receipt: RunReceipt = serde_json::from_str(&content).expect("Failed to parse run.json");

    let warmup_count = receipt.samples.iter().filter(|s| s.warmup).count();
    assert_eq!(
        warmup_count, expected,
        "Expected {} warmup samples, got {}",
        expected, warmup_count
    );
}

// ============================================================================
// MICROCRATE INTEGRATION STEPS
// ============================================================================

/// Background: verify perfgate is installed
#[given("a working perfgate installation")]
async fn given_working_perfgate_installation(_world: &mut PerfgateWorld) {
    // Perfgate is built as part of the test suite
}

// SHA-256 steps

#[when(expr = "I compute SHA-256 of {string}")]
async fn when_compute_sha256(world: &mut PerfgateWorld, input: String) {
    world.computed_hash = Some(sha256_hex(input.as_bytes()));
}

#[then(expr = "the hash should be {string}")]
async fn then_hash_should_be(world: &mut PerfgateWorld, expected: String) {
    let hash = world.computed_hash.as_ref().expect("Hash not computed");
    assert_eq!(
        hash, &expected,
        "Expected hash '{}', got '{}'",
        expected, hash
    );
}

// Stats steps

#[given(expr = "a list of values {string}")]
async fn given_list_of_values(world: &mut PerfgateWorld, values_str: String) {
    let values: Vec<u64> = values_str
        .split(',')
        .map(|s| s.trim().parse().expect("Failed to parse value"))
        .collect();
    world.stats_values = values;
}

#[when("I compute the median")]
async fn when_compute_median(world: &mut PerfgateWorld) {
    let summary = summarize_u64(&world.stats_values).expect("Failed to compute stats");
    world.computed_median = Some(summary.median);
}

#[then(expr = "the median should be {int}")]
async fn then_median_should_be(world: &mut PerfgateWorld, expected: u64) {
    let median = world.computed_median.expect("Median not computed");
    assert_eq!(
        median, expected,
        "Expected median {}, got {}",
        expected, median
    );
}

// Validation steps

#[when(expr = "I validate bench name {string}")]
async fn when_validate_bench_name(world: &mut PerfgateWorld, name: String) {
    world.validation_result = Some(validate_bench_name_fn(&name));
}

#[then("the validation should pass")]
async fn then_validation_should_pass(world: &mut PerfgateWorld) {
    let result = world
        .validation_result
        .as_ref()
        .expect("Validation not performed");
    assert!(
        result.is_ok(),
        "Expected validation to pass, but got error: {:?}",
        result
    );
}

#[then(expr = "the validation should fail with {string}")]
async fn then_validation_should_fail_with(world: &mut PerfgateWorld, expected_hint: String) {
    let result = world
        .validation_result
        .as_ref()
        .expect("Validation not performed");
    assert!(
        result.is_err(),
        "Expected validation to fail, but it passed"
    );
    let err = result.as_ref().unwrap_err();
    let err_str = err.to_string().to_lowercase();
    assert!(
        err_str.contains(&expected_hint.to_lowercase()),
        "Expected error to contain '{}', got: {}",
        expected_hint,
        err_str
    );
}

// Host detect steps

#[given(expr = "baseline host with os {string}")]
async fn given_baseline_host_os(world: &mut PerfgateWorld, os: String) {
    world.baseline_host = Some(Box::new(HostInfo {
        os,
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    }));
}

#[given(expr = "baseline host with cpu_count {int}")]
async fn given_baseline_host_cpu(world: &mut PerfgateWorld, cpu_count: u32) {
    let mut host = world.baseline_host.take().unwrap_or_else(|| {
        Box::new(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        })
    });
    host.cpu_count = Some(cpu_count);
    world.baseline_host = Some(host);
}

#[given(expr = "current host with os {string}")]
async fn given_current_host_os(world: &mut PerfgateWorld, os: String) {
    world.current_host = Some(Box::new(HostInfo {
        os,
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    }));
}

#[given(expr = "current host with cpu_count {int}")]
async fn given_current_host_cpu(world: &mut PerfgateWorld, cpu_count: u32) {
    let mut host = world.current_host.take().unwrap_or_else(|| {
        Box::new(HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        })
    });
    host.cpu_count = Some(cpu_count);
    world.current_host = Some(host);
}

#[when("I detect host mismatch")]
async fn when_detect_host_mismatch(world: &mut PerfgateWorld) {
    let baseline = world.baseline_host.as_ref().expect("Baseline host not set");
    let current = world.current_host.as_ref().expect("Current host not set");
    world.host_mismatch = detect_host_mismatch(baseline, current).map(Box::new);
}

#[then("a mismatch should be detected")]
async fn then_mismatch_detected(world: &mut PerfgateWorld) {
    assert!(
        world.host_mismatch.is_some(),
        "Expected host mismatch to be detected"
    );
}

#[then("no mismatch should be detected")]
async fn then_no_mismatch_detected(world: &mut PerfgateWorld) {
    assert!(
        world.host_mismatch.is_none(),
        "Expected no host mismatch, but got: {:?}",
        world.host_mismatch
    );
}

#[then(expr = "the reason should contain {string}")]
async fn then_reason_should_contain(world: &mut PerfgateWorld, expected: String) {
    let mismatch = world.host_mismatch.as_ref().expect("No mismatch detected");
    assert!(
        mismatch.reasons.iter().any(|r| r.contains(&expected)),
        "Expected reason to contain '{}', got: {:?}",
        expected,
        mismatch.reasons
    );
}

// Export steps

#[given(expr = "a run receipt for bench {string}")]
async fn given_run_receipt_for_export(world: &mut PerfgateWorld, bench_name: String) {
    world.test_run_receipt = Some(Box::new(RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        run: RunMeta {
            id: "test-run-001".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            ended_at: "2024-01-15T10:00:05Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: bench_name,
            cwd: None,
            command: vec!["echo".to_string(), "hello".to_string()],
            repeat: 5,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples: vec![Sample {
            wall_ms: 100,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            binary_bytes: None,
            stdout: None,
            stderr: None,
        }],
        stats: Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            binary_bytes: None,
            throughput_per_s: None,
        },
    }));
}

#[when("I export to CSV format")]
async fn when_export_to_csv(world: &mut PerfgateWorld) {
    let receipt = world
        .test_run_receipt
        .as_ref()
        .expect("Run receipt not set");
    world.exported_output =
        Some(ExportUseCase::export_run(receipt, ExportFormat::Csv).expect("Failed to export"));
}

#[then("the output should be valid CSV")]
async fn then_output_valid_csv(world: &mut PerfgateWorld) {
    let output = world.exported_output.as_ref().expect("Output not set");
    let lines: Vec<&str> = output.trim().lines().collect();
    assert!(
        lines.len() >= 2,
        "CSV should have at least header and one data row"
    );
}

#[then(expr = "the header should contain {string}")]
async fn then_header_should_contain(world: &mut PerfgateWorld, expected: String) {
    let output = world.exported_output.as_ref().expect("Output not set");
    let header = output.lines().next().expect("No header line");
    assert!(
        header.contains(&expected),
        "Expected header to contain '{}', got: {}",
        expected,
        header
    );
}

// Render steps

#[given(expr = "a compare receipt with status {string}")]
async fn given_compare_receipt_for_render(world: &mut PerfgateWorld, status: String) {
    let verdict_status = match status.as_str() {
        "pass" => VerdictStatus::Pass,
        "warn" => VerdictStatus::Warn,
        "fail" => VerdictStatus::Fail,
        _ => panic!("Invalid status: {}", status),
    };

    let metric_status = match verdict_status {
        VerdictStatus::Pass => MetricStatus::Pass,
        VerdictStatus::Warn => MetricStatus::Warn,
        VerdictStatus::Fail => MetricStatus::Fail,
        VerdictStatus::Skip => MetricStatus::Skip,
    };

    let mut deltas = BTreeMap::new();
    deltas.insert(
        Metric::WallMs,
        Delta {
            baseline: 100.0,
            current: 150.0,
            ratio: 1.5,
            pct: 0.5,
            regression: 0.5,
            cv: None,
            noise_threshold: None,
            statistic: MetricStatistic::Median,
            significance: None,
            status: metric_status,
        },
    );

    world.test_compare_receipt = Some(Box::new(CompareReceipt {
        schema: COMPARE_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        bench: BenchMeta {
            name: "test-bench".to_string(),
            cwd: None,
            command: vec!["true".to_string()],
            repeat: 1,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        baseline_ref: CompareRef {
            path: None,
            run_id: None,
        },
        current_ref: CompareRef {
            path: None,
            run_id: None,
        },
        budgets: BTreeMap::new(),
        deltas,
        verdict: Verdict {
            status: verdict_status,
            counts: VerdictCounts {
                pass: if verdict_status == VerdictStatus::Pass {
                    1
                } else {
                    0
                },
                warn: if verdict_status == VerdictStatus::Warn {
                    1
                } else {
                    0
                },
                fail: if verdict_status == VerdictStatus::Fail {
                    1
                } else {
                    0
                },
                skip: if verdict_status == VerdictStatus::Skip {
                    1
                } else {
                    0
                },
            },
            reasons: vec![],
        },
    }));
}

#[when("I render markdown")]
async fn when_render_markdown(world: &mut PerfgateWorld) {
    let compare = world
        .test_compare_receipt
        .as_ref()
        .expect("Compare receipt not set");
    world.rendered_markdown = Some(render_markdown(compare));
}

#[then(expr = "the output should contain {string}")]
async fn then_output_should_contain(world: &mut PerfgateWorld, expected: String) {
    let markdown = world
        .rendered_markdown
        .as_ref()
        .expect("Markdown not rendered");
    assert!(
        markdown.contains(&expected),
        "Expected output to contain '{}', got:\n{}",
        expected,
        markdown
    );
}

#[then("the output should contain a markdown table")]
async fn then_output_should_contain_table(world: &mut PerfgateWorld) {
    let markdown = world
        .rendered_markdown
        .as_ref()
        .expect("Markdown not rendered");
    assert!(
        markdown.contains("| metric |"),
        "Expected markdown table, got:\n{}",
        markdown
    );
}

// Sensor steps

#[given(expr = "a perfgate report with status {string}")]
async fn given_perfgate_report(world: &mut PerfgateWorld, status: String) {
    let verdict_status = match status.as_str() {
        "pass" => VerdictStatus::Pass,
        "warn" => VerdictStatus::Warn,
        "fail" => VerdictStatus::Fail,
        _ => panic!("Invalid status: {}", status),
    };

    world.test_perfgate_report = Some(Box::new(PerfgateReport {
        report_type: REPORT_SCHEMA_V1.to_string(),
        verdict: Verdict {
            status: verdict_status,
            counts: VerdictCounts {
                pass: if verdict_status == VerdictStatus::Pass {
                    2
                } else {
                    0
                },
                warn: if verdict_status == VerdictStatus::Warn {
                    1
                } else {
                    0
                },
                fail: if verdict_status == VerdictStatus::Fail {
                    1
                } else {
                    0
                },
                skip: if verdict_status == VerdictStatus::Skip {
                    1
                } else {
                    0
                },
            },
            reasons: vec![],
        },
        compare: None,
        findings: vec![],
        summary: ReportSummary {
            pass_count: if verdict_status == VerdictStatus::Pass {
                2
            } else {
                0
            },
            warn_count: if verdict_status == VerdictStatus::Warn {
                1
            } else {
                0
            },
            fail_count: if verdict_status == VerdictStatus::Fail {
                1
            } else {
                0
            },
            skip_count: if verdict_status == VerdictStatus::Skip {
                1
            } else {
                0
            },
            total_count: 4,
        },
    }));
}

#[when("I build a sensor report")]
async fn when_build_sensor_report(world: &mut PerfgateWorld) {
    let report = world
        .test_perfgate_report
        .as_ref()
        .expect("Perfgate report not set");
    let tool = ToolInfo {
        name: "perfgate".to_string(),
        version: "0.1.0".to_string(),
    };

    world.sensor_report = Some(Box::new(
        SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
            .baseline(true, None)
            .build(report),
    ));
}

#[then(expr = "the sensor report should have schema {string}")]
async fn then_sensor_report_schema(world: &mut PerfgateWorld, expected: String) {
    let sensor_report = world
        .sensor_report
        .as_ref()
        .expect("Sensor report not built");
    assert_eq!(
        sensor_report.schema, expected,
        "Expected schema '{}', got '{}'",
        expected, sensor_report.schema
    );
}

#[then(expr = "the verdict status should be {string}")]
async fn then_verdict_status_should_be(world: &mut PerfgateWorld, expected: String) {
    let sensor_report = world
        .sensor_report
        .as_ref()
        .expect("Sensor report not built");
    let actual = match sensor_report.verdict.status {
        perfgate_types::SensorVerdictStatus::Pass => "pass",
        perfgate_types::SensorVerdictStatus::Warn => "warn",
        perfgate_types::SensorVerdictStatus::Fail => "fail",
        perfgate_types::SensorVerdictStatus::Skip => "skip",
    };
    assert_eq!(
        actual, expected,
        "Expected verdict status '{}', got '{}'",
        expected, actual
    );
}

// ============================================================================
// ERROR MICROCRATE STEPS
// ============================================================================

#[when("I convert ValidationError::Empty to PerfgateError")]
async fn when_convert_validation_error_empty(world: &mut PerfgateWorld) {
    world.perfgate_error = Some(ValidationError::Empty.into());
}

#[when("I convert StatsError::NoSamples to PerfgateError")]
async fn when_convert_stats_error_no_samples(world: &mut PerfgateWorld) {
    world.perfgate_error = Some(StatsError::NoSamples.into());
}

#[when("I convert AdapterError::Timeout to PerfgateError")]
async fn when_convert_adapter_error_timeout(world: &mut PerfgateWorld) {
    world.perfgate_error = Some(AdapterError::Timeout.into());
}

#[when("I convert ConfigValidationError::BenchName to PerfgateError")]
async fn when_convert_config_error_bench_name(world: &mut PerfgateWorld) {
    world.perfgate_error = Some(ConfigValidationError::BenchName("test error".to_string()).into());
}

#[when("I convert IoError::BaselineResolve to PerfgateError")]
async fn when_convert_io_error_baseline_resolve(world: &mut PerfgateWorld) {
    world.perfgate_error = Some(IoError::BaselineResolve("file not found".to_string()).into());
}

#[when("I convert PairedError::NoSamples to PerfgateError")]
async fn when_convert_paired_error_no_samples(world: &mut PerfgateWorld) {
    world.perfgate_error = Some(PairedError::NoSamples.into());
}

#[when("I convert std::io::Error to PerfgateError")]
async fn when_convert_std_io_error(world: &mut PerfgateWorld) {
    let std_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    world.perfgate_error = Some(PerfgateError::Io(perfgate_error::IoError::from(std_err)));
}

#[then(expr = "the error category should be {string}")]
async fn then_error_category_should_be(world: &mut PerfgateWorld, expected: String) {
    let err = world.perfgate_error.as_ref().expect("Error not set");
    let category = err.category();
    assert_eq!(
        category.as_str(),
        expected,
        "Expected category '{}', got '{}'",
        expected,
        category.as_str()
    );
}

#[then("the error should be recoverable")]
async fn then_error_should_be_recoverable(world: &mut PerfgateWorld) {
    let err = world.perfgate_error.as_ref().expect("Error not set");
    assert!(
        err.is_recoverable(),
        "Expected error to be recoverable, but it is not"
    );
}

#[then("the error should not be recoverable")]
async fn then_error_should_not_be_recoverable(world: &mut PerfgateWorld) {
    let err = world.perfgate_error.as_ref().expect("Error not set");
    assert!(
        !err.is_recoverable(),
        "Expected error to not be recoverable, but it is"
    );
}

#[then("the error exit code should be positive")]
async fn then_error_exit_code_should_be_positive(world: &mut PerfgateWorld) {
    let err = world.perfgate_error.as_ref().expect("Error not set");
    let exit_code = err.exit_code();
    assert!(
        exit_code > 0,
        "Expected exit code to be positive, got {}",
        exit_code
    );
}

#[then(expr = "the error exit code should be {int}")]
async fn then_error_exit_code_value(world: &mut PerfgateWorld, expected: i32) {
    let err = world.perfgate_error.as_ref().expect("Error not set");
    let exit_code = err.exit_code();
    assert_eq!(
        exit_code, expected,
        "Expected exit code {}, got {}",
        expected, exit_code
    );
}

// ============================================================================
// BUDGET MICROCRATE STEPS
// ============================================================================

#[given(expr = "a budget with threshold {float} and warn_threshold {float} for Direction::Lower")]
async fn given_budget_direction_lower(
    world: &mut PerfgateWorld,
    threshold: f64,
    warn_threshold: f64,
) {
    world.test_budget = Some(perfgate_types::Budget {
        noise_threshold: None,
        noise_policy: perfgate_types::NoisePolicy::Ignore,
        threshold,
        warn_threshold,
        direction: perfgate_types::Direction::Lower,
    });
}

#[given(expr = "a budget with threshold {float} and warn_threshold {float} for Direction::Higher")]
async fn given_budget_direction_higher(
    world: &mut PerfgateWorld,
    threshold: f64,
    warn_threshold: f64,
) {
    world.test_budget = Some(perfgate_types::Budget {
        noise_threshold: None,
        noise_policy: perfgate_types::NoisePolicy::Ignore,
        threshold,
        warn_threshold,
        direction: perfgate_types::Direction::Higher,
    });
}

#[when(expr = "I evaluate budget with baseline {float} and current {float}")]
async fn when_evaluate_budget(world: &mut PerfgateWorld, baseline: f64, current: f64) {
    let budget = world.test_budget.as_ref().expect("Budget not set");
    match evaluate_budget(baseline, current, budget, None) {
        Ok(result) => {
            world.budget_result = Some(result);
            world.budget_error = None;
        }
        Err(e) => {
            world.budget_result = None;
            world.budget_error = Some(e.to_string());
        }
    }
}

#[then(expr = "the budget status should be {string}")]
async fn then_budget_status_should_be(world: &mut PerfgateWorld, expected: String) {
    let result = world.budget_result.as_ref().expect("Budget result not set");
    let actual = match result.status {
        perfgate_types::MetricStatus::Pass => "pass",
        perfgate_types::MetricStatus::Warn => "warn",
        perfgate_types::MetricStatus::Fail => "fail",
        perfgate_types::MetricStatus::Skip => "skip",
    };
    assert_eq!(
        actual, expected,
        "Expected budget status '{}', got '{}'",
        expected, actual
    );
}

#[then(expr = "the regression should be {float}")]
async fn then_regression_should_be(world: &mut PerfgateWorld, expected: f64) {
    let result = world.budget_result.as_ref().expect("Budget result not set");
    assert!(
        (result.regression - expected).abs() < 1e-10,
        "Expected regression {}, got {}",
        expected,
        result.regression
    );
}

#[then(expr = "the budget evaluation should fail with {string}")]
async fn then_budget_evaluation_should_fail(world: &mut PerfgateWorld, expected_hint: String) {
    let error = world
        .budget_error
        .as_ref()
        .expect("Expected budget evaluation to fail");
    assert!(
        error.contains(&expected_hint),
        "Expected error to contain '{}', got: {}",
        expected_hint,
        error
    );
}

#[given(expr = "budget statuses {string}")]
async fn given_budget_statuses(world: &mut PerfgateWorld, statuses_str: String) {
    let statuses: Vec<perfgate_types::MetricStatus> = statuses_str
        .split(',')
        .map(|s| match s.trim() {
            "pass" => perfgate_types::MetricStatus::Pass,
            "warn" => perfgate_types::MetricStatus::Warn,
            "fail" => perfgate_types::MetricStatus::Fail,
            _ => panic!("Invalid status: {}", s),
        })
        .collect();
    world.budget_statuses = statuses;
}

#[when("I aggregate the verdict")]
async fn when_aggregate_verdict(world: &mut PerfgateWorld) {
    world.aggregated_verdict = Some(aggregate_verdict(&world.budget_statuses));
}

#[then(expr = "the aggregated verdict should be {string}")]
async fn then_aggregated_verdict_should_be(world: &mut PerfgateWorld, expected: String) {
    let verdict = world
        .aggregated_verdict
        .as_ref()
        .expect("Verdict not aggregated");
    let actual = match verdict.status {
        perfgate_types::VerdictStatus::Pass => "pass",
        perfgate_types::VerdictStatus::Warn => "warn",
        perfgate_types::VerdictStatus::Fail => "fail",
        perfgate_types::VerdictStatus::Skip => "skip",
    };
    assert_eq!(
        actual, expected,
        "Expected verdict '{}', got '{}'",
        expected, actual
    );
}

#[then(expr = "the verdict counts should be pass {int}, warn {int}, fail {int}")]
async fn then_verdict_counts(world: &mut PerfgateWorld, pass: u32, warn: u32, fail: u32) {
    let verdict = world
        .aggregated_verdict
        .as_ref()
        .expect("Verdict not aggregated");
    assert_eq!(
        verdict.counts.pass, pass,
        "Expected {} pass, got {}",
        pass, verdict.counts.pass
    );
    assert_eq!(
        verdict.counts.warn, warn,
        "Expected {} warn, got {}",
        warn, verdict.counts.warn
    );
    assert_eq!(
        verdict.counts.fail, fail,
        "Expected {} fail, got {}",
        fail, verdict.counts.fail
    );
}

#[when(expr = "I generate a reason token for Metric::WallMs with status {string}")]
async fn when_generate_reason_token(world: &mut PerfgateWorld, status_str: String) {
    let status = match status_str.as_str() {
        "pass" => perfgate_types::MetricStatus::Pass,
        "warn" => perfgate_types::MetricStatus::Warn,
        "fail" => perfgate_types::MetricStatus::Fail,
        _ => panic!("Invalid status: {}", status_str),
    };
    world.reason_token = Some(budget_reason_token(perfgate_types::Metric::WallMs, status));
}

#[then(expr = "the reason token should be {string}")]
async fn then_reason_token_should_be(world: &mut PerfgateWorld, expected: String) {
    let token = world.reason_token.as_ref().expect("Reason token not set");
    assert_eq!(
        token, &expected,
        "Expected reason token '{}', got '{}'",
        expected, token
    );
}

// ============================================================================
// SIGNIFICANCE MICROCRATE STEPS
// ============================================================================

#[given(expr = "baseline samples {string}")]
async fn given_baseline_samples(world: &mut PerfgateWorld, samples_str: String) {
    world.significance_baseline = samples_str
        .split(',')
        .map(|s| s.trim().parse().expect("Failed to parse sample"))
        .collect();
}

#[given(expr = "current samples {string}")]
async fn given_current_samples(world: &mut PerfgateWorld, samples_str: String) {
    world.significance_current = samples_str
        .split(',')
        .map(|s| s.trim().parse().expect("Failed to parse sample"))
        .collect();
}

#[when(expr = "I compute significance with alpha {float} and min_samples {int}")]
async fn when_compute_significance(world: &mut PerfgateWorld, alpha: f64, min_samples: usize) {
    world.significance_result = compute_significance(
        &world.significance_baseline,
        &world.significance_current,
        alpha,
        min_samples,
    );
}

#[then("the result should be significant")]
async fn then_result_should_be_significant(world: &mut PerfgateWorld) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert!(
        result.significant,
        "Expected result to be significant, but it is not (p-value: {})",
        result.p_value
    );
}

#[then("the result should not be significant")]
async fn then_result_should_not_be_significant(world: &mut PerfgateWorld) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert!(
        !result.significant,
        "Expected result to not be significant, but it is (p-value: {})",
        result.p_value
    );
}

#[then("the result should be none")]
async fn then_result_should_be_none(world: &mut PerfgateWorld) {
    assert!(
        world.significance_result.is_none(),
        "Expected result to be None, but got {:?}",
        world.significance_result
    );
}

#[then(expr = "the p-value should be less than {float}")]
async fn then_p_value_should_be_less_than(world: &mut PerfgateWorld, threshold: f64) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert!(
        result.p_value < threshold,
        "Expected p-value < {}, got {}",
        threshold,
        result.p_value
    );
}

#[then(expr = "the p-value should be approximately {float}")]
async fn then_p_value_should_be_approximately(world: &mut PerfgateWorld, expected: f64) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert!(
        (result.p_value - expected).abs() < 0.01,
        "Expected p-value ≈ {}, got {}",
        expected,
        result.p_value
    );
}

#[then(expr = "the p-value should be {float}")]
async fn then_p_value_should_be(world: &mut PerfgateWorld, expected: f64) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert!(
        (result.p_value - expected).abs() < 1e-10,
        "Expected p-value {}, got {}",
        expected,
        result.p_value
    );
}

#[then(expr = "the baseline sample count should be {int}")]
async fn then_baseline_sample_count(world: &mut PerfgateWorld, expected: u32) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert_eq!(
        result.baseline_samples, expected,
        "Expected {} baseline samples, got {}",
        expected, result.baseline_samples
    );
}

#[then(expr = "the current sample count should be {int}")]
async fn then_current_sample_count(world: &mut PerfgateWorld, expected: u32) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert_eq!(
        result.current_samples, expected,
        "Expected {} current samples, got {}",
        expected, result.current_samples
    );
}

#[then(expr = "the alpha value should be {float}")]
async fn then_alpha_value_should_be(world: &mut PerfgateWorld, expected: f64) {
    let result = world
        .significance_result
        .as_ref()
        .expect("Significance result not set");
    assert!(
        (result.alpha - expected).abs() < 1e-10,
        "Expected alpha {}, got {}",
        expected,
        result.alpha
    );
}

// ============================================================================
// Baseline Command
// ============================================================================

#[when("I run perfgate baseline with no args")]
async fn when_baseline_no_args(world: &mut PerfgateWorld) {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline");
    let output = cmd.output().expect("failed to run command");

    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

#[when("I run perfgate baseline list without server")]
async fn when_baseline_list_no_server(world: &mut PerfgateWorld) {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline").arg("list");
    let output = cmd.output().expect("failed to run command");

    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

#[when("I run perfgate baseline upload without server")]
async fn when_baseline_upload_no_server(world: &mut PerfgateWorld) {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline")
        .arg("upload")
        .arg("--file")
        .arg("dummy.json");
    let output = cmd.output().expect("failed to run command");

    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

#[when("I run perfgate baseline download without server")]
async fn when_baseline_download_no_server(world: &mut PerfgateWorld) {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline")
        .arg("download")
        .arg("--benchmark")
        .arg("bench")
        .arg("--output")
        .arg("out.json");
    let output = cmd.output().expect("failed to run command");

    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

#[when("I run perfgate baseline delete without server")]
async fn when_baseline_delete_no_server(world: &mut PerfgateWorld) {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline")
        .arg("delete")
        .arg("--benchmark")
        .arg("bench");
    let output = cmd.output().expect("failed to run command");

    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

use cucumber::gherkin::Step;

#[given(expr = "a compare receipt exists at {string} with:")]
async fn given_compare_receipt_exists_with(
    world: &mut PerfgateWorld,
    step: &Step,
    path_str: String,
) {
    world.ensure_temp_dir();
    let path = world.temp_path().join(path_str);

    // Default to a PASS compare receipt
    let mut receipt = world.create_compare_receipt(VerdictStatus::Pass);
    receipt.bench.name = path.file_stem().unwrap().to_string_lossy().to_string();

    if let Some(table) = &step.table {
        // parse the table
        for row in table.rows.iter().skip(1) {
            let metric_name = &row[0];
            let status_str = &row[1];
            let current: f64 = row[2].parse().unwrap();
            let pct: f64 = row[3].parse().unwrap();

            let status = match status_str.as_str() {
                "pass" => perfgate_types::MetricStatus::Pass,
                "warn" => perfgate_types::MetricStatus::Warn,
                "fail" => perfgate_types::MetricStatus::Fail,
                _ => panic!("unknown status"),
            };

            let metric = match metric_name.as_str() {
                "wall_ms" => perfgate_types::Metric::WallMs,
                _ => panic!("unknown metric"),
            };

            receipt.deltas.insert(
                metric,
                perfgate_types::Delta {
                    baseline: current / (1.0 + pct),
                    current,
                    ratio: 1.0 + pct,
                    pct,
                    regression: if pct > 0.0 { pct } else { 0.0 },
                    cv: None,
                    noise_threshold: None,
                    statistic: perfgate_types::MetricStatistic::Median,
                    significance: None,
                    status,
                },
            );

            // update verdict if we have warn/fail
            if status == perfgate_types::MetricStatus::Fail {
                receipt.verdict.status = VerdictStatus::Fail;
                receipt.verdict.counts.fail += 1;
            } else if status == perfgate_types::MetricStatus::Warn {
                if receipt.verdict.status != VerdictStatus::Fail {
                    receipt.verdict.status = VerdictStatus::Warn;
                }
                receipt.verdict.counts.warn += 1;
            }
        }
    }

    let json = serde_json::to_string_pretty(&receipt).unwrap();
    std::fs::write(path, json).unwrap();
}

#[then("the command should succeed")]
async fn then_command_should_succeed(world: &mut PerfgateWorld) {
    let actual = world.last_exit_code.expect("No exit code recorded");
    assert_eq!(
        actual, 0,
        "Expected command to succeed (exit code 0), got {}. Stderr: {}",
        actual, world.last_stderr
    );
}

#[given("a mock baseline server is running")]
async fn given_mock_server(world: &mut PerfgateWorld) {
    let server = wiremock::MockServer::start().await;

    // Mock the upload endpoint
    wiremock::Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path_regex(
            r"^/api/v1/projects/[^/]+/baselines$",
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": "new-baseline-id",
                "benchmark": "bench1",
                "version": "v1",
                "created_at": "2026-01-01T00:00:00Z",
                "etag": "test-etag"
            })),
        )
        .mount(&server)
        .await;

    world.server = Some(server);
}

#[given(expr = "a baseline file exists at {string}")]
async fn given_baseline_file_exists_at(world: &mut PerfgateWorld, path_str: String) {
    world.ensure_temp_dir();
    let path = world.temp_path().join(path_str);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }

    // Create a dummy run receipt
    let receipt = world.create_run_receipt(100);
    // Set bench name to match file name if possible
    let mut receipt = receipt;
    if let Some(stem) = path.file_stem() {
        receipt.bench.name = stem.to_string_lossy().to_string();
    }

    let json = serde_json::to_string_pretty(&receipt).unwrap();
    std::fs::write(path, json).unwrap();
}

#[when(expr = "I run {string}")]
async fn when_run_command(world: &mut PerfgateWorld, command: String) {
    world.ensure_temp_dir();
    let args: Vec<String> = shell_words::split(&command).expect("Failed to parse command");
    if args.is_empty() || args[0] != "perfgate" {
        panic!("Only perfgate commands are supported in this generic step");
    }

    let mut cmd = perfgate_cmd();
    cmd.current_dir(world.temp_path());

    // Add server URL if mock server is running
    if let Some(ref server) = world.server {
        cmd.arg("--baseline-server").arg(server.uri() + "/api/v1");
    }

    for arg in args.iter().skip(1) {
        cmd.arg(arg);
    }

    let output = cmd.output().expect("Failed to execute command");
    world.last_exit_code = Some(output.status.code().unwrap_or(-1));
    world.last_stdout = String::from_utf8_lossy(&output.stdout).to_string();
    world.last_stderr = String::from_utf8_lossy(&output.stderr).to_string();
}

// ============================================================================
// AUTH STEPS
// ============================================================================

#[then(expr = "API key {string} should be valid")]
async fn then_api_key_valid(_world: &mut PerfgateWorld, key: String) {
    use perfgate_auth::validate_key_format;
    assert!(validate_key_format(&key).is_ok());
}

#[then(expr = "API key {string} should be invalid")]
async fn then_api_key_invalid(_world: &mut PerfgateWorld, key: String) {
    use perfgate_auth::validate_key_format;
    assert!(validate_key_format(&key).is_err());
}

#[given(expr = "a role {string}")]
async fn given_role(world: &mut PerfgateWorld, role_str: String) {
    use perfgate_auth::Role;
    let role = match role_str.as_str() {
        "viewer" => Role::Viewer,
        "contributor" => Role::Contributor,
        "promoter" => Role::Promoter,
        "admin" => Role::Admin,
        _ => panic!("Unknown role: {}", role_str),
    };
    world.current_role = Some(role);
}

#[then(expr = "it should have scope {string}")]
async fn then_role_has_scope(world: &mut PerfgateWorld, scope_str: String) {
    use perfgate_auth::Scope;
    let scope = match scope_str.as_str() {
        "read" => Scope::Read,
        "write" => Scope::Write,
        "promote" => Scope::Promote,
        "delete" => Scope::Delete,
        "admin" => Scope::Admin,
        _ => panic!("Unknown scope: {}", scope_str),
    };
    let role = world.current_role.expect("No role set");
    assert!(role.has_scope(scope));
}

#[then(expr = "it should not have scope {string}")]
async fn then_role_not_has_scope(world: &mut PerfgateWorld, scope_str: String) {
    use perfgate_auth::Scope;
    let scope = match scope_str.as_str() {
        "read" => Scope::Read,
        "write" => Scope::Write,
        "promote" => Scope::Promote,
        "delete" => Scope::Delete,
        "admin" => Scope::Admin,
        _ => panic!("Unknown scope: {}", scope_str),
    };
    let role = world.current_role.expect("No role set");
    assert!(!role.has_scope(scope));
}

#[when("I generate a live API key")]
async fn when_generate_live_key(world: &mut PerfgateWorld) {
    use perfgate_auth::generate_api_key;
    world.last_api_key = Some(generate_api_key(false));
}

#[when("I generate a test API key")]
async fn when_generate_test_key(world: &mut PerfgateWorld) {
    use perfgate_auth::generate_api_key;
    world.last_api_key = Some(generate_api_key(true));
}

#[then(expr = "it should start with {string}")]
async fn then_key_starts_with(world: &mut PerfgateWorld, prefix: String) {
    let key = world.last_api_key.as_ref().expect("No API key generated");
    assert!(key.starts_with(&prefix));
}

#[then(expr = "it should be at least {int} characters long")]
async fn then_key_length(world: &mut PerfgateWorld, len: usize) {
    let key = world.last_api_key.as_ref().expect("No API key generated");
    assert!(key.len() >= len);
}

// ============================================================================
// MAIN FUNCTION
// ============================================================================

#[tokio::main]
async fn main() {
    // Filter out @unix tagged scenarios on non-Unix platforms
    #[cfg(unix)]
    {
        PerfgateWorld::run("features/").await;
    }

    #[cfg(not(unix))]
    {
        use cucumber::World;
        PerfgateWorld::cucumber()
            .filter_run("features/", |_feature, _rule, scenario| {
                // Skip scenarios tagged with @unix on non-Unix platforms
                !scenario.tags.iter().any(|tag| tag.to_lowercase() == "unix")
            })
            .await;
    }
}
