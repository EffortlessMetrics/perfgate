//! Integration tests for `perfgate check --mode cockpit`
//!
//! **Validates: Cockpit integration mode**
//!
//! Tests:
//! - Schema conformance (output matches sensor.report.v1)
//! - Determinism (same input -> byte-identical output fields)
//! - Survivability (tool errors produce valid receipts)
//! - Artifact layout (correct file structure)
//! - Exit code contract (exit 0 unless catastrophic)

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
fn create_baseline_receipt(temp_dir: &std::path::Path, bench_name: &str) -> std::path::PathBuf {
    let baselines_dir = temp_dir.join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");

    let baseline_path = baselines_dir.join(format!("{}.json", bench_name));

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

/// Test cockpit mode produces sensor.report.v1 schema
#[test]
fn test_cockpit_mode_produces_sensor_report_schema() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "test-bench");

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

    assert!(
        output.status.success(),
        "cockpit mode should exit 0: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Check report.json exists at root
    let report_path = out_dir.join("report.json");
    assert!(report_path.exists(), "report.json should exist at root");

    // Parse and verify schema
    let report_content = fs::read_to_string(&report_path).expect("failed to read report");
    let report: Value = serde_json::from_str(&report_content).expect("failed to parse report");

    assert_eq!(
        report["schema"], "sensor.report.v1",
        "schema should be sensor.report.v1"
    );
}

/// Test cockpit mode artifact layout
#[test]
fn test_cockpit_mode_artifact_layout() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "test-bench");
    let _baseline_path = create_baseline_receipt(temp_dir.path(), "test-bench");

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
    assert!(
        output.status.success(),
        "cockpit mode should exit 0: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify cockpit artifact layout:
    // artifacts/perfgate/
    // ├── report.json                    # sensor.report.v1 envelope
    // ├── comment.md                     # Markdown summary
    // └── extras/
    //     ├── perfgate.run.v1.json       # perfgate.run.v1
    //     ├── perfgate.compare.v1.json   # perfgate.compare.v1 (if baseline)
    //     └── perfgate.report.v1.json    # perfgate.report.v1 (native)

    assert!(out_dir.join("report.json").exists(), "report.json at root");
    assert!(out_dir.join("comment.md").exists(), "comment.md at root");
    assert!(out_dir.join("extras").is_dir(), "extras/ directory");
    assert!(
        out_dir.join("extras/perfgate.run.v1.json").exists(),
        "extras/perfgate.run.v1.json"
    );
    assert!(
        out_dir.join("extras/perfgate.compare.v1.json").exists(),
        "extras/perfgate.compare.v1.json (baseline present)"
    );
    assert!(
        out_dir.join("extras/perfgate.report.v1.json").exists(),
        "extras/perfgate.report.v1.json"
    );

    // Verify the root report.json has sensor.report.v1 schema
    let root_report: Value =
        serde_json::from_str(&fs::read_to_string(out_dir.join("report.json")).unwrap()).unwrap();
    assert_eq!(root_report["schema"], "sensor.report.v1");

    // Verify extras/perfgate.report.v1.json has perfgate.report.v1 schema
    let native_report: Value = serde_json::from_str(
        &fs::read_to_string(out_dir.join("extras/perfgate.report.v1.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(native_report["report_type"], "perfgate.report.v1");
}

/// Test cockpit mode exits 0 even on verdict fail
#[test]
fn test_cockpit_mode_exits_zero_on_fail() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "test-bench");

    // Create a baseline with very low wall_ms to trigger a regression
    let baselines_dir = temp_dir.path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");
    let baseline_path = baselines_dir.join("test-bench.json");

    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": { "name": "perfgate", "version": "0.1.0" },
        "run": {
            "id": "baseline-run-id",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:01:00Z",
            "host": { "os": "linux", "arch": "x86_64" }
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 1, "exit_code": 0, "warmup": false, "timed_out": false},
            {"wall_ms": 1, "exit_code": 0, "warmup": false, "timed_out": false}
        ],
        "stats": {
            "wall_ms": { "median": 1, "min": 1, "max": 1 }
        }
    });

    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .expect("Failed to write baseline");

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

    // In cockpit mode, should still exit 0 even if verdict is fail
    assert!(
        output.status.success(),
        "cockpit mode should exit 0 even on fail: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // But the report should contain the fail verdict
    let report_path = out_dir.join("report.json");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    // The verdict status should be fail (actual runs will be much slower than 1ms baseline)
    let verdict_status = report["verdict"]["status"].as_str().unwrap();
    assert!(
        verdict_status == "fail" || verdict_status == "warn",
        "verdict should be fail or warn, got: {}",
        verdict_status
    );
}

/// Test cockpit mode report structure completeness
#[test]
fn test_cockpit_mode_report_structure() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "test-bench");
    let _baseline_path = create_baseline_receipt(temp_dir.path(), "test-bench");

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

    // Verify required fields exist
    assert!(report.get("schema").is_some(), "schema field missing");
    assert!(report.get("tool").is_some(), "tool field missing");
    assert!(report.get("run").is_some(), "run field missing");
    assert!(report.get("verdict").is_some(), "verdict field missing");
    assert!(report.get("findings").is_some(), "findings field missing");
    assert!(report.get("data").is_some(), "data field missing");

    // Verify tool info
    assert_eq!(report["tool"]["name"], "perfgate");

    // Verify run metadata
    let run = &report["run"];
    assert!(run.get("started_at").is_some(), "started_at missing");
    assert!(run.get("ended_at").is_some(), "ended_at missing");
    assert!(run.get("duration_ms").is_some(), "duration_ms missing");
    assert!(run.get("capabilities").is_some(), "capabilities missing");

    // Verify capabilities (baseline should be available since we created one)
    assert_eq!(run["capabilities"]["baseline"]["status"], "available");

    // Verify verdict structure
    let verdict = &report["verdict"];
    assert!(verdict.get("status").is_some(), "verdict.status missing");
    assert!(verdict.get("counts").is_some(), "verdict.counts missing");
    assert!(verdict.get("reasons").is_some(), "verdict.reasons missing");

    // Verify counts use cockpit vocabulary (info/warn/error not pass/warn/fail)
    let counts = &verdict["counts"];
    assert!(counts.get("info").is_some(), "counts.info missing");
    assert!(counts.get("warn").is_some(), "counts.warn missing");
    assert!(counts.get("error").is_some(), "counts.error missing");

    // Verify data section: has summary, no compare key
    let data = &report["data"];
    assert!(data.get("summary").is_some(), "data.summary missing");
    assert!(
        data.get("compare").is_none(),
        "data should not have compare key"
    );
}

/// Test cockpit mode no baseline shows unavailable capability
#[test]
fn test_cockpit_mode_no_baseline_capability() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "no-baseline-bench");
    // Don't create a baseline

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("no-baseline-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit mode should exit 0: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = out_dir.join("report.json");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    // Baseline capability should be unavailable
    assert_eq!(
        report["run"]["capabilities"]["baseline"]["status"],
        "unavailable"
    );

    // Reason should be the normalized token
    let reason = report["run"]["capabilities"]["baseline"]["reason"]
        .as_str()
        .unwrap_or("");
    assert_eq!(
        reason, "no_baseline",
        "reason should be 'no_baseline' token, got: {}",
        reason
    );
}

/// Test cockpit mode handles config errors gracefully
#[test]
fn test_cockpit_mode_config_error_produces_report() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");

    // Create an invalid config file
    let config_path = temp_dir.path().join("invalid.toml");
    fs::write(&config_path, "this is not valid toml {{{").expect("write invalid config");

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

    // Should still exit 0 (error recorded in report)
    assert!(
        output.status.success(),
        "cockpit mode should exit 0 on config error: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // Report should exist with error
    let report_path = out_dir.join("report.json");
    assert!(
        report_path.exists(),
        "report.json should exist even on error"
    );

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    assert_eq!(report["schema"], "sensor.report.v1");
    assert_eq!(report["verdict"]["status"], "fail");
    assert_eq!(report["verdict"]["reasons"][0], "tool_error");

    // Should have error finding with tool.runtime check_id
    let findings = report["findings"].as_array().expect("findings array");
    assert!(!findings.is_empty(), "should have at least one finding");
    assert_eq!(findings[0]["severity"], "error");
    assert_eq!(findings[0]["check_id"], "tool.runtime");
    assert_eq!(findings[0]["code"], "runtime_error");
    // Finding should have structured data with stage and error_kind
    let finding_data = &findings[0]["data"];
    assert!(
        finding_data.get("stage").is_some(),
        "finding should have stage"
    );
    assert!(
        finding_data.get("error_kind").is_some(),
        "finding should have error_kind"
    );
}

/// Test standard mode still works (not affected by cockpit changes)
#[test]
fn test_standard_mode_still_works() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "test-bench");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("standard"); // Explicit standard mode

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    // Standard mode writes perfgate.report.v1 directly to report.json
    let report_path = out_dir.join("report.json");
    assert!(report_path.exists());

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    // Standard mode should NOT produce sensor.report.v1
    assert_eq!(
        report["report_type"], "perfgate.report.v1",
        "standard mode should produce perfgate.report.v1"
    );
}

/// Test default mode is standard (backward compatibility)
#[test]
fn test_default_mode_is_standard() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "test-bench");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir);
    // No --mode argument - should default to standard

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    let report_path = out_dir.join("report.json");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    // Default should be standard mode (perfgate.report.v1)
    assert_eq!(
        report["report_type"], "perfgate.report.v1",
        "default mode should produce perfgate.report.v1"
    );
}
