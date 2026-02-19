//! Integration tests for `perfgate paired` command
//!
//! **Validates: Paired benchmarking mode with interleaved execution**

use assert_cmd::Command;
use predicates::prelude::*;
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

/// Returns a cross-platform command that exits with failure.
#[cfg(unix)]
fn fail_command() -> Vec<&'static str> {
    vec!["false"]
}

#[cfg(windows)]
fn fail_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "1"]
}

/// Test basic paired run produces valid JSON receipt
#[test]
fn test_paired_basic_produces_valid_json() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("test-paired")
        .arg("--repeat")
        .arg("2")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    cmd.assert().success();

    // Verify output file exists
    assert!(output_path.exists(), "output file should exist");

    // Read and parse the output file
    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify schema field is "perfgate.paired.v1"
    assert_eq!(
        receipt["schema"].as_str(),
        Some("perfgate.paired.v1"),
        "schema should be 'perfgate.paired.v1'"
    );

    // Verify bench name matches
    assert_eq!(
        receipt["bench"]["name"].as_str(),
        Some("test-paired"),
        "bench name should be 'test-paired'"
    );

    // Verify samples array exists and has expected count
    let samples = receipt["samples"]
        .as_array()
        .expect("samples should be an array");
    assert_eq!(samples.len(), 2, "should have 2 paired samples (repeat=2)");
}

/// Test paired stats have correct structure (mean, median, std_dev, ci_lower, ci_upper)
#[test]
fn test_paired_stats_have_correct_structure() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("stats-test")
        .arg("--repeat")
        .arg("3")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify stats structure
    let stats = &receipt["stats"];

    // Verify baseline_wall_ms summary (U64Summary: median, min, max)
    assert!(
        stats["baseline_wall_ms"]["median"].is_u64(),
        "baseline_wall_ms should have median"
    );
    assert!(
        stats["baseline_wall_ms"]["min"].is_u64(),
        "baseline_wall_ms should have min"
    );
    assert!(
        stats["baseline_wall_ms"]["max"].is_u64(),
        "baseline_wall_ms should have max"
    );

    // Verify current_wall_ms summary
    assert!(
        stats["current_wall_ms"]["median"].is_u64(),
        "current_wall_ms should have median"
    );
    assert!(
        stats["current_wall_ms"]["min"].is_u64(),
        "current_wall_ms should have min"
    );
    assert!(
        stats["current_wall_ms"]["max"].is_u64(),
        "current_wall_ms should have max"
    );

    // Verify wall_diff_ms (PairedDiffSummary: mean, median, std_dev, min, max, count)
    let wall_diff = &stats["wall_diff_ms"];
    assert!(
        wall_diff["mean"].is_f64() || wall_diff["mean"].is_i64(),
        "wall_diff_ms should have mean"
    );
    assert!(
        wall_diff["median"].is_f64() || wall_diff["median"].is_i64(),
        "wall_diff_ms should have median"
    );
    assert!(
        wall_diff["std_dev"].is_f64() || wall_diff["std_dev"].is_i64(),
        "wall_diff_ms should have std_dev"
    );
    assert!(
        wall_diff["min"].is_f64() || wall_diff["min"].is_i64(),
        "wall_diff_ms should have min"
    );
    assert!(
        wall_diff["max"].is_f64() || wall_diff["max"].is_i64(),
        "wall_diff_ms should have max"
    );
    assert!(
        wall_diff["count"].is_u64(),
        "wall_diff_ms should have count"
    );
    assert_eq!(
        wall_diff["count"].as_u64(),
        Some(3),
        "wall_diff_ms count should match repeat"
    );
}

/// Test paired with work_units for throughput calculations
#[test]
fn test_paired_with_work_units_throughput() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("throughput-test")
        .arg("--repeat")
        .arg("2")
        .arg("--work")
        .arg("1000")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    // Verify bench has work_units
    assert_eq!(
        receipt["bench"]["work_units"].as_u64(),
        Some(1000),
        "bench should have work_units"
    );

    // Verify stats include throughput fields when work_units provided
    let stats = &receipt["stats"];
    assert!(
        stats["baseline_throughput_per_s"].is_object(),
        "baseline_throughput_per_s should exist when work_units provided"
    );
    assert!(
        stats["current_throughput_per_s"].is_object(),
        "current_throughput_per_s should exist when work_units provided"
    );
    assert!(
        stats["throughput_diff_per_s"].is_object(),
        "throughput_diff_per_s should exist when work_units provided"
    );
}

/// Test that paired samples are structured correctly with interleaved execution
#[test]
fn test_paired_samples_structure() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("samples-test")
        .arg("--repeat")
        .arg("3")
        .arg("--warmup")
        .arg("1")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    let samples = receipt["samples"]
        .as_array()
        .expect("samples should be an array");

    // Should have warmup + repeat samples
    assert_eq!(
        samples.len(),
        4,
        "should have 4 samples (1 warmup + 3 repeat)"
    );

    // Verify each sample has required paired structure
    for (i, sample) in samples.iter().enumerate() {
        // Check pair_index
        assert_eq!(
            sample["pair_index"].as_u64(),
            Some(i as u64),
            "pair_index should match iteration"
        );

        // Check warmup flag
        let is_warmup = i < 1; // first sample is warmup
        assert_eq!(
            sample["warmup"].as_bool(),
            Some(is_warmup),
            "warmup flag should be correct for sample {i}"
        );

        // Check baseline half
        assert!(
            sample["baseline"]["wall_ms"].is_u64(),
            "baseline should have wall_ms"
        );
        assert!(
            sample["baseline"]["exit_code"].is_i64(),
            "baseline should have exit_code"
        );
        assert!(
            sample["baseline"]["timed_out"].is_boolean(),
            "baseline should have timed_out"
        );

        // Check current half
        assert!(
            sample["current"]["wall_ms"].is_u64(),
            "current should have wall_ms"
        );
        assert!(
            sample["current"]["exit_code"].is_i64(),
            "current should have exit_code"
        );
        assert!(
            sample["current"]["timed_out"].is_boolean(),
            "current should have timed_out"
        );

        // Check wall_diff_ms computed correctly
        assert!(
            sample["wall_diff_ms"].is_i64(),
            "sample should have wall_diff_ms"
        );
    }
}

/// Test error handling when baseline command fails
#[test]
fn test_paired_error_when_baseline_fails() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("fail-test")
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd");

    for arg in fail_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    // Should fail because baseline exits non-zero
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("baseline"));
}

/// Test error handling when current command fails
#[test]
fn test_paired_error_when_current_fails() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("fail-test")
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in fail_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    // Should fail because current exits non-zero
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("current"));
}

/// Test --allow-nonzero flag allows non-zero exits
#[test]
fn test_paired_allow_nonzero_succeeds() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("allow-nonzero-test")
        .arg("--repeat")
        .arg("1")
        .arg("--allow-nonzero")
        .arg("--baseline-cmd");

    for arg in fail_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    // Should succeed with --allow-nonzero
    cmd.assert().success();

    // Verify file was written
    assert!(output_path.exists(), "output file should exist");
}

/// Test missing required arguments fails
#[test]
fn test_paired_missing_name_fails() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--baseline-cmd")
        .arg("true")
        .arg("--current-cmd")
        .arg("true");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--name"));
}

/// Test missing baseline-cmd fails
#[test]
fn test_paired_missing_baseline_cmd_fails() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("test")
        .arg("--current-cmd")
        .arg("true");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("baseline"));
}

/// Test missing current-cmd fails
#[test]
fn test_paired_missing_current_cmd_fails() {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("test")
        .arg("--baseline-cmd")
        .arg("true");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("current"));
}

/// Test receipt contains required tool info
#[test]
fn test_paired_receipt_contains_tool_info() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("tool-info-test")
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

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

    // Verify run metadata
    assert!(receipt["run"]["id"].is_string(), "run id should be present");
    assert!(
        receipt["run"]["started_at"].is_string(),
        "started_at should be present"
    );
    assert!(
        receipt["run"]["ended_at"].is_string(),
        "ended_at should be present"
    );
    assert!(receipt["run"]["host"].is_object(), "host should be present");
}

/// Test --pretty flag produces formatted JSON
#[test]
fn test_paired_pretty_flag_formats_json() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("pretty-test")
        .arg("--repeat")
        .arg("1")
        .arg("--pretty")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");

    // Pretty-printed JSON should contain newlines and indentation
    assert!(
        content.contains('\n'),
        "pretty-printed JSON should contain newlines"
    );
    assert!(
        content.contains("  "),
        "pretty-printed JSON should contain indentation"
    );
}

/// Test default output file name
#[test]
fn test_paired_default_output_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("paired")
        .arg("--name")
        .arg("default-out-test")
        .arg("--repeat")
        .arg("1")
        .arg("--baseline-cmd");

    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in success_command() {
        cmd.arg(arg);
    }

    cmd.assert().success();

    // Default output file should be "perfgate-paired.json"
    let default_output = temp_dir.path().join("perfgate-paired.json");
    assert!(
        default_output.exists(),
        "default output file 'perfgate-paired.json' should exist"
    );
}

/// Test bench metadata contains correct commands
#[test]
fn test_paired_bench_metadata() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let baseline_cmd = success_command();
    let current_cmd = success_command();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("meta-test")
        .arg("--repeat")
        .arg("5")
        .arg("--warmup")
        .arg("2")
        .arg("--baseline-cmd");

    for arg in &baseline_cmd {
        cmd.arg(arg);
    }

    cmd.arg("--current-cmd");
    for arg in &current_cmd {
        cmd.arg(arg);
    }

    cmd.arg("--out").arg(&output_path);

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    let bench = &receipt["bench"];

    // Verify bench metadata
    assert_eq!(
        bench["name"].as_str(),
        Some("meta-test"),
        "bench name should match"
    );
    assert_eq!(bench["repeat"].as_u64(), Some(5), "repeat should match");
    assert_eq!(bench["warmup"].as_u64(), Some(2), "warmup should match");

    // Verify commands are arrays
    assert!(
        bench["current_command"].is_array(),
        "current_command should be an array"
    );
}

/// Test paired command with shell string arguments
#[test]
fn test_paired_with_shell_strings() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("paired.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("paired")
        .arg("--name")
        .arg("shell-test")
        .arg("--repeat")
        .arg("1");

    #[cfg(unix)]
    {
        cmd.arg("--baseline-cmd").arg("true");
        cmd.arg("--current-cmd").arg("true");
    }
    #[cfg(windows)]
    {
        cmd.arg("--baseline-cmd").arg("cmd /c exit 0");
        cmd.arg("--current-cmd").arg("cmd /c exit 0");
    }

    cmd.arg("--out").arg(&output_path);

    cmd.assert().success();

    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    assert_eq!(
        receipt["bench"]["name"].as_str(),
        Some("shell-test"),
        "bench name should match"
    );

    let baseline_cmd = receipt["bench"]["baseline_command"]
        .as_array()
        .expect("baseline_command should be array");
    let current_cmd = receipt["bench"]["current_command"]
        .as_array()
        .expect("current_command should be array");

    #[cfg(unix)]
    {
        assert_eq!(baseline_cmd[0], "true");
        assert_eq!(current_cmd[0], "true");
    }
    #[cfg(windows)]
    {
        assert_eq!(baseline_cmd[0], "cmd");
        assert_eq!(baseline_cmd[1], "/c");
        assert_eq!(baseline_cmd[2], "exit");
        assert_eq!(baseline_cmd[3], "0");
        assert_eq!(current_cmd[0], "cmd");
    }
}
