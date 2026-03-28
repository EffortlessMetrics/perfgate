//! Integration tests for `perfgate promote` command
//!
//! **Validates: Promote use case requirements**

use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

mod common;
use common::{fixtures_dir, perfgate_cmd};

/// Test basic promote copies file correctly
///
/// **Validates: Promote creates baseline file**
#[test]
fn test_promote_creates_baseline_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    let current = fixtures_dir().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path);

    // Should exit with code 0 (success)
    cmd.assert().success();

    // Verify output file exists
    assert!(baseline_path.exists(), "baseline file should exist");

    // Verify it's valid JSON with correct schema
    let content = fs::read_to_string(&baseline_path).expect("failed to read baseline file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");

    assert_eq!(
        receipt["schema"].as_str(),
        Some("perfgate.run.v1"),
        "schema should be 'perfgate.run.v1'"
    );
}

/// Test promote preserves receipt data without normalize
///
/// **Validates: Receipt data preservation**
#[test]
fn test_promote_preserves_receipt_data() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    let current = fixtures_dir().join("baseline.json");

    // Read original to get run_id
    let original_content = fs::read_to_string(&current).expect("failed to read original");
    let original: serde_json::Value =
        serde_json::from_str(&original_content).expect("failed to parse original");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path);

    cmd.assert().success();

    // Read promoted baseline
    let promoted_content = fs::read_to_string(&baseline_path).expect("failed to read promoted");
    let promoted: serde_json::Value =
        serde_json::from_str(&promoted_content).expect("failed to parse promoted");

    // run_id should be preserved without normalize
    assert_eq!(
        original["run"]["id"].as_str(),
        promoted["run"]["id"].as_str(),
        "run_id should be preserved"
    );

    // timestamps should be preserved
    assert_eq!(
        original["run"]["started_at"].as_str(),
        promoted["run"]["started_at"].as_str(),
        "started_at should be preserved"
    );

    // bench metadata should be preserved
    assert_eq!(
        original["bench"]["name"].as_str(),
        promoted["bench"]["name"].as_str(),
        "bench name should be preserved"
    );

    // stats should be preserved
    assert_eq!(
        original["stats"]["wall_ms"]["median"], promoted["stats"]["wall_ms"]["median"],
        "stats should be preserved"
    );
}

/// Test promote with normalize strips run-specific fields
///
/// **Validates: Normalize functionality**
#[test]
fn test_promote_normalize_strips_run_specific_fields() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    let current = fixtures_dir().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path)
        .arg("--normalize");

    cmd.assert().success();

    // Read promoted baseline
    let promoted_content = fs::read_to_string(&baseline_path).expect("failed to read promoted");
    let promoted: serde_json::Value =
        serde_json::from_str(&promoted_content).expect("failed to parse promoted");

    // run_id should be "baseline"
    assert_eq!(
        promoted["run"]["id"].as_str(),
        Some("baseline"),
        "run_id should be 'baseline' after normalize"
    );

    // timestamps should be epoch
    assert_eq!(
        promoted["run"]["started_at"].as_str(),
        Some("1970-01-01T00:00:00Z"),
        "started_at should be epoch after normalize"
    );

    assert_eq!(
        promoted["run"]["ended_at"].as_str(),
        Some("1970-01-01T00:00:00Z"),
        "ended_at should be epoch after normalize"
    );
}

/// Test promote with normalize preserves important data
///
/// **Validates: Normalize preserves bench data**
#[test]
fn test_promote_normalize_preserves_important_data() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    let current = fixtures_dir().join("baseline.json");

    // Read original
    let original_content = fs::read_to_string(&current).expect("failed to read original");
    let original: serde_json::Value =
        serde_json::from_str(&original_content).expect("failed to parse original");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path)
        .arg("--normalize");

    cmd.assert().success();

    // Read promoted baseline
    let promoted_content = fs::read_to_string(&baseline_path).expect("failed to read promoted");
    let promoted: serde_json::Value =
        serde_json::from_str(&promoted_content).expect("failed to parse promoted");

    // bench metadata should be preserved
    assert_eq!(
        original["bench"]["name"].as_str(),
        promoted["bench"]["name"].as_str(),
        "bench name should be preserved after normalize"
    );

    assert_eq!(
        original["bench"]["command"], promoted["bench"]["command"],
        "bench command should be preserved after normalize"
    );

    // stats should be preserved
    assert_eq!(
        original["stats"]["wall_ms"]["median"], promoted["stats"]["wall_ms"]["median"],
        "stats should be preserved after normalize"
    );

    // samples should be preserved
    assert_eq!(
        original["samples"].as_array().map(|a| a.len()),
        promoted["samples"].as_array().map(|a| a.len()),
        "samples count should be preserved after normalize"
    );

    // host info should be preserved (os/arch)
    assert_eq!(
        original["run"]["host"]["os"].as_str(),
        promoted["run"]["host"]["os"].as_str(),
        "host os should be preserved after normalize"
    );

    assert_eq!(
        original["run"]["host"]["arch"].as_str(),
        promoted["run"]["host"]["arch"].as_str(),
        "host arch should be preserved after normalize"
    );
}

/// Test promote fails gracefully with missing source file
///
/// **Validates: Error handling for missing source**
#[test]
fn test_promote_missing_source_file() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    let nonexistent = temp_dir.path().join("nonexistent.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&nonexistent)
        .arg("--to")
        .arg(&baseline_path);

    // Should exit with code 1 (error)
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("read"));
}

/// Test promote fails gracefully with invalid JSON source
///
/// **Validates: Error handling for invalid JSON**
#[test]
fn test_promote_invalid_json_source() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    // Create an invalid JSON file
    let invalid_json = temp_dir.path().join("invalid.json");
    fs::write(&invalid_json, "{ invalid json }").expect("failed to write invalid json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&invalid_json)
        .arg("--to")
        .arg(&baseline_path);

    // Should exit with code 1 (error)
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("parse"));
}

/// Test promote with pretty flag formats JSON nicely
///
/// **Validates: Pretty-print option**
#[test]
fn test_promote_pretty_flag() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    let current = fixtures_dir().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path)
        .arg("--pretty");

    cmd.assert().success();

    // Read promoted baseline
    let content = fs::read_to_string(&baseline_path).expect("failed to read promoted");

    // Pretty-printed JSON should have newlines and indentation
    assert!(
        content.contains('\n'),
        "pretty-printed JSON should contain newlines"
    );
    assert!(
        content.contains("  "),
        "pretty-printed JSON should have indentation"
    );
}

/// Test atomic write behavior - no temp files should remain
///
/// **Validates: Atomic write behavior**
#[test]
fn test_promote_atomic_write_no_temp_files() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    let current = fixtures_dir().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path);

    cmd.assert().success();

    // Check no .tmp files remain in the directory
    let entries: Vec<_> = fs::read_dir(temp_dir.path())
        .expect("failed to read temp dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            name.starts_with('.') && name.ends_with(".tmp")
        })
        .collect();

    assert!(
        entries.is_empty(),
        "no .tmp files should remain after promote, found: {:?}",
        entries.iter().map(|e| e.path()).collect::<Vec<_>>()
    );
}

/// Test promote --ratchet updates config and emits ratchet artifact.
#[test]
fn test_promote_ratchet_updates_config() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let compare_path = temp_dir.path().join("compare.json");
    let baseline_path = temp_dir.path().join("baseline.json");
    let config_path = temp_dir.path().join("perfgate.toml");
    let ratchet_out = temp_dir.path().join("ratchet.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    // Build compare receipt first.
    let mut compare_cmd = perfgate_cmd();
    compare_cmd
        .arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--significance-alpha")
        .arg("0.05")
        .arg("--require-significance")
        .arg("--out")
        .arg(&compare_path);
    compare_cmd.assert().success();

    // Config with ratchet enabled and threshold budget for wall_ms.
    let config = r#"[ratchet]
enabled = true
mode = "threshold"
min_improvement = 0.01
max_tightening = 0.10
require_significance = false
allow_metrics = ["wall_ms"]

[[bench]]
name = "test-benchmark"
command = ["echo", "hello"]
[bench.budgets.wall_ms]
threshold = 0.20
"#;
    fs::write(&config_path, config).expect("write config");

    let mut promote_cmd = perfgate_cmd();
    promote_cmd
        .arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path)
        .arg("--ratchet")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--config")
        .arg(&config_path)
        .arg("--ratchet-out")
        .arg(&ratchet_out);

    promote_cmd.assert().success();
    assert!(ratchet_out.exists(), "ratchet artifact should exist");
    let updated_cfg = fs::read_to_string(&config_path).expect("read config");
    assert!(
        updated_cfg.contains("threshold = 0."),
        "threshold should still be present"
    );
}

/// Test promote creates parent directories if needed
///
/// **Validates: Directory creation**
#[test]
fn test_promote_creates_parent_directories() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir
        .path()
        .join("subdir")
        .join("nested")
        .join("baseline.json");

    let current = fixtures_dir().join("baseline.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current)
        .arg("--to")
        .arg(&baseline_path);

    cmd.assert().success();

    // Verify the file was created in the nested directory
    assert!(
        baseline_path.exists(),
        "baseline file should exist in nested directory"
    );
}
