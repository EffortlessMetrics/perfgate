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
use cucumber::{given, then, when, World};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Re-export types we need for fixture creation
use perfgate_types::{
    BenchConfigFile, BenchMeta, CompareReceipt, CompareRef, ConfigFile, DefaultsConfig, Delta,
    HostInfo, Metric, MetricStatus, PerfgateReport, RunMeta, RunReceipt, Sample, Stats, ToolInfo,
    U64Summary, Verdict, VerdictCounts, VerdictStatus, COMPARE_SCHEMA_V1, REPORT_SCHEMA_V1,
    RUN_SCHEMA_V1,
};

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
    /// Path to config file (for check command)
    config_path: Option<PathBuf>,
    /// Path to artifacts directory (for check command)
    artifacts_dir: Option<PathBuf>,
    /// Config file being built
    config: Option<ConfigFile>,
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
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
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
                max_rss_kb: Some(1024),
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary {
                    median: wall_ms_median,
                    min: wall_ms_median.saturating_sub(10),
                    max: wall_ms_median.saturating_add(10),
                },
                max_rss_kb: Some(U64Summary {
                    median: 1024,
                    min: 1000,
                    max: 1100,
                }),
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
                status: metric_status,
            },
        );

        let reasons = match verdict_status {
            VerdictStatus::Pass => vec![],
            VerdictStatus::Warn => vec!["wall_ms: +15.0% (warn threshold: 18.0%)".to_string()],
            VerdictStatus::Fail => vec!["wall_ms: +50.0% exceeds 20.0% threshold".to_string()],
        };

        let counts = match verdict_status {
            VerdictStatus::Pass => VerdictCounts {
                pass: 1,
                warn: 0,
                fail: 0,
            },
            VerdictStatus::Warn => VerdictCounts {
                pass: 0,
                warn: 1,
                fail: 0,
            },
            VerdictStatus::Fail => VerdictCounts {
                pass: 0,
                warn: 0,
                fail: 1,
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

/// Create a baseline receipt with specified max_rss_kb
#[given(expr = "a baseline receipt with max_rss_kb median of {int}")]
async fn given_baseline_receipt_with_rss(world: &mut PerfgateWorld, max_rss_kb: u64) {
    world.ensure_temp_dir();
    let mut receipt = world.create_run_receipt(world.baseline_wall_ms.unwrap_or(1000));
    receipt.stats.max_rss_kb = Some(U64Summary {
        median: max_rss_kb,
        min: max_rss_kb.saturating_sub(100),
        max: max_rss_kb.saturating_add(100),
    });
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
    receipt.stats.max_rss_kb = Some(U64Summary {
        median: max_rss_kb,
        min: max_rss_kb.saturating_sub(100),
        max: max_rss_kb.saturating_add(100),
    });
    let current_path = world.temp_path().join("current.json");

    let json = serde_json::to_string_pretty(&receipt).expect("Failed to serialize current");
    fs::write(&current_path, json).expect("Failed to write current receipt");
    world.current_path = Some(current_path);
}

// ============================================================================
// WHEN STEPS - CLI Command Execution
// ============================================================================

/// Helper function to get the perfgate binary command
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

/// Assert the reasons mention regression percentage
#[then("the reasons should mention regression percentage")]
async fn then_reasons_mention_regression(world: &mut PerfgateWorld) {
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read output file");
    let receipt: CompareReceipt =
        serde_json::from_str(&content).expect("Failed to parse compare receipt");

    assert!(
        !receipt.verdict.reasons.is_empty(),
        "Verdict should have reasons for regression"
    );

    let has_percentage = receipt
        .verdict
        .reasons
        .iter()
        .any(|r: &String| r.contains('%') || r.contains("threshold"));

    assert!(
        has_percentage,
        "Reasons should mention regression percentage: {:?}",
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
    if let Some(ref run_id) = world.source_run_id {
        receipt.run.id = run_id.clone();
    }
    if let Some(ref started_at) = world.source_started_at {
        receipt.run.started_at = started_at.clone();
    }
    if let Some(ref bench_name) = world.source_bench_name {
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
    if let Some(ref source_path) = world.source_run_path {
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
    if let Some(ref source_path) = world.source_run_path {
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
    if let Some(ref source_path) = world.source_run_path {
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
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
        },
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
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(threshold),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
        },
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
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(threshold),
            warn_factor: Some(warn_factor),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
        },
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
            repeat: Some(repeat),
            warmup: Some(warmup),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
        },
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
            repeat: Some(repeat),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some("baselines".to_string()),
        },
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
            repeat: Some(1),
            warmup: Some(0),
            threshold: Some(0.20),
            warn_factor: Some(0.90),
            out_dir: None,
            baseline_dir: Some(baseline_dir.clone()),
        },
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
