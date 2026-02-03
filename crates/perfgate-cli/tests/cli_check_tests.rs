//! Integration tests for `perfgate check` command
//!
//! **Validates: Config-driven one-command workflow**

#![allow(deprecated)]

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

/// Returns a cross-platform command that exits successfully.
#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

/// Create a minimal config file with a single bench.
fn create_config_file(temp_dir: &std::path::Path, bench_name: &str) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate.toml");
    let success_cmd = success_command();

    let cmd_str = success_cmd
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let config_content = format!(
        r#"
[defaults]
repeat = 2
warmup = 0
threshold = 0.20

[[bench]]
name = "{}"
command = [{}]
"#,
        bench_name, cmd_str
    );

    fs::write(&config_path, config_content).expect("Failed to write config file");
    config_path
}

/// Create a baseline receipt for testing.
/// Uses high wall_ms values to avoid false regression detection.
fn create_baseline_receipt(temp_dir: &std::path::Path, bench_name: &str) -> std::path::PathBuf {
    let baselines_dir = temp_dir.join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");

    let baseline_path = baselines_dir.join(format!("{}.json", bench_name));

    // Use high baseline values (10 seconds) so that actual runs don't exceed the 20% threshold
    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {
            "name": "perfgate",
            "version": "0.1.0"
        },
        "run": {
            "id": "baseline-run-id",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:01:00Z",
            "host": {
                "os": "linux",
                "arch": "x86_64"
            }
        },
        "bench": {
            "name": bench_name,
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 10000, "exit_code": 0, "warmup": false, "timed_out": false},
            {"wall_ms": 10200, "exit_code": 0, "warmup": false, "timed_out": false}
        ],
        "stats": {
            "wall_ms": {
                "median": 10100,
                "min": 10000,
                "max": 10200
            }
        }
    });

    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .expect("Failed to write baseline");

    baseline_path
}

/// Test basic check command with config file
#[test]
fn test_check_basic_with_config() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "test-bench");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should succeed (pass or no baseline warning)
    assert!(
        output.status.success() || output.status.code() == Some(0),
        "check should succeed: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // run.json should exist
    assert!(out_dir.join("run.json").exists(), "run.json should exist");

    // comment.md should exist
    assert!(
        out_dir.join("comment.md").exists(),
        "comment.md should exist"
    );
}

/// Test check command with baseline
#[test]
fn test_check_with_baseline() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "with-baseline");
    let _baseline_path = create_baseline_receipt(temp_dir.path(), "with-baseline");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("with-baseline")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should succeed
    assert!(
        output.status.success(),
        "check with baseline should succeed: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // All artifacts should exist
    assert!(out_dir.join("run.json").exists(), "run.json should exist");
    assert!(
        out_dir.join("compare.json").exists(),
        "compare.json should exist"
    );
    assert!(
        out_dir.join("report.json").exists(),
        "report.json should exist"
    );
    assert!(
        out_dir.join("comment.md").exists(),
        "comment.md should exist"
    );
}

/// Test check command with missing baseline (warning only)
#[test]
fn test_check_missing_baseline_warns() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "no-baseline");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("no-baseline")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should succeed (warning only)
    assert!(
        output.status.success(),
        "check without baseline should succeed with warning: {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // run.json should exist, but not compare.json
    assert!(out_dir.join("run.json").exists(), "run.json should exist");
    assert!(
        !out_dir.join("compare.json").exists(),
        "compare.json should not exist when no baseline"
    );

    // comment.md should mention no baseline
    let md_content =
        fs::read_to_string(out_dir.join("comment.md")).expect("failed to read comment.md");
    assert!(
        md_content.contains("no baseline"),
        "comment.md should mention no baseline"
    );
}

/// Test check command with --require-baseline fails when baseline missing
#[test]
fn test_check_require_baseline_fails() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "required-baseline");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("required-baseline")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--require-baseline");

    let output = cmd.output().expect("failed to execute check");

    // Should fail (exit code 1 for tool error)
    assert!(
        !output.status.success(),
        "check with --require-baseline should fail when no baseline"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "exit code should be 1 for tool error"
    );

    // Error should mention baseline required
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("baseline required") || stderr.contains("baseline"),
        "stderr should mention baseline: {}",
        stderr
    );
}

/// Test check command with unknown bench name fails
#[test]
fn test_check_unknown_bench_fails() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "existing-bench");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("nonexistent-bench")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should fail
    assert!(
        !output.status.success(),
        "check with unknown bench should fail"
    );

    // Error should mention bench not found
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("nonexistent-bench"),
        "stderr should mention bench not found: {}",
        stderr
    );
}

/// Test check command generates valid JSON artifacts
#[test]
fn test_check_produces_valid_json() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "json-test");
    let _baseline_path = create_baseline_receipt(temp_dir.path(), "json-test");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("json-test")
        .arg("--out-dir")
        .arg(&out_dir);

    cmd.assert().success();

    // Verify run.json is valid
    let run_content = fs::read_to_string(out_dir.join("run.json")).expect("read run.json");
    let run_json: serde_json::Value =
        serde_json::from_str(&run_content).expect("run.json should be valid JSON");
    assert_eq!(
        run_json["schema"].as_str(),
        Some("perfgate.run.v1"),
        "run.json should have correct schema"
    );

    // Verify compare.json is valid
    let compare_content =
        fs::read_to_string(out_dir.join("compare.json")).expect("read compare.json");
    let compare_json: serde_json::Value =
        serde_json::from_str(&compare_content).expect("compare.json should be valid JSON");
    assert_eq!(
        compare_json["schema"].as_str(),
        Some("perfgate.compare.v1"),
        "compare.json should have correct schema"
    );

    // Verify report.json is valid
    let report_content = fs::read_to_string(out_dir.join("report.json")).expect("read report.json");
    let report_json: serde_json::Value =
        serde_json::from_str(&report_content).expect("report.json should be valid JSON");
    assert_eq!(
        report_json["report_type"].as_str(),
        Some("perfgate.report.v1"),
        "report.json should have correct report_type"
    );
}
