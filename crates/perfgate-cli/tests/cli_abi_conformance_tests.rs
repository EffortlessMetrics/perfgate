//! ABI conformance tests for sensor.report.v1
//!
//! Validates:
//! - Artifact ordering is sorted by (type, path)
//! - Data opacity: `data` has no `compare` key
//! - Error convention: config error → tool.runtime + runtime_error + structured data
//! - Extras files use versioned names

use assert_cmd::Command;
use serde_json::Value;
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

/// Create a minimal config file with given benches.
fn create_config(temp_dir: &std::path::Path, bench_names: &[&str]) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate.toml");
    let success_cmd = success_command();
    let cmd_str = success_cmd
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let mut content = String::from("[defaults]\nrepeat = 2\nwarmup = 0\nthreshold = 0.20\n\n");
    for name in bench_names {
        content.push_str(&format!(
            "[[bench]]\nname = \"{}\"\ncommand = [{}]\n\n",
            name, cmd_str
        ));
    }

    fs::write(&config_path, content).expect("Failed to write config");
    config_path
}

/// Test that artifacts are sorted by (type, path)
#[test]
fn test_artifact_ordering_sorted() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config(temp_dir.path(), &["test-bench"]);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    let report_path = out_dir.join("report.json");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    let artifacts = report["artifacts"].as_array().expect("artifacts array");
    assert!(!artifacts.is_empty(), "should have artifacts");

    // Verify sorted by (type, path)
    for i in 1..artifacts.len() {
        let prev_type = artifacts[i - 1]["type"].as_str().unwrap();
        let prev_path = artifacts[i - 1]["path"].as_str().unwrap();
        let curr_type = artifacts[i]["type"].as_str().unwrap();
        let curr_path = artifacts[i]["path"].as_str().unwrap();

        assert!(
            (prev_type, prev_path) <= (curr_type, curr_path),
            "artifacts not sorted: ({}, {}) should come before ({}, {})",
            prev_type,
            prev_path,
            curr_type,
            curr_path
        );
    }
}

/// Test that data section has no `compare` key (opacity)
#[test]
fn test_data_opacity_no_compare() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config(temp_dir.path(), &["test-bench"]);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    let report_path = out_dir.join("report.json");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    let data = &report["data"];
    assert!(data.get("summary").is_some(), "data should have summary");
    assert!(
        data.get("compare").is_none(),
        "data should NOT have compare key"
    );
}

/// Test error convention: config error → tool.runtime + runtime_error
#[test]
fn test_error_convention_config_error() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = temp_dir.path().join("bad.toml");
    fs::write(&config_path, "this is not valid toml {{{").expect("write bad config");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success(), "cockpit mode should exit 0");

    let report_path = out_dir.join("report.json");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    assert_eq!(report["verdict"]["status"], "fail");
    assert_eq!(report["verdict"]["reasons"][0], "tool_error");

    let findings = report["findings"].as_array().expect("findings array");
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["check_id"], "tool.runtime");
    assert_eq!(findings[0]["code"], "runtime_error");
    assert_eq!(findings[0]["severity"], "error");

    let finding_data = &findings[0]["data"];
    assert!(
        finding_data.get("stage").is_some(),
        "finding data should have stage"
    );
    assert!(
        finding_data.get("error_kind").is_some(),
        "finding data should have error_kind"
    );
}

/// Test extras files use versioned names
#[test]
fn test_extras_versioned_names() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config(temp_dir.path(), &["test-bench"]);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    // Versioned names should exist
    assert!(
        out_dir.join("extras/perfgate.run.v1.json").exists(),
        "extras/perfgate.run.v1.json should exist"
    );
    assert!(
        out_dir.join("extras/perfgate.report.v1.json").exists(),
        "extras/perfgate.report.v1.json should exist"
    );

    // Old names should NOT exist
    assert!(
        !out_dir.join("extras/run.json").exists(),
        "extras/run.json should NOT exist (old name)"
    );
    assert!(
        !out_dir.join("extras/report.json").exists(),
        "extras/report.json should NOT exist (old name)"
    );
}

/// Test baseline reason is normalized to `no_baseline` token
#[test]
fn test_baseline_reason_normalized() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config(temp_dir.path(), &["no-bl-bench"]);

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("no-bl-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    let report_path = out_dir.join("report.json");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    assert_eq!(
        report["run"]["capabilities"]["baseline"]["status"],
        "unavailable"
    );
    assert_eq!(
        report["run"]["capabilities"]["baseline"]["reason"], "no_baseline",
        "baseline reason should be normalized 'no_baseline' token"
    );
}
