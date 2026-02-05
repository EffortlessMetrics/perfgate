//! CLI integration tests for the report command.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn perfgate_cmd() -> Command {
    Command::new(assert_cmd::cargo::cargo_bin!("perfgate"))
}

/// Creates a minimal compare receipt JSON for testing.
fn create_compare_receipt(verdict_status: &str, metric_status: &str) -> String {
    format!(
        r#"{{
  "schema": "perfgate.compare.v1",
  "tool": {{"name": "perfgate", "version": "0.1.0"}},
  "bench": {{"name": "test-bench", "cwd": null, "command": ["true"], "repeat": 1, "warmup": 0}},
  "baseline_ref": {{"path": "baseline.json", "run_id": "b123"}},
  "current_ref": {{"path": "current.json", "run_id": "c456"}},
  "budgets": {{"wall_ms": {{"threshold": 0.2, "warn_threshold": 0.18, "direction": "lower"}}}},
  "deltas": {{"wall_ms": {{"baseline": 100.0, "current": 150.0, "ratio": 1.5, "pct": 0.5, "regression": 0.5, "status": "{}"}}}},
  "verdict": {{"status": "{}", "counts": {{"pass": 0, "warn": 0, "fail": 1}}, "reasons": ["wall_ms_fail"]}}
}}"#,
        metric_status, verdict_status
    )
}

#[test]
fn test_report_basic() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path = dir.path().join("report.json");

    fs::write(&compare_path, create_compare_receipt("fail", "fail")).unwrap();

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path)
        .assert()
        .success();

    assert!(report_path.exists());

    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(report["report_type"], "perfgate.report.v1");
    assert_eq!(report["verdict"]["status"], "fail");
    assert_eq!(report["summary"]["fail_count"], 1);
}

#[test]
fn test_report_pass_verdict_no_findings() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path = dir.path().join("report.json");

    // Pass verdict compare receipt
    let pass_compare = r#"{
  "schema": "perfgate.compare.v1",
  "tool": {"name": "perfgate", "version": "0.1.0"},
  "bench": {"name": "test-bench", "command": ["true"], "repeat": 1, "warmup": 0},
  "baseline_ref": {"path": "b.json", "run_id": "b123"},
  "current_ref": {"path": "c.json", "run_id": "c456"},
  "budgets": {"wall_ms": {"threshold": 0.2, "warn_threshold": 0.18, "direction": "lower"}},
  "deltas": {"wall_ms": {"baseline": 100.0, "current": 90.0, "ratio": 0.9, "pct": -0.1, "regression": 0.0, "status": "pass"}},
  "verdict": {"status": "pass", "counts": {"pass": 1, "warn": 0, "fail": 0}, "reasons": []}
}"#;

    fs::write(&compare_path, pass_compare).unwrap();

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path)
        .assert()
        .success();

    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(report["verdict"]["status"], "pass");
    assert!(report["findings"].as_array().unwrap().is_empty());
    assert_eq!(report["summary"]["pass_count"], 1);
}

#[test]
fn test_report_warn_verdict_has_finding() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path = dir.path().join("report.json");

    // Warn verdict compare receipt
    let warn_compare = r#"{
  "schema": "perfgate.compare.v1",
  "tool": {"name": "perfgate", "version": "0.1.0"},
  "bench": {"name": "test-bench", "command": ["true"], "repeat": 1, "warmup": 0},
  "baseline_ref": {"path": "b.json", "run_id": "b123"},
  "current_ref": {"path": "c.json", "run_id": "c456"},
  "budgets": {"wall_ms": {"threshold": 0.2, "warn_threshold": 0.18, "direction": "lower"}},
  "deltas": {"wall_ms": {"baseline": 100.0, "current": 119.0, "ratio": 1.19, "pct": 0.19, "regression": 0.19, "status": "warn"}},
  "verdict": {"status": "warn", "counts": {"pass": 0, "warn": 1, "fail": 0}, "reasons": ["wall_ms_warn"]}
}"#;

    fs::write(&compare_path, warn_compare).unwrap();

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path)
        .assert()
        .success();

    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(report["verdict"]["status"], "warn");
    assert_eq!(report["findings"].as_array().unwrap().len(), 1);
    assert_eq!(report["findings"][0]["code"], "metric_warn");
    assert_eq!(report["summary"]["warn_count"], 1);
}

#[test]
fn test_report_fail_verdict_has_finding() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path = dir.path().join("report.json");

    fs::write(&compare_path, create_compare_receipt("fail", "fail")).unwrap();

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path)
        .assert()
        .success();

    let content = fs::read_to_string(&report_path).unwrap();
    let report: serde_json::Value = serde_json::from_str(&content).unwrap();

    assert_eq!(report["verdict"]["status"], "fail");
    assert_eq!(report["findings"].as_array().unwrap().len(), 1);
    assert_eq!(report["findings"][0]["code"], "metric_fail");
    assert_eq!(report["findings"][0]["severity"], "fail");
}

#[test]
fn test_report_with_markdown_output() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path = dir.path().join("report.json");
    let md_path = dir.path().join("comment.md");

    fs::write(&compare_path, create_compare_receipt("fail", "fail")).unwrap();

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path)
        .arg("--md")
        .arg(&md_path)
        .assert()
        .success();

    assert!(report_path.exists());
    assert!(md_path.exists());

    let md_content = fs::read_to_string(&md_path).unwrap();
    assert!(md_content.contains("perfgate"));
    assert!(md_content.contains("fail"));
}

#[test]
fn test_report_pretty_output() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path = dir.path().join("report.json");

    fs::write(&compare_path, create_compare_receipt("fail", "fail")).unwrap();

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path)
        .arg("--pretty")
        .assert()
        .success();

    let content = fs::read_to_string(&report_path).unwrap();

    // Pretty-printed JSON has newlines and indentation
    assert!(content.contains('\n'));
    assert!(content.contains("  "));
}

#[test]
fn test_report_deterministic() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path1 = dir.path().join("report1.json");
    let report_path2 = dir.path().join("report2.json");

    fs::write(&compare_path, create_compare_receipt("fail", "fail")).unwrap();

    // Run twice
    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path1)
        .assert()
        .success();

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path2)
        .assert()
        .success();

    let content1 = fs::read_to_string(&report_path1).unwrap();
    let content2 = fs::read_to_string(&report_path2).unwrap();

    assert_eq!(content1, content2, "Report output should be deterministic");
}

#[test]
fn test_report_missing_compare_file() {
    let dir = tempdir().unwrap();
    let report_path = dir.path().join("report.json");

    perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(dir.path().join("nonexistent.json"))
        .arg("--out")
        .arg(&report_path)
        .assert()
        .failure()
        .code(1);
}

#[test]
fn test_report_missing_compare_argument() {
    perfgate_cmd()
        .arg("report")
        .arg("--out")
        .arg("report.json")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--compare"));
}

#[test]
fn test_report_exit_code_always_0_or_1() {
    // The report command should exit 0 on success and 1 on error.
    // It does NOT use exit codes 2 or 3 (those are for compare command).

    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    let report_path = dir.path().join("report.json");

    // Even with fail verdict, report exits 0
    fs::write(&compare_path, create_compare_receipt("fail", "fail")).unwrap();

    let output = perfgate_cmd()
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
}
