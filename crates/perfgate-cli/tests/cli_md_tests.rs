//! Integration tests for `perfgate md` command
//!
//! **Validates: Requirements 7.1, 7.6**

use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

mod common;
use common::{fixtures_dir, generate_compare_receipt, perfgate_cmd};

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
    let mut cmd = perfgate_cmd();
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
    let mut cmd = perfgate_cmd();
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
    let mut cmd = perfgate_cmd();
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
    let mut cmd = perfgate_cmd();
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

    let mut cmd = perfgate_cmd();
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
    let mut cmd = perfgate_cmd();
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
    let mut cmd = perfgate_cmd();
    cmd.arg("md").arg("--compare").arg(&compare_receipt_path);

    let output = cmd.assert().success();

    // If the receipt has reasons, verify they appear in the markdown
    if receipt["verdict"]["reasons"]
        .as_array()
        .is_some_and(|reasons| !reasons.is_empty())
    {
        output.stdout(predicate::str::contains("Notes:"));
    }
}

/// Test markdown template rendering with Handlebars.
#[test]
fn test_md_template_renders_custom_output() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");
    let template_path = temp_dir.path().join("comment.hbs");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    fs::write(
        &template_path,
        r#"{{header}}
bench={{bench.name}}
{{#each rows}}
- {{metric}} {{status}} {{delta_pct}}
{{/each}}
"#,
    )
    .expect("write template");

    let mut cmd = perfgate_cmd();
    cmd.arg("md")
        .arg("--compare")
        .arg(&compare_receipt_path)
        .arg("--template")
        .arg(&template_path);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("bench=test-benchmark"))
        .stdout(predicate::str::contains("- wall_ms"));
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
    let mut cmd = perfgate_cmd();
    cmd.arg("md").arg("--compare").arg(&compare_receipt_path);

    cmd.assert()
        .success()
        // Verify wall_ms metric is present
        .stdout(predicate::str::contains("wall_ms"))
        // Verify max_rss_kb metric is present (fixtures have this metric)
        .stdout(predicate::str::contains("max_rss_kb"));
}

/// Test markdown stdout is deterministic for the same compare receipt.
#[test]
fn test_md_stdout_deterministic() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_receipt_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_warn.json");
    generate_compare_receipt(&baseline, &current, &compare_receipt_path)
        .expect("failed to generate compare receipt");

    let output1 = perfgate_cmd()
        .arg("md")
        .arg("--compare")
        .arg(&compare_receipt_path)
        .output()
        .expect("first md run");
    assert!(output1.status.success(), "first md run should succeed");

    let output2 = perfgate_cmd()
        .arg("md")
        .arg("--compare")
        .arg(&compare_receipt_path)
        .output()
        .expect("second md run");
    assert!(output2.status.success(), "second md run should succeed");

    assert_eq!(
        output1.stdout, output2.stdout,
        "markdown stdout should be byte-for-byte deterministic"
    );
}
