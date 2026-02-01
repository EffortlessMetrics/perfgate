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
    BenchMeta, CompareReceipt, CompareRef, Delta, HostInfo, Metric, MetricStatus, RunMeta,
    RunReceipt, Sample, Stats, ToolInfo, U64Summary, Verdict, VerdictCounts, VerdictStatus,
    COMPARE_SCHEMA_V1, RUN_SCHEMA_V1,
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
    let output_path = world.output_path.as_ref().expect("No output path set");
    let content = fs::read_to_string(output_path).expect("Failed to read markdown file");

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
