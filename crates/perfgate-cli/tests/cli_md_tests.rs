//! Integration tests for `perfgate md` command
//!
//! **Validates: Requirements 7.1, 7.6**

#![allow(deprecated)]

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

/// Helper function to run compare and generate a compare receipt
fn generate_compare_receipt(
    baseline: &PathBuf,
    current: &PathBuf,
    output_path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = Command::cargo_bin("perfgate")?;
    cmd.arg("compare")
        .arg("--baseline")
        .arg(baseline)
        .arg("--current")
        .arg(current)
        .arg("--out")
        .arg(output_path);

    // We don't care about the exit code here, just that the receipt is generated
    let _ = cmd.output();
    Ok(())
}

/// Test markdown generation from compare receipt with pass verdict
/// Verify output contains expected table structure and verdict emoji
///
/// **Validates: Requirements 7.1, 7.6**
#[test]
fn test_md_pass_verdict_stdout() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // First, generate a compare receipt
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    // Run perfgate md command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md").arg("--compare").arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify verdict emoji for pass (Requirement 7.2)
        .stdout(predicate::str::contains("✅"))
        // Verify benchmark name is present (Requirement 7.3)
        .stdout(predicate::str::contains("test-benchmark"))
        // Verify table header with columns (Requirement 7.4)
        .stdout(predicate::str::contains("| metric |"))
        .stdout(predicate::str::contains("baseline"))
        .stdout(predicate::str::contains("current"))
        .stdout(predicate::str::contains("delta"))
        .stdout(predicate::str::contains("budget"))
        .stdout(predicate::str::contains("status"))
        // Verify metric row exists
        .stdout(predicate::str::contains("wall_ms"));
}

/// Test markdown generation from compare receipt with warn verdict
/// Verify output contains warning emoji
///
/// **Validates: Requirements 7.1, 7.6**
#[test]
fn test_md_warn_verdict_stdout() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_warn.json");

    // Generate a compare receipt with warn verdict
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    // Run perfgate md command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md").arg("--compare").arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify verdict emoji for warn (Requirement 7.2)
        .stdout(predicate::str::contains("⚠️"))
        // Verify benchmark name is present (Requirement 7.3)
        .stdout(predicate::str::contains("test-benchmark"))
        // Verify table structure (Requirement 7.4)
        .stdout(predicate::str::contains("| metric |"));
}

/// Test markdown generation from compare receipt with fail verdict
/// Verify output contains fail emoji
///
/// **Validates: Requirements 7.1, 7.6**
#[test]
fn test_md_fail_verdict_stdout() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_fail.json");

    // Generate a compare receipt with fail verdict
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    // Run perfgate md command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md").arg("--compare").arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify verdict emoji for fail (Requirement 7.2)
        .stdout(predicate::str::contains("❌"))
        // Verify benchmark name is present (Requirement 7.3)
        .stdout(predicate::str::contains("test-benchmark"))
        // Verify table structure (Requirement 7.4)
        .stdout(predicate::str::contains("| metric |"));
}

/// Test markdown output to file with --out flag
/// Verify file is created with expected content
///
/// **Validates: Requirements 7.1, 7.6**
#[test]
fn test_md_output_to_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");
    let md_output_path = temp_dir.path().join("output.md");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // Generate a compare receipt
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    // Run perfgate md command with --out flag
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md")
        .arg("--compare")
        .arg(&compare_receipt_path)
        .arg("--out")
        .arg(&md_output_path);

    cmd.assert().success();

    // Verify output file exists
    assert!(md_output_path.exists(), "markdown output file should exist");

    // Read and verify content
    let content = fs::read_to_string(&md_output_path).expect("failed to read markdown file");

    // Verify verdict emoji (Requirement 7.2)
    assert!(content.contains("✅"), "markdown should contain pass emoji");

    // Verify benchmark name (Requirement 7.3)
    assert!(
        content.contains("test-benchmark"),
        "markdown should contain benchmark name"
    );

    // Verify table header columns (Requirement 7.4)
    assert!(
        content.contains("| metric |"),
        "markdown should contain table header"
    );
    assert!(
        content.contains("baseline"),
        "markdown should contain baseline column"
    );
    assert!(
        content.contains("current"),
        "markdown should contain current column"
    );
    assert!(
        content.contains("delta"),
        "markdown should contain delta column"
    );
    assert!(
        content.contains("budget"),
        "markdown should contain budget column"
    );
    assert!(
        content.contains("status"),
        "markdown should contain status column"
    );

    // Verify metric row
    assert!(
        content.contains("wall_ms"),
        "markdown should contain wall_ms metric"
    );
}

/// Test markdown command with missing compare file
/// Should exit with error code 1
///
/// **Validates: Requirements 7.1**
#[test]
fn test_md_missing_compare_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let nonexistent_path = temp_dir.path().join("nonexistent.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md").arg("--compare").arg(&nonexistent_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

/// Test markdown command without required --compare argument
/// Should fail with missing argument error
///
/// **Validates: Requirements 7.1**
#[test]
fn test_md_missing_compare_argument() {
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--compare"));
}

/// Test markdown output contains verdict reasons when present
/// Uses fail scenario which should have reasons
///
/// **Validates: Requirements 7.5**
#[test]
fn test_md_contains_verdict_reasons() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_fail.json");

    // Generate a compare receipt with fail verdict (should have reasons)
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    // Read the compare receipt to check if it has reasons
    let content =
        fs::read_to_string(&compare_receipt_path).expect("failed to read compare receipt");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("compare receipt should be valid JSON");

    // Run perfgate md command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md").arg("--compare").arg(&compare_receipt_path);

    let output = cmd.assert().success();

    // If the receipt has reasons, verify they appear in the markdown
    if let Some(reasons) = receipt["verdict"]["reasons"].as_array() {
        if !reasons.is_empty() {
            output.stdout(predicate::str::contains("Notes:"));
        }
    }
}

/// Test markdown table contains all expected metrics
/// Verifies both wall_ms and max_rss_kb are present when available
///
/// **Validates: Requirements 7.4**
#[test]
fn test_md_contains_all_metrics() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // Generate a compare receipt
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    // Run perfgate md command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("md").arg("--compare").arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify wall_ms metric is present
        .stdout(predicate::str::contains("wall_ms"))
        // Verify max_rss_kb metric is present (fixtures have this metric)
        .stdout(predicate::str::contains("max_rss_kb"));
}
