//! Integration tests for `perfgate run` command
//!
//! **Validates: Requirements 1.1, 1.2, 9.1**

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

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

/// Test basic run with `--name test -- <success_command>`
/// Verify output file is valid JSON with correct schema
///
/// **Validates: Requirements 1.1, 1.2, 9.1**
#[test]
fn test_run_basic_command() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    // Run perfgate with a simple command that always succeeds
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("run")
        .arg("--name")
        .arg("test")
        .arg("--repeat")
        .arg("2") // Use fewer repeats for faster tests
        .arg("--out")
        .arg(&output_path)
        .arg("--");

    // Add the cross-platform success command
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    // Verify output file exists
    assert!(output_path.exists(), "output file should exist");

    // Read and parse the output file
    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify schema field is "perfgate.run.v1"
    assert_eq!(
        receipt["schema"].as_str(),
        Some("perfgate.run.v1"),
        "schema should be 'perfgate.run.v1'"
    );

    // Verify bench name matches
    assert_eq!(
        receipt["bench"]["name"].as_str(),
        Some("test"),
        "bench name should be 'test'"
    );

    // Verify samples array exists and has expected count
    let samples = receipt["samples"]
        .as_array()
        .expect("samples should be an array");
    assert_eq!(samples.len(), 2, "should have 2 samples (repeat=2)");

    // Verify stats exist
    assert!(
        receipt["stats"]["wall_ms"].is_object(),
        "stats should contain wall_ms"
    );
}

/// Test that run command with default repeat count produces 5 samples
///
/// **Validates: Requirements 1.2**
#[test]
fn test_run_default_repeat_count() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("run")
        .arg("--name")
        .arg("default-repeat-test")
        .arg("--out")
        .arg(&output_path)
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Default repeat is 5
    let samples = receipt["samples"]
        .as_array()
        .expect("samples should be an array");
    assert_eq!(samples.len(), 5, "default repeat should produce 5 samples");
}

/// Test that run command fails without required arguments
///
/// **Validates: Requirements 1.1**
#[test]
fn test_run_missing_name_fails() {
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("run").arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--name"));
}

/// Test that run command fails without command after --
///
/// **Validates: Requirements 1.1**
#[test]
fn test_run_missing_command_fails() {
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("run").arg("--name").arg("test");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

/// Test that receipt contains required tool info
///
/// **Validates: Requirements 9.1**
#[test]
fn test_run_receipt_contains_tool_info() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("run")
        .arg("--name")
        .arg("tool-info-test")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify tool info
    assert_eq!(
        receipt["tool"]["name"].as_str(),
        Some("perfgate"),
        "tool name should be 'perfgate'"
    );
    assert!(
        receipt["tool"]["version"].is_string(),
        "tool version should be present"
    );
}

/// Test that samples contain wall_ms and exit_code
///
/// **Validates: Requirements 1.1, 9.1**
#[test]
fn test_run_samples_contain_required_fields() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("run")
        .arg("--name")
        .arg("sample-fields-test")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    let samples = receipt["samples"]
        .as_array()
        .expect("samples should be an array");
    let sample = &samples[0];

    // Verify required sample fields
    assert!(sample["wall_ms"].is_u64(), "sample should have wall_ms");
    assert!(sample["exit_code"].is_i64(), "sample should have exit_code");
    assert_eq!(
        sample["exit_code"].as_i64(),
        Some(0),
        "exit_code should be 0 for successful command"
    );
    assert_eq!(
        sample["warmup"].as_bool(),
        Some(false),
        "warmup should be false for measured samples"
    );
}
