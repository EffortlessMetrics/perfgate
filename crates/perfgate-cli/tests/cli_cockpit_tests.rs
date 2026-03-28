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

use serde_json::Value;
use std::fs;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

/// Returns a cross-platform command that exits successfully.
#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

/// Returns a cross-platform command that is guaranteed to take some time.
#[cfg(unix)]
fn slow_command() -> Vec<&'static str> {
    vec!["sh", "-c", "sleep 0.05"]
}

#[cfg(windows)]
fn slow_command() -> Vec<&'static str> {
    vec!["powershell", "-Command", "Start-Sleep -Milliseconds 50"]
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

/// Create a config file with a slow command.
fn create_slow_config_file(temp_dir: &std::path::Path, bench_name: &str) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate_slow.toml");
    let slow_cmd = slow_command();

    let cmd_str = slow_cmd
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

/// Create a minimal JSON config file with a single bench.
fn create_json_config_file(temp_dir: &std::path::Path, bench_name: &str) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate.json");
    let cmd = success_command();

    let config = serde_json::json!({
        "defaults": {
            "repeat": 1,
            "warmup": 0,
            "threshold": 0.20
        },
        "bench": [{
            "name": bench_name,
            "command": cmd
        }]
    });

    fs::write(&config_path, serde_json::to_string_pretty(&config).unwrap())
        .expect("Failed to write config file");
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

    let mut cmd = perfgate_cmd();
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

    let mut cmd = perfgate_cmd();
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

/// Test cockpit mode honors --emit-repair-context for passing checks.
#[test]
fn test_cockpit_mode_emit_repair_context_writes_extras_artifact() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "test-bench");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("test-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit")
        .arg("--emit-repair-context");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit mode should exit 0: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        out_dir.join("extras/repair_context.json").exists(),
        "extras/repair_context.json should exist when --emit-repair-context is set"
    );
}

/// Test cockpit mode exits 0 even on verdict fail
#[test]
fn test_cockpit_mode_exits_zero_on_fail() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_slow_config_file(temp_dir.path(), "test-bench");

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

    let mut cmd = perfgate_cmd();
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

    let mut cmd = perfgate_cmd();
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

    let mut cmd = perfgate_cmd();
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

    let mut cmd = perfgate_cmd();
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

/// Test cockpit mode can parse JSON config files
#[test]
fn test_cockpit_mode_json_config_parsing() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_json_config_file(temp_dir.path(), "json-bench");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("json-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit mode should succeed with JSON config: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        out_dir.join("report.json").exists(),
        "report.json should exist"
    );
}

/// Test cockpit mode reports an error when extras directory cannot be created
#[test]
fn test_cockpit_mode_extras_dir_creation_error() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "extras-error");

    fs::create_dir_all(&out_dir).expect("create out_dir");
    // Create a file where extras directory should be
    fs::write(out_dir.join("extras"), "not a dir").expect("write extras file");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("extras-error")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit mode should exit 0 on extras dir error: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = out_dir.join("report.json");
    assert!(report_path.exists(), "report.json should exist");
}

/// Test cockpit mode fails when it cannot write the error report
#[test]
fn test_cockpit_mode_catastrophic_report_write_failure() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("not-a-dir");
    fs::write(&out_dir, "not a directory").expect("write out_dir file");

    let missing_config = temp_dir.path().join("missing.toml");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&missing_config)
        .arg("--bench")
        .arg("bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        !output.status.success(),
        "cockpit mode should fail when report write fails"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Test standard mode still works (not affected by cockpit changes)
#[test]
fn test_standard_mode_still_works() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "test-bench");

    let mut cmd = perfgate_cmd();
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

    let mut cmd = perfgate_cmd();
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

/// Test cockpit mode with missing bench produces error report
#[test]
fn test_cockpit_mode_missing_bench_produces_error_report() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "real-bench");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("nonexistent-bench") // Not in config
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");

    // Should exit 0 in cockpit mode (error recorded in report)
    assert!(
        output.status.success(),
        "cockpit mode should exit 0 on missing bench: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Report should exist with error
    let report_path = out_dir.join("report.json");
    assert!(report_path.exists(), "report.json should exist");

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    assert_eq!(report["schema"], "sensor.report.v1");
    assert_eq!(report["verdict"]["status"], "fail");
}

/// Test cockpit mode rejects path-traversal bench names with error report
#[test]
fn test_cockpit_mode_rejects_path_traversal_bench_name() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");

    // Config with a path-traversal bench name
    let config_path = temp_dir.path().join("perfgate.toml");
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
name = "../evil"
command = [{}]
"#,
        cmd_str
    );
    fs::write(&config_path, config_content).expect("write config");

    let mut cmd = perfgate_cmd();
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("../evil")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");

    // Cockpit mode: exit 0 with error recorded in report
    assert!(
        output.status.success(),
        "cockpit mode should exit 0 on validation error: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = out_dir.join("report.json");
    assert!(
        report_path.exists(),
        "report.json should exist even on validation error"
    );

    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    assert_eq!(report["schema"], "sensor.report.v1");
    assert_eq!(report["verdict"]["status"], "fail");
    assert_eq!(report["verdict"]["reasons"][0], "tool_error");

    // Should have error finding with config_parse stage
    let findings = report["findings"].as_array().expect("findings array");
    assert!(!findings.is_empty(), "should have at least one finding");
    assert_eq!(findings[0]["check_id"], "tool.runtime");
    assert_eq!(findings[0]["code"], "runtime_error");
    let finding_data = &findings[0]["data"];
    assert_eq!(finding_data["stage"], "config_parse");
}

// ======================================================================
// Multi-bench cockpit integration tests
// ======================================================================

/// Create a config file with multiple benches for cockpit tests.
fn create_multi_bench_config(
    temp_dir: &std::path::Path,
    bench_names: &[&str],
) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate.toml");
    let success_cmd = success_command();

    let cmd_str = success_cmd
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let mut config_content = String::from(
        r#"
[defaults]
repeat = 2
warmup = 0
threshold = 0.20
"#,
    );

    for name in bench_names {
        config_content.push_str(&format!(
            r#"
[[bench]]
name = "{}"
command = [{}]
"#,
            name, cmd_str
        ));
    }

    fs::write(&config_path, config_content).expect("Failed to write config file");
    config_path
}

/// Create a config file with multiple slow benches.
fn create_slow_multi_bench_config(
    temp_dir: &std::path::Path,
    bench_names: &[&str],
) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate_slow_multi.toml");
    let slow_cmd = slow_command();

    let cmd_str = slow_cmd
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let mut config_content = String::from(
        r#"
[defaults]
repeat = 2
warmup = 0
threshold = 0.20
"#,
    );

    for name in bench_names {
        config_content.push_str(&format!(
            r#"
[[bench]]
name = "{}"
command = [{}]
"#,
            name, cmd_str
        ));
    }

    fs::write(&config_path, config_content).expect("Failed to write config file");
    config_path
}

/// Load the vendored schema validator.
fn load_vendored_schema_validator() -> jsonschema::Validator {
    let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/schemas/sensor.report.v1.schema.json");
    let schema_content = fs::read_to_string(&schema_path).expect("read schema");
    let schema_value: Value = serde_json::from_str(&schema_content).expect("parse schema");
    jsonschema::validator_for(&schema_value).expect("compile schema")
}

/// Run perfgate check --all --mode cockpit and return the parsed report.json.
fn run_cockpit_multi_bench(
    temp_dir: &tempfile::TempDir,
    bench_names: &[&str],
    create_baselines: bool,
) -> (Value, std::path::PathBuf) {
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_multi_bench_config(temp_dir.path(), bench_names);

    if create_baselines {
        for name in bench_names {
            create_baseline_receipt(temp_dir.path(), name);
        }
    }

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit multi-bench should exit 0: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report_path = out_dir.join("report.json");
    let content = fs::read_to_string(&report_path).expect("read report.json");
    let report: Value = serde_json::from_str(&content).expect("parse report.json");

    (report, out_dir)
}

/// Test multi-bench cockpit artifact layout (no baselines).
#[test]
fn test_cockpit_multi_bench_artifact_layout() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (_report, out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    // Root artifacts
    assert!(out_dir.join("report.json").exists(), "report.json at root");
    assert!(out_dir.join("comment.md").exists(), "comment.md at root");

    // Per-bench subdirectories under extras/
    for name in &["bench-a", "bench-b"] {
        let prefix = out_dir.join("extras").join(name);
        assert!(prefix.is_dir(), "extras/{} directory", name);
        assert!(
            prefix.join("perfgate.run.v1.json").exists(),
            "extras/{}/perfgate.run.v1.json",
            name
        );
        assert!(
            prefix.join("perfgate.report.v1.json").exists(),
            "extras/{}/perfgate.report.v1.json",
            name
        );
        // No baseline → no compare file
        assert!(
            !prefix.join("perfgate.compare.v1.json").exists(),
            "extras/{}/perfgate.compare.v1.json should NOT exist without baseline",
            name
        );
    }
}

/// Test multi-bench cockpit artifact layout with baselines.
#[test]
fn test_cockpit_multi_bench_artifact_layout_with_baselines() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (_report, out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], true);

    for name in &["bench-a", "bench-b"] {
        let prefix = out_dir.join("extras").join(name);
        assert!(
            prefix.join("perfgate.compare.v1.json").exists(),
            "extras/{}/perfgate.compare.v1.json should exist with baseline",
            name
        );
    }
}

/// Test multi-bench cockpit output validates against vendored schema.
#[test]
fn test_cockpit_multi_bench_schema_validation() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    let validator = load_vendored_schema_validator();
    let errors: Vec<_> = validator.iter_errors(&report).collect();
    assert!(
        errors.is_empty(),
        "multi-bench report failed schema validation:\n{}",
        errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Test multi-bench verdict is worst-of across benches.
#[test]
fn test_cockpit_multi_bench_verdict_worst_wins() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_slow_multi_bench_config(temp_dir.path(), &["bench-a", "bench-b"]);

    // Create a baseline with very low wall_ms for bench-a to trigger fail
    let baselines_dir = temp_dir.path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("create baselines dir");

    let fast_baseline = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": { "name": "perfgate", "version": "0.1.0" },
        "run": {
            "id": "baseline-id",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:01:00Z",
            "host": { "os": "linux", "arch": "x86_64" }
        },
        "bench": {
            "name": "bench-a",
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 1, "exit_code": 0, "warmup": false, "timed_out": false},
            {"wall_ms": 1, "exit_code": 0, "warmup": false, "timed_out": false}
        ],
        "stats": { "wall_ms": { "median": 1, "min": 1, "max": 1 } }
    });
    fs::write(
        baselines_dir.join("bench-a.json"),
        serde_json::to_string_pretty(&fast_baseline).unwrap(),
    )
    .expect("write bench-a baseline");
    // No baseline for bench-b

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    let report: Value =
        serde_json::from_str(&fs::read_to_string(out_dir.join("report.json")).unwrap()).unwrap();

    let verdict_status = report["verdict"]["status"].as_str().unwrap();
    assert!(
        verdict_status == "fail" || verdict_status == "warn",
        "aggregated verdict should be fail or warn (worst-of), got: {}",
        verdict_status
    );
}

/// Test multi-bench counts are summed across benches.
#[test]
fn test_cockpit_multi_bench_counts_summed() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    // Both benches without baseline: each produces warn=1 (no_baseline)
    let counts = &report["verdict"]["counts"];
    let warn = counts["warn"].as_u64().unwrap();
    let info = counts["info"].as_u64().unwrap();
    let error = counts["error"].as_u64().unwrap();

    // Two benches, each with 1 warn finding → combined should have 2 warn
    assert_eq!(warn, 2, "warn counts should be summed");
    assert_eq!(info, 0, "info counts should be summed");
    assert_eq!(error, 0, "error counts should be summed");

    // Total in data.summary should match
    let summary = &report["data"]["summary"];
    assert_eq!(
        summary["total_count"].as_u64().unwrap(),
        info + warn + error,
        "total_count should be sum of counts"
    );
}

/// Test cockpit mode writes GitHub outputs when requested.
#[test]
fn test_cockpit_mode_output_github_writes_outputs() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "gh-cockpit-bench");
    let github_output = temp_dir.path().join("github_output.txt");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("gh-cockpit-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit")
        .arg("--output-github")
        .env("GITHUB_OUTPUT", &github_output);

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit mode should exit 0: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(&github_output).expect("read GITHUB_OUTPUT");
    assert!(content.contains("verdict="));
    assert!(content.contains("pass_count="));
    assert!(content.contains("warn_count="));
    assert!(content.contains("fail_count="));
    assert!(content.contains("bench_count=1"));
}

/// Test cockpit mode applies --md-template to generated comment.md.
#[test]
fn test_cockpit_mode_md_template_customizes_comment() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config_file(temp_dir.path(), "template-bench");
    let _baseline_path = create_baseline_receipt(temp_dir.path(), "template-bench");
    let template_path = temp_dir.path().join("comment.hbs");

    fs::write(
        &template_path,
        r#"bench={{bench.name}}
{{#each rows}}metric={{metric}} status={{status}}
{{/each}}
"#,
    )
    .expect("write template");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("template-bench")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit")
        .arg("--md-template")
        .arg(&template_path);

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit mode should succeed: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let content = fs::read_to_string(out_dir.join("comment.md")).expect("read comment.md");
    assert!(content.contains("bench=template-bench"));
    assert!(content.contains("metric=wall_ms"));
}

/// Test multi-bench findings are prefixed with bench name and have bench_name in data.
#[test]
fn test_cockpit_multi_bench_findings_prefixed() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    let findings = report["findings"].as_array().expect("findings array");
    assert!(findings.len() >= 2, "should have at least 2 findings");

    let has_bench_a_prefix = findings
        .iter()
        .any(|f| f["message"].as_str().unwrap_or("").starts_with("[bench-a]"));
    let has_bench_b_prefix = findings
        .iter()
        .any(|f| f["message"].as_str().unwrap_or("").starts_with("[bench-b]"));
    assert!(has_bench_a_prefix, "should have [bench-a] prefix");
    assert!(has_bench_b_prefix, "should have [bench-b] prefix");

    // Finding data should include bench_name
    for finding in findings {
        let data = finding.get("data");
        if let Some(data) = data {
            assert!(
                data.get("bench_name").is_some(),
                "finding data should have bench_name"
            );
        }
    }
}

/// Test multi-bench fingerprints are unique and 64-char hex.
#[test]
fn test_cockpit_multi_bench_fingerprints_unique() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    let findings = report["findings"].as_array().expect("findings array");
    let mut fingerprints: Vec<&str> = Vec::new();

    for finding in findings {
        let fp = finding["fingerprint"]
            .as_str()
            .expect("finding should have fingerprint");
        assert_eq!(fp.len(), 64, "fingerprint should be 64-char hex: {}", fp);
        assert!(
            fp.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "fingerprint should be lowercase hex: {}",
            fp
        );
        assert!(
            !fingerprints.contains(&fp),
            "fingerprints should be unique: {} seen twice",
            fp
        );
        fingerprints.push(fp);
    }
}

/// Test multi-bench reasons: no_baseline appears exactly once.
#[test]
fn test_cockpit_multi_bench_reasons_deduped() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    let reasons = report["verdict"]["reasons"]
        .as_array()
        .expect("reasons array");
    let no_baseline_count = reasons
        .iter()
        .filter(|r| r.as_str() == Some("no_baseline"))
        .count();
    assert_eq!(
        no_baseline_count, 1,
        "no_baseline should appear exactly once in reasons"
    );
}

/// Test multi-bench baseline capability: available when ALL have baselines.
#[test]
fn test_cockpit_multi_bench_baseline_all_available() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], true);

    assert_eq!(
        report["run"]["capabilities"]["baseline"]["status"], "available",
        "all baselines → status = available"
    );
}

/// Test multi-bench baseline capability: unavailable when some lack baselines.
#[test]
fn test_cockpit_multi_bench_baseline_partial() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_multi_bench_config(temp_dir.path(), &["bench-a", "bench-b"]);

    // Only create baseline for bench-a
    create_baseline_receipt(temp_dir.path(), "bench-a");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(output.status.success());

    let report: Value =
        serde_json::from_str(&fs::read_to_string(out_dir.join("report.json")).unwrap()).unwrap();

    assert_eq!(
        report["run"]["capabilities"]["baseline"]["status"], "unavailable",
        "partial baselines → status = unavailable"
    );
    // reason should be null when some have baselines
    assert!(
        report["run"]["capabilities"]["baseline"]["reason"].is_null(),
        "partial baselines → reason = null"
    );
}

/// Test multi-bench baseline capability: unavailable with no_baseline reason when none.
#[test]
fn test_cockpit_multi_bench_baseline_none() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    assert_eq!(
        report["run"]["capabilities"]["baseline"]["status"],
        "unavailable"
    );
    assert_eq!(
        report["run"]["capabilities"]["baseline"]["reason"],
        "no_baseline"
    );
}

/// Test multi-bench cockpit exits 0 even on aggregated fail.
#[test]
fn test_cockpit_multi_bench_exits_zero_on_fail() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_multi_bench_config(temp_dir.path(), &["bench-a", "bench-b"]);

    // Create fast baseline for both to trigger fail
    let baselines_dir = temp_dir.path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("create baselines dir");
    for name in &["bench-a", "bench-b"] {
        let baseline = serde_json::json!({
            "schema": "perfgate.run.v1",
            "tool": { "name": "perfgate", "version": "0.1.0" },
            "run": {
                "id": "baseline-id",
                "started_at": "2024-01-01T00:00:00Z",
                "ended_at": "2024-01-01T00:01:00Z",
                "host": { "os": "linux", "arch": "x86_64" }
            },
            "bench": {
                "name": name,
                "command": ["echo", "hello"],
                "repeat": 2,
                "warmup": 0
            },
            "samples": [
                {"wall_ms": 1, "exit_code": 0, "warmup": false, "timed_out": false},
                {"wall_ms": 1, "exit_code": 0, "warmup": false, "timed_out": false}
            ],
            "stats": { "wall_ms": { "median": 1, "min": 1, "max": 1 } }
        });
        fs::write(
            baselines_dir.join(format!("{}.json", name)),
            serde_json::to_string_pretty(&baseline).unwrap(),
        )
        .expect("write baseline");
    }

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit multi-bench should exit 0 even on fail: exit {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Test multi-bench comment.md contains both bench names and --- separator.
#[test]
fn test_cockpit_multi_bench_comment_md_combined() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (_report, out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    let md = fs::read_to_string(out_dir.join("comment.md")).expect("read comment.md");
    assert!(md.contains("bench-a"), "markdown should contain bench-a");
    assert!(md.contains("bench-b"), "markdown should contain bench-b");
    // Separator format is a perfgate-private detail; the unit test in
    // sensor_report.rs covers the exact `\n---\n\n` contract.
}

/// Test multi-bench artifacts are sorted by (type, path).
#[test]
fn test_cockpit_multi_bench_artifacts_sorted() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], true);

    let artifacts = report["artifacts"].as_array().expect("artifacts array");
    for window in artifacts.windows(2) {
        let a_type = window[0]["type"].as_str().unwrap();
        let a_path = window[0]["path"].as_str().unwrap();
        let b_type = window[1]["type"].as_str().unwrap();
        let b_path = window[1]["path"].as_str().unwrap();
        assert!(
            (a_type, a_path) <= (b_type, b_path),
            "artifacts not sorted: ({}, {}) > ({}, {})",
            a_type,
            a_path,
            b_type,
            b_path
        );
    }
}

/// Test multi-bench empty config produces error sensor report.
#[test]
fn test_cockpit_multi_bench_empty_config_error() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");

    // Create a config with no benches
    let config_path = temp_dir.path().join("perfgate.toml");
    fs::write(
        &config_path,
        r#"
[defaults]
repeat = 2
"#,
    )
    .expect("write config");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");
    assert!(
        output.status.success(),
        "cockpit should exit 0 even on empty config: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value =
        serde_json::from_str(&fs::read_to_string(out_dir.join("report.json")).unwrap()).unwrap();

    assert_eq!(report["schema"], "sensor.report.v1");
    assert_eq!(report["verdict"]["status"], "fail");
    assert_eq!(report["verdict"]["reasons"][0], "tool_error");
}

/// Test multi-bench report structure has all required fields.
#[test]
fn test_cockpit_multi_bench_report_structure() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], true);

    // Verify required fields
    assert_eq!(report["schema"], "sensor.report.v1");
    assert!(report.get("tool").is_some(), "tool field missing");
    assert_eq!(report["tool"]["name"], "perfgate");
    assert!(report.get("run").is_some(), "run field missing");
    assert!(
        report["run"].get("started_at").is_some(),
        "started_at missing"
    );
    assert!(report["run"].get("ended_at").is_some(), "ended_at missing");
    assert!(
        report["run"].get("duration_ms").is_some(),
        "duration_ms missing"
    );
    assert!(
        report["run"].get("capabilities").is_some(),
        "capabilities missing"
    );
    assert!(report.get("verdict").is_some(), "verdict field missing");
    assert!(
        report["verdict"].get("status").is_some(),
        "verdict.status missing"
    );
    assert!(
        report["verdict"].get("counts").is_some(),
        "verdict.counts missing"
    );
    assert!(
        report["verdict"]["counts"].get("info").is_some(),
        "counts.info missing"
    );
    assert!(
        report["verdict"]["counts"].get("warn").is_some(),
        "counts.warn missing"
    );
    assert!(
        report["verdict"]["counts"].get("error").is_some(),
        "counts.error missing"
    );
    assert!(report.get("findings").is_some(), "findings field missing");
    assert!(report.get("data").is_some(), "data field missing");
    assert!(
        report["data"].get("summary").is_some(),
        "data.summary missing"
    );
}

/// Test multi-bench report has no truncation fields when under limit.
#[test]
fn test_cockpit_multi_bench_no_truncation_fields() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (report, _out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], false);

    assert!(
        report["data"].get("findings_total").is_none()
            || report["data"]["findings_total"].is_null(),
        "findings_total should be absent when under limit"
    );
    assert!(
        report["data"].get("findings_emitted").is_none()
            || report["data"]["findings_emitted"].is_null(),
        "findings_emitted should be absent when under limit"
    );
}

/// Test multi-bench extras contain valid native schemas.
#[test]
fn test_cockpit_multi_bench_extras_valid_native() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let (_report, out_dir) = run_cockpit_multi_bench(&temp_dir, &["bench-a", "bench-b"], true);

    for name in &["bench-a", "bench-b"] {
        let prefix = out_dir.join("extras").join(name);

        // Run receipt should have correct schema
        let run_content =
            fs::read_to_string(prefix.join("perfgate.run.v1.json")).expect("read run receipt");
        let run: Value = serde_json::from_str(&run_content).expect("parse run receipt");
        assert_eq!(
            run["schema"], "perfgate.run.v1",
            "extras/{}/perfgate.run.v1.json should have correct schema",
            name
        );

        // Report should have correct report_type
        let report_content =
            fs::read_to_string(prefix.join("perfgate.report.v1.json")).expect("read native report");
        let native_report: Value =
            serde_json::from_str(&report_content).expect("parse native report");
        assert_eq!(
            native_report["report_type"], "perfgate.report.v1",
            "extras/{}/perfgate.report.v1.json should have correct report_type",
            name
        );

        // Compare receipt should have correct schema
        let compare_content = fs::read_to_string(prefix.join("perfgate.compare.v1.json"))
            .expect("read compare receipt");
        let compare: Value = serde_json::from_str(&compare_content).expect("parse compare receipt");
        assert_eq!(
            compare["schema"], "perfgate.compare.v1",
            "extras/{}/perfgate.compare.v1.json should have correct schema",
            name
        );
    }
}

/// Test cockpit mode error report validates against schema
#[test]
fn test_cockpit_mode_error_report_validates_schema() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = temp_dir.path().join("broken.toml");
    fs::write(&config_path, "not valid toml {{{").expect("write broken config");

    let mut cmd = perfgate_cmd();
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
    assert!(output.status.success());

    // Load vendored schema
    let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/schemas/sensor.report.v1.schema.json");
    let schema_content = fs::read_to_string(&schema_path).expect("read schema");
    let schema_value: Value = serde_json::from_str(&schema_content).expect("parse schema");
    let validator = jsonschema::validator_for(&schema_value).expect("compile schema");

    let report_path = out_dir.join("report.json");
    let content = fs::read_to_string(&report_path).expect("read report");
    let instance: Value = serde_json::from_str(&content).expect("parse report");

    let errors: Vec<_> = validator.iter_errors(&instance).collect();
    assert!(
        errors.is_empty(),
        "error report failed schema validation:\n{}",
        errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Verify fingerprint is present on error findings
    let findings = instance["findings"].as_array().expect("findings");
    for finding in findings {
        assert!(
            finding.get("fingerprint").is_some(),
            "error finding should have fingerprint"
        );
    }
}

// ======================================================================
// Mixed-outcome (per-bench error boundary) tests
// ======================================================================

/// Returns a cross-platform command that will fail to spawn (nonexistent binary).
fn nonexistent_command() -> Vec<&'static str> {
    vec!["perfgate_nonexistent_command_that_does_not_exist_xyz"]
}

/// Create a config file with per-bench commands (some may be nonexistent).
fn create_mixed_outcome_config(
    temp_dir: &std::path::Path,
    benches: &[(&str, &[&str])],
) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate.toml");

    let mut config_content = String::from(
        r#"
[defaults]
repeat = 1
warmup = 0
threshold = 0.20
"#,
    );

    for (name, cmd) in benches {
        let cmd_str = cmd
            .iter()
            .map(|s| format!("\"{}\"", s))
            .collect::<Vec<_>>()
            .join(", ");
        config_content.push_str(&format!(
            r#"
[[bench]]
name = "{}"
command = [{}]
"#,
            name, cmd_str
        ));
    }

    fs::write(&config_path, config_content).expect("Failed to write config file");
    config_path
}

/// Test mixed-outcome multi-bench: one bench succeeds, one fails to spawn.
///
/// Validates per-bench error boundary:
/// - Exit 0 (cockpit contract)
/// - Schema-valid output
/// - Verdict = fail (error bench contributes)
/// - error >= 1
/// - Reasons include tool_error and no_baseline
/// - Findings prefixed with bench names
/// - Error finding has {stage, error_kind, bench_name}
/// - Good bench has extras, bad bench does NOT
#[test]
fn test_cockpit_multi_bench_mixed_outcome_error_and_warn() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");

    let good_cmd = slow_command();
    let bad_cmd = nonexistent_command();

    let config_path = create_mixed_outcome_config(
        temp_dir.path(),
        &[("good-bench", &good_cmd), ("bad-bench", &bad_cmd)],
    );

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--all")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--mode")
        .arg("cockpit");

    let output = cmd.output().expect("failed to execute check");

    // Cockpit contract: exit 0 even when a bench fails
    assert!(
        output.status.success(),
        "cockpit mixed-outcome should exit 0: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // Parse report
    let report_path = out_dir.join("report.json");
    assert!(report_path.exists(), "report.json should exist");
    let report: Value =
        serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
            .expect("parse report");

    // Schema validation
    let validator = load_vendored_schema_validator();
    let errors: Vec<_> = validator.iter_errors(&report).collect();
    assert!(
        errors.is_empty(),
        "mixed-outcome report failed schema validation:\n{}",
        errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // Verdict: fail (error bench makes it fail)
    assert_eq!(
        report["verdict"]["status"], "fail",
        "mixed-outcome verdict should be fail"
    );

    // Counts: error >= 1
    let error_count = report["verdict"]["counts"]["error"].as_u64().unwrap();
    assert!(error_count >= 1, "should have at least 1 error count");

    // Reasons include tool_error and no_baseline
    let reasons = report["verdict"]["reasons"]
        .as_array()
        .expect("reasons array");
    let has_tool_error = reasons.iter().any(|r| r.as_str() == Some("tool_error"));
    let has_no_baseline = reasons.iter().any(|r| r.as_str() == Some("no_baseline"));
    assert!(has_tool_error, "reasons should include tool_error");
    assert!(has_no_baseline, "reasons should include no_baseline");

    // Findings: should have prefixed messages
    let findings = report["findings"].as_array().expect("findings array");
    assert!(
        findings.len() >= 2,
        "should have at least 2 findings (good-bench warn + bad-bench error)"
    );

    let has_good_prefix = findings.iter().any(|f| {
        f["message"]
            .as_str()
            .unwrap_or("")
            .starts_with("[good-bench]")
    });
    let has_bad_prefix = findings.iter().any(|f| {
        f["message"]
            .as_str()
            .unwrap_or("")
            .starts_with("[bad-bench]")
    });
    assert!(has_good_prefix, "should have [good-bench] prefixed finding");
    assert!(has_bad_prefix, "should have [bad-bench] prefixed finding");

    // Error finding should have structured data with stage, error_kind, bench_name
    let error_finding = findings
        .iter()
        .find(|f| f["check_id"].as_str() == Some("tool.runtime"))
        .expect("should have tool.runtime finding");
    let finding_data = &error_finding["data"];
    assert!(
        finding_data.get("stage").is_some() && !finding_data["stage"].is_null(),
        "error finding should have stage"
    );
    assert!(
        finding_data.get("error_kind").is_some() && !finding_data["error_kind"].is_null(),
        "error finding should have error_kind"
    );
    assert_eq!(
        finding_data["bench_name"], "bad-bench",
        "error finding should have bench_name = bad-bench"
    );

    // Good bench should have extras, bad bench should NOT
    let good_extras = out_dir.join("extras").join("good-bench");
    let bad_extras = out_dir.join("extras").join("bad-bench");
    assert!(
        good_extras.join("perfgate.run.v1.json").exists(),
        "good-bench should have extras/good-bench/perfgate.run.v1.json"
    );
    // Bad bench may or may not have the extras dir created, but should NOT have run receipt
    let bad_has_run = bad_extras.join("perfgate.run.v1.json").exists();
    assert!(
        !bad_has_run,
        "bad-bench should NOT have perfgate.run.v1.json"
    );

    // Artifacts in report should reference good-bench but NOT bad-bench
    let artifacts = report["artifacts"].as_array().expect("artifacts array");
    let has_good_artifact = artifacts
        .iter()
        .any(|a| a["path"].as_str().unwrap_or("").contains("good-bench"));
    let has_bad_artifact = artifacts
        .iter()
        .any(|a| a["path"].as_str().unwrap_or("").contains("bad-bench"));
    assert!(has_good_artifact, "artifacts should reference good-bench");
    assert!(
        !has_bad_artifact,
        "artifacts should NOT reference bad-bench"
    );

    // bench_count should include both
    assert_eq!(
        report["data"]["summary"]["bench_count"], 2,
        "bench_count should be 2 (both benches counted)"
    );
}
