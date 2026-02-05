//! Integration tests for CPU time tracking in `perfgate`.
//!
//! **Validates: CPU time (user + system) collection and reporting**

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

// ============================================================================
// Platform-specific test helpers
// ============================================================================

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

/// Returns a command that does some CPU work (for more meaningful CPU time).
/// On Unix: ["sh", "-c", "for i in $(seq 1 1000); do echo $i > /dev/null; done"]
/// On Windows: ["cmd", "/c", "exit", "0"] (minimal work, CPU time may be 0)
#[cfg(unix)]
fn cpu_work_command() -> Vec<&'static str> {
    vec![
        "sh",
        "-c",
        "for i in $(seq 1 1000); do echo $i > /dev/null; done",
    ]
}

#[cfg(windows)]
fn cpu_work_command() -> Vec<&'static str> {
    // Windows doesn't provide CPU time via adapters, so use simple command
    vec!["cmd", "/c", "exit", "0"]
}

// ============================================================================
// CPU time in run output tests (Unix-only for actual CPU values)
// ============================================================================

/// Test that run command output includes cpu_ms field in samples on Unix.
///
/// **Validates: CPU time collection in samples**
#[cfg(unix)]
#[test]
fn test_run_samples_include_cpu_ms_on_unix() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("run")
        .arg("--name")
        .arg("cpu-time-test")
        .arg("--repeat")
        .arg("2")
        .arg("--out")
        .arg(&output_path)
        .arg("--");

    // Use a command that does some CPU work
    for arg in cpu_work_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    // Read and parse the output file
    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify samples array exists
    let samples = receipt["samples"]
        .as_array()
        .expect("samples should be an array");
    assert!(!samples.is_empty(), "should have samples");

    // On Unix, all samples should have cpu_ms field (may be 0 for fast commands)
    for (i, sample) in samples.iter().enumerate() {
        assert!(
            sample.get("cpu_ms").is_some(),
            "sample {} should have cpu_ms field on Unix",
            i
        );
        // cpu_ms should be a non-negative integer
        let cpu_ms = sample["cpu_ms"]
            .as_u64()
            .expect("cpu_ms should be a valid u64");
        // Just verify it's a reasonable value (not checking exact value as it varies)
        assert!(
            cpu_ms < 60_000,
            "cpu_ms should be reasonable (< 60 seconds), got {}",
            cpu_ms
        );
    }
}

/// Test that CPU time appears in Stats summaries on Unix.
///
/// **Validates: CPU time stats aggregation**
#[cfg(unix)]
#[test]
fn test_run_stats_include_cpu_ms_summary_on_unix() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("run")
        .arg("--name")
        .arg("cpu-stats-test")
        .arg("--repeat")
        .arg("3")
        .arg("--out")
        .arg(&output_path)
        .arg("--");

    for arg in cpu_work_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // On Unix, stats should have cpu_ms summary
    assert!(
        receipt["stats"]["cpu_ms"].is_object(),
        "stats should contain cpu_ms summary on Unix"
    );

    let cpu_stats = &receipt["stats"]["cpu_ms"];
    assert!(
        cpu_stats["median"].is_u64(),
        "cpu_ms stats should have median"
    );
    assert!(cpu_stats["min"].is_u64(), "cpu_ms stats should have min");
    assert!(cpu_stats["max"].is_u64(), "cpu_ms stats should have max");

    // Verify ordering: min <= median <= max
    let min = cpu_stats["min"].as_u64().unwrap();
    let median = cpu_stats["median"].as_u64().unwrap();
    let max = cpu_stats["max"].as_u64().unwrap();
    assert!(
        min <= median,
        "min ({}) should be <= median ({})",
        min,
        median
    );
    assert!(
        median <= max,
        "median ({}) should be <= max ({})",
        median,
        max
    );
}

// ============================================================================
// CSV export tests for CPU time
// ============================================================================

/// Test that CSV export includes cpu_ms_median column.
///
/// **Validates: CPU time in export format**
#[test]
fn test_export_csv_includes_cpu_ms_column() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let run_path = temp_dir.path().join("run.json");
    let export_path = temp_dir.path().join("export.csv");

    // First, run a benchmark to get a run receipt
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("run")
        .arg("--name")
        .arg("csv-export-test")
        .arg("--repeat")
        .arg("2")
        .arg("--out")
        .arg(&run_path)
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    // Now export to CSV
    let mut export_cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    export_cmd
        .arg("export")
        .arg("--run")
        .arg(&run_path)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    export_cmd.assert().success();

    // Read and verify CSV content
    let content = fs::read_to_string(&export_path).expect("failed to read export file");

    // Verify CSV header includes cpu_ms_median
    let header = content.lines().next().expect("CSV should have header");
    assert!(
        header.contains("cpu_ms_median"),
        "CSV header should contain cpu_ms_median column. Header: {}",
        header
    );
}

/// Test that JSONL export includes cpu_ms fields when present.
#[test]
fn test_export_jsonl_includes_cpu_ms_when_present() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let run_path = temp_dir.path().join("run.json");
    let export_path = temp_dir.path().join("export.jsonl");

    // First, run a benchmark
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("run")
        .arg("--name")
        .arg("jsonl-export-test")
        .arg("--repeat")
        .arg("2")
        .arg("--out")
        .arg(&run_path)
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    // Export to JSONL
    let mut export_cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    export_cmd
        .arg("export")
        .arg("--run")
        .arg(&run_path)
        .arg("--format")
        .arg("jsonl")
        .arg("--out")
        .arg(&export_path);

    export_cmd.assert().success();

    let content = fs::read_to_string(&export_path).expect("failed to read export file");
    let lines: Vec<&str> = content.trim().split('\n').collect();
    assert!(!lines.is_empty(), "JSONL should have content");

    // Parse the first line and check for cpu_ms_median field
    let parsed: serde_json::Value = serde_json::from_str(lines[0]).expect("should be valid JSON");

    // cpu_ms_median may be null on non-Unix platforms, but the field should exist
    assert!(
        parsed.get("cpu_ms_median").is_some(),
        "JSONL should have cpu_ms_median field"
    );
}

// ============================================================================
// Graceful handling when cpu_ms is None (non-Unix platforms or missing data)
// ============================================================================

/// Test graceful handling when cpu_ms is None in fixture data.
///
/// **Validates: CPU time optional handling**
#[test]
fn test_export_handles_missing_cpu_ms_gracefully() {
    use std::path::PathBuf;

    let temp_dir = tempdir().expect("failed to create temp dir");
    let export_path = temp_dir.path().join("export.csv");

    // Use the existing baseline.json fixture which doesn't have cpu_ms
    let baseline = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("baseline.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("export")
        .arg("--run")
        .arg(&baseline)
        .arg("--format")
        .arg("csv")
        .arg("--out")
        .arg(&export_path);

    // Should succeed even if cpu_ms is missing
    cmd.assert().success();

    let content = fs::read_to_string(&export_path).expect("failed to read export file");

    // Header should still have cpu_ms_median column
    let header = content.lines().next().expect("CSV should have header");
    assert!(
        header.contains("cpu_ms_median"),
        "CSV header should contain cpu_ms_median even when data is missing"
    );
}

/// Test that compare works correctly when cpu_ms data is not present in receipts.
#[test]
fn test_compare_handles_missing_cpu_ms_gracefully() {
    use std::path::PathBuf;

    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_path = temp_dir.path().join("compare.json");

    // Use fixtures that don't have cpu_ms data
    let baseline = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("baseline.json");
    let current = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("current_pass.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&compare_path);

    // Should succeed without cpu_ms data
    cmd.assert().success();

    let content = fs::read_to_string(&compare_path).expect("failed to read compare file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("compare.json should be valid JSON");

    // Compare should succeed but not have cpu_ms delta (since both lack it)
    // The deltas map should only contain metrics that exist in both baseline and current
    let deltas = &receipt["deltas"];
    assert!(deltas.is_object(), "deltas should be an object");

    // wall_ms should exist (it's required)
    assert!(
        deltas.get("wall_ms").is_some(),
        "deltas should contain wall_ms"
    );
}

// ============================================================================
// CPU time budgeting tests (when thresholds exist)
// ============================================================================

/// Test that CPU time budget violations are detected in compare.
#[test]
fn test_compare_detects_cpu_ms_budget_violation() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");

    // Create baseline receipt with cpu_ms
    let baseline = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.1.0"},
        "run": {
            "id": "baseline",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:01:00Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "cpu-budget-test",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 50},
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 50}
        ],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "cpu_ms": {"median": 50, "min": 50, "max": 50}
        }
    });

    // Create current receipt with higher cpu_ms (regression)
    let current = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.1.0"},
        "run": {
            "id": "current",
            "started_at": "2024-01-02T00:00:00Z",
            "ended_at": "2024-01-02T00:01:00Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "cpu-budget-test",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 100},
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 100}
        ],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "cpu_ms": {"median": 100, "min": 100, "max": 100}
        }
    });

    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&baseline).unwrap(),
    )
    .expect("write baseline");
    fs::write(
        &current_path,
        serde_json::to_string_pretty(&current).unwrap(),
    )
    .expect("write current");

    // Compare with cpu_ms threshold (10% threshold, which is exceeded by 100% regression)
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--current")
        .arg(&current_path)
        .arg("--metric-threshold")
        .arg("cpu_ms=0.10") // 10% threshold
        .arg("--out")
        .arg(&compare_path);

    // Should exit with code 2 (policy fail) due to CPU time regression
    cmd.assert().code(2);

    let content = fs::read_to_string(&compare_path).expect("failed to read compare file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("compare.json should be valid JSON");

    // Verify cpu_ms delta exists and shows failure
    let cpu_delta = &receipt["deltas"]["cpu_ms"];
    assert!(cpu_delta.is_object(), "deltas should contain cpu_ms");
    assert_eq!(
        cpu_delta["status"].as_str(),
        Some("fail"),
        "cpu_ms status should be fail due to 100% regression exceeding 10% threshold"
    );

    // Verify the regression percentage is approximately 100% (1.0)
    let regression = cpu_delta["regression"].as_f64().unwrap();
    assert!(
        (regression - 1.0).abs() < 0.01,
        "regression should be ~100% (1.0), got {}",
        regression
    );
}

/// Test that CPU time budget passes when within threshold.
#[test]
fn test_compare_cpu_ms_budget_passes_within_threshold() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");

    // Create baseline and current with similar cpu_ms values
    let baseline = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.1.0"},
        "run": {
            "id": "baseline",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:01:00Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "cpu-budget-pass-test",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 100},
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 100}
        ],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "cpu_ms": {"median": 100, "min": 100, "max": 100}
        }
    });

    // Current with only 5% increase in cpu_ms
    let current = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.1.0"},
        "run": {
            "id": "current",
            "started_at": "2024-01-02T00:00:00Z",
            "ended_at": "2024-01-02T00:01:00Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "cpu-budget-pass-test",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 105},
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 105}
        ],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "cpu_ms": {"median": 105, "min": 105, "max": 105}
        }
    });

    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&baseline).unwrap(),
    )
    .expect("write baseline");
    fs::write(
        &current_path,
        serde_json::to_string_pretty(&current).unwrap(),
    )
    .expect("write current");

    // Compare with cpu_ms threshold (20% threshold, which should pass with 5% regression)
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--current")
        .arg(&current_path)
        .arg("--metric-threshold")
        .arg("cpu_ms=0.20") // 20% threshold
        .arg("--out")
        .arg(&compare_path);

    // Should exit with code 0 (success)
    cmd.assert().success();

    let content = fs::read_to_string(&compare_path).expect("failed to read compare file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("compare.json should be valid JSON");

    // Verify cpu_ms delta shows pass
    let cpu_delta = &receipt["deltas"]["cpu_ms"];
    assert!(cpu_delta.is_object(), "deltas should contain cpu_ms");
    assert_eq!(
        cpu_delta["status"].as_str(),
        Some("pass"),
        "cpu_ms status should be pass (5% regression within 20% threshold)"
    );
}

// ============================================================================
// CPU time in markdown output tests
// ============================================================================

/// Test that markdown output includes CPU time when present.
#[test]
fn test_md_includes_cpu_ms_when_present() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");
    let md_path = temp_dir.path().join("comment.md");

    // Create receipts with cpu_ms
    let baseline = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.1.0"},
        "run": {
            "id": "baseline",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:01:00Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "md-cpu-test",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 80},
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 80}
        ],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "cpu_ms": {"median": 80, "min": 80, "max": 80}
        }
    });

    let current = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.1.0"},
        "run": {
            "id": "current",
            "started_at": "2024-01-02T00:00:00Z",
            "ended_at": "2024-01-02T00:01:00Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "md-cpu-test",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 85},
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "cpu_ms": 85}
        ],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "cpu_ms": {"median": 85, "min": 85, "max": 85}
        }
    });

    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&baseline).unwrap(),
    )
    .expect("write baseline");
    fs::write(
        &current_path,
        serde_json::to_string_pretty(&current).unwrap(),
    )
    .expect("write current");

    // Generate compare receipt with cpu_ms threshold
    let mut compare_cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    compare_cmd
        .arg("compare")
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--current")
        .arg(&current_path)
        .arg("--metric-threshold")
        .arg("cpu_ms=0.20")
        .arg("--out")
        .arg(&compare_path);
    compare_cmd.assert().success();

    // Generate markdown
    let mut md_cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    md_cmd
        .arg("md")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&md_path);
    md_cmd.assert().success();

    let md_content = fs::read_to_string(&md_path).expect("failed to read markdown file");

    // Markdown should contain cpu_ms metric
    assert!(
        md_content.contains("cpu_ms"),
        "Markdown output should contain cpu_ms metric"
    );
}
