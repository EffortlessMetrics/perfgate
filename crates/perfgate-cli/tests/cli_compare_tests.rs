//! Integration tests for `perfgate compare` command
//!
//! **Validates: Requirements 4.1, 6.1, 6.2, 6.3**

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

/// Returns the path to the test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

/// Test compare with pass scenario - current is better than baseline
/// Exit code should be 0 (pass)
///
/// **Validates: Requirements 4.1, 6.1**
#[test]
fn test_compare_pass_scenario() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&output_path);

    // Should exit with code 0 (pass)
    cmd.assert().success();

    // Verify output file exists and is valid JSON
    assert!(output_path.exists(), "output file should exist");

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify schema field is "perfgate.compare.v1"
    assert_eq!(
        receipt["schema"].as_str(),
        Some("perfgate.compare.v1"),
        "schema should be 'perfgate.compare.v1'"
    );

    // Verify verdict is Pass (serialized as lowercase)
    assert_eq!(
        receipt["verdict"]["status"].as_str(),
        Some("pass"),
        "verdict should be pass for improved performance"
    );
}

/// Test compare with warn scenario - current is slightly worse than baseline
/// Exit code should be 0 (warn without --fail-on-warn)
///
/// **Validates: Requirements 4.1, 6.1, 6.3**
#[test]
fn test_compare_warn_scenario_without_fail_on_warn() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_warn.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&output_path);

    // Should exit with code 0 (warn without --fail-on-warn)
    cmd.assert().success();

    // Verify output file exists
    assert!(output_path.exists(), "output file should exist");

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify verdict is Warn (serialized as lowercase)
    assert_eq!(
        receipt["verdict"]["status"].as_str(),
        Some("warn"),
        "verdict should be warn for slight regression"
    );
}

/// Test compare with warn scenario and --fail-on-warn flag
/// Exit code should be 3
///
/// **Validates: Requirements 4.1, 6.3**
#[test]
fn test_compare_warn_scenario_with_fail_on_warn() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_warn.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--fail-on-warn")
        .arg("--out")
        .arg(&output_path);

    // Should exit with code 3 (warn with --fail-on-warn)
    cmd.assert().code(3);

    // Verify output file still exists (receipt is written before exit)
    assert!(output_path.exists(), "output file should exist");
}

/// Test compare with fail scenario - current is significantly worse than baseline
/// Exit code should be 2 (fail)
///
/// **Validates: Requirements 4.1, 6.2**
#[test]
fn test_compare_fail_scenario() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_fail.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&output_path);

    // Should exit with code 2 (fail)
    cmd.assert().code(2);

    // Verify output file exists
    assert!(output_path.exists(), "output file should exist");

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify verdict is Fail (serialized as lowercase)
    assert_eq!(
        receipt["verdict"]["status"].as_str(),
        Some("fail"),
        "verdict should be fail for significant regression"
    );
}

/// Test compare with missing baseline file
/// Exit code should be 1 (tool error)
///
/// **Validates: Requirements 6.1**
#[test]
fn test_compare_missing_baseline_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = temp_dir.path().join("nonexistent.json");
    let current = fixtures_dir().join("current_pass.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&output_path);

    // Should exit with code 1 (tool error)
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

/// Test compare with missing current file
/// Exit code should be 1 (tool error)
///
/// **Validates: Requirements 6.1**
#[test]
fn test_compare_missing_current_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = temp_dir.path().join("nonexistent.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&output_path);

    // Should exit with code 1 (tool error)
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

/// Test compare receipt contains required fields
///
/// **Validates: Requirements 4.1**
#[test]
fn test_compare_receipt_contains_required_fields() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&output_path);

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify required fields exist
    assert!(receipt["schema"].is_string(), "schema should exist");
    assert!(receipt["tool"].is_object(), "tool should exist");
    assert!(receipt["bench"].is_object(), "bench should exist");
    assert!(
        receipt["baseline_ref"].is_object(),
        "baseline_ref should exist"
    );
    assert!(
        receipt["current_ref"].is_object(),
        "current_ref should exist"
    );
    assert!(receipt["budgets"].is_object(), "budgets should exist");
    assert!(receipt["deltas"].is_object(), "deltas should exist");
    assert!(receipt["verdict"].is_object(), "verdict should exist");

    // Verify verdict has required fields
    assert!(
        receipt["verdict"]["status"].is_string(),
        "verdict.status should exist"
    );
    assert!(
        receipt["verdict"]["counts"].is_object(),
        "verdict.counts should exist"
    );
}

/// Test compare with custom threshold
///
/// **Validates: Requirements 4.1**
#[test]
fn test_compare_with_custom_threshold() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    // Use current_fail which has 50% regression
    // With threshold of 0.60 (60%), it should pass
    let current = fixtures_dir().join("current_fail.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--threshold")
        .arg("0.60")
        .arg("--out")
        .arg(&output_path);

    // With 60% threshold, 50% regression should pass
    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify verdict is Pass with higher threshold (serialized as lowercase)
    assert_eq!(
        receipt["verdict"]["status"].as_str(),
        Some("pass"),
        "verdict should be pass with higher threshold"
    );
}
