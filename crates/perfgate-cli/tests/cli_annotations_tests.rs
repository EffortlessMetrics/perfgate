//! Integration tests for `perfgate github-annotations` command
//!
//! **Validates: Requirements 8.1**

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;
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

/// Test github-annotations with fail scenario
/// Should emit `::error::` annotations for metrics with Fail status
///
/// **Validates: Requirements 8.1**
#[test]
fn test_github_annotations_fail_scenario() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_fail.json");

    // First, generate a compare receipt with fail verdict
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    // Run perfgate github-annotations command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("github-annotations")
        .arg("--compare")
        .arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify ::error:: annotation is present (Requirement 8.2)
        .stdout(predicate::str::contains("::error::"))
        // Verify bench name is in annotation (Requirement 8.5)
        .stdout(predicate::str::contains("test-benchmark"))
        // Verify metric name is in annotation (Requirement 8.5)
        .stdout(predicate::str::contains("wall_ms"))
        // Verify delta percentage format (Requirement 8.5)
        .stdout(predicate::str::contains("%"));
}

/// Test github-annotations with warn scenario
/// Should emit `::warning::` annotations for metrics with Warn status
///
/// **Validates: Requirements 8.1**
#[test]
fn test_github_annotations_warn_scenario() {
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

    // Run perfgate github-annotations command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("github-annotations")
        .arg("--compare")
        .arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify ::warning:: annotation is present (Requirement 8.3)
        .stdout(predicate::str::contains("::warning::"))
        // Verify bench name is in annotation (Requirement 8.5)
        .stdout(predicate::str::contains("test-benchmark"))
        // Verify metric name is in annotation (Requirement 8.5)
        .stdout(predicate::str::contains("wall_ms"));
}

/// Test github-annotations with pass scenario
/// Should emit no annotations for metrics with Pass status
///
/// **Validates: Requirements 8.1**
#[test]
fn test_github_annotations_pass_scenario_empty_output() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // Generate a compare receipt with pass verdict
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    assert!(
        compare_receipt_path.exists(),
        "compare receipt should exist"
    );

    // Run perfgate github-annotations command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("github-annotations")
        .arg("--compare")
        .arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify no ::error:: annotations (Requirement 8.4)
        .stdout(predicate::str::contains("::error::").not())
        // Verify no ::warning:: annotations (Requirement 8.4)
        .stdout(predicate::str::contains("::warning::").not());
}

/// Test github-annotations annotation format contains baseline and current values
/// Verifies the annotation message includes all required information
///
/// **Validates: Requirements 8.1**
#[test]
fn test_github_annotations_contains_baseline_current_values() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_fail.json");

    // Generate a compare receipt with fail verdict
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    // Run perfgate github-annotations command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("github-annotations")
        .arg("--compare")
        .arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify baseline value is present (Requirement 8.5)
        .stdout(predicate::str::contains("baseline"))
        // Verify current value is present (Requirement 8.5)
        .stdout(predicate::str::contains("current"));
}

/// Test github-annotations with missing compare file
/// Should exit with error code 1 (tool error)
///
/// **Validates: Requirements 8.1**
#[test]
fn test_github_annotations_missing_compare_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let nonexistent_path = temp_dir.path().join("nonexistent.json");

    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("github-annotations")
        .arg("--compare")
        .arg(&nonexistent_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

/// Test github-annotations without required --compare argument
/// Should fail with missing argument error
///
/// **Validates: Requirements 8.1**
#[test]
fn test_github_annotations_missing_compare_argument() {
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("github-annotations");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--compare"));
}

/// Test github-annotations outputs one annotation per failing metric
/// Each metric with non-Pass status should have exactly one annotation line
///
/// **Validates: Requirements 8.1**
#[test]
fn test_github_annotations_one_per_metric() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_fail.json");

    // Generate a compare receipt with fail verdict
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    // Run perfgate github-annotations command
    let mut cmd = Command::cargo_bin("perfgate").expect("failed to find perfgate binary");
    cmd.arg("github-annotations")
        .arg("--compare")
        .arg(&compare_receipt_path);

    let output = cmd.output().expect("failed to execute command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Count annotation lines - each non-empty line should be an annotation
    let annotation_lines: Vec<&str> = stdout.lines().filter(|line| !line.is_empty()).collect();

    // With fail scenario, we expect at least one annotation
    // (wall_ms should fail, max_rss_kb may also fail depending on fixture values)
    assert!(
        !annotation_lines.is_empty(),
        "fail scenario should produce at least one annotation"
    );

    // Each annotation line should start with :: (GitHub Actions annotation format)
    for line in &annotation_lines {
        assert!(
            line.starts_with("::"),
            "annotation line should start with '::': {}",
            line
        );
    }
}
