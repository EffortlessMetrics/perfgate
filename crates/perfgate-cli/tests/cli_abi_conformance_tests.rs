//! ABI conformance tests for sensor.report.v1
//!
//! Validates:
//! - Artifact ordering is sorted by (type, path)
//! - Data opacity: `data` has no `compare` key
//! - Error convention: config error → tool.runtime + runtime_error + structured data
//! - Extras files use versioned names
//! - Schema validation of golden fixtures against vendored schema
//! - Determinism of cockpit output

mod common;

use common::perfgate_cmd;
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

/// Load the vendored schema and compile a validator.
fn load_vendored_schema_validator() -> jsonschema::Validator {
    let schema_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../contracts/schemas/sensor.report.v1.schema.json");
    let schema_content = fs::read_to_string(&schema_path).expect("read vendored schema");
    let schema_value: Value = serde_json::from_str(&schema_content).expect("parse vendored schema");
    jsonschema::validator_for(&schema_value).expect("compile schema")
}

/// Validate a JSON value against the vendored schema.
fn validate_against_schema(validator: &jsonschema::Validator, instance: &Value, name: &str) {
    let errors: Vec<_> = validator.iter_errors(instance).collect();
    assert!(
        errors.is_empty(),
        "{} failed schema validation:\n{}",
        name,
        errors
            .iter()
            .map(|e| format!("  - {}", e))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Test a basic cockpit check passes.
#[test]
fn test_cockpit_basic_pass() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config(temp_dir.path(), &["test-bench"]);

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
    if !output.status.success() {
        eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(output.status.success(), "cockpit mode should exit 0");

    let report_path = out_dir.join("report.json");
    assert!(report_path.exists(), "report.json should exist");
}

/// Test that artifacts are sorted by (type, path)
#[test]
fn test_artifact_ordering_sorted() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config(temp_dir.path(), &["test-bench"]);

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
    if !output.status.success() {
        eprintln!("STDOUT: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
    }
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

    let mut cmd = perfgate_cmd();
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

// --- Schema validation of golden fixtures ---

#[test]
fn test_golden_pass_validates_against_schema() {
    let validator = load_vendored_schema_validator();
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/sensor_report_pass.json");
    let content = fs::read_to_string(&fixture_path).expect("read pass fixture");
    let instance: Value = serde_json::from_str(&content).expect("parse pass fixture");
    validate_against_schema(&validator, &instance, "sensor_report_pass.json");
}

#[test]
fn test_golden_fail_validates_against_schema() {
    let validator = load_vendored_schema_validator();
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/sensor_report_fail.json");
    let content = fs::read_to_string(&fixture_path).expect("read fail fixture");
    let instance: Value = serde_json::from_str(&content).expect("parse fail fixture");
    validate_against_schema(&validator, &instance, "sensor_report_fail.json");
}

#[test]
fn test_golden_warn_validates_against_schema() {
    let validator = load_vendored_schema_validator();
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/sensor_report_warn.json");
    let content = fs::read_to_string(&fixture_path).expect("read warn fixture");
    let instance: Value = serde_json::from_str(&content).expect("parse warn fixture");
    validate_against_schema(&validator, &instance, "sensor_report_warn.json");
}

#[test]
fn test_golden_no_baseline_validates_against_schema() {
    let validator = load_vendored_schema_validator();
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/sensor_report_no_baseline.json");
    let content = fs::read_to_string(&fixture_path).expect("read no_baseline fixture");
    let instance: Value = serde_json::from_str(&content).expect("parse no_baseline fixture");
    validate_against_schema(&validator, &instance, "sensor_report_no_baseline.json");
}

#[test]
fn test_golden_error_validates_against_schema() {
    let validator = load_vendored_schema_validator();
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/sensor_report_error.json");
    let content = fs::read_to_string(&fixture_path).expect("read error fixture");
    let instance: Value = serde_json::from_str(&content).expect("parse error fixture");
    validate_against_schema(&validator, &instance, "sensor_report_error.json");
}

#[test]
fn test_golden_multi_bench_validates_against_schema() {
    let validator = load_vendored_schema_validator();
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/sensor_report_multi_bench.json");
    let content = fs::read_to_string(&fixture_path).expect("read multi_bench fixture");
    let instance: Value = serde_json::from_str(&content).expect("parse multi_bench fixture");
    validate_against_schema(&validator, &instance, "sensor_report_multi_bench.json");
}

/// Test that cockpit mode output validates against vendored schema
#[test]
fn test_cockpit_output_validates_against_vendored_schema() {
    let validator = load_vendored_schema_validator();

    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config(temp_dir.path(), &["test-bench"]);

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
    let content = fs::read_to_string(&report_path).expect("read cockpit report");
    let instance: Value = serde_json::from_str(&content).expect("parse cockpit report");
    validate_against_schema(&validator, &instance, "cockpit output report.json");
}

/// Test that error report validates against vendored schema
#[test]
fn test_cockpit_error_report_validates_against_schema() {
    let validator = load_vendored_schema_validator();

    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = temp_dir.path().join("bad.toml");
    fs::write(&config_path, "not valid toml {{{").expect("write bad config");

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

    let report_path = out_dir.join("report.json");
    let content = fs::read_to_string(&report_path).expect("read error report");
    let instance: Value = serde_json::from_str(&content).expect("parse error report");
    validate_against_schema(&validator, &instance, "cockpit error report");
}

/// Test determinism: run cockpit twice, null out time-varying fields, assert structural identity
#[test]
fn test_cockpit_mode_determinism() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let config_path = create_config(temp_dir.path(), &["det-bench"]);

    let mut reports: Vec<Value> = Vec::new();

    for i in 0..2 {
        let out_dir = temp_dir.path().join(format!("run{}", i));

        let mut cmd = perfgate_cmd();
        cmd.current_dir(temp_dir.path())
            .arg("check")
            .arg("--config")
            .arg(&config_path)
            .arg("--bench")
            .arg("det-bench")
            .arg("--out-dir")
            .arg(&out_dir)
            .arg("--mode")
            .arg("cockpit");

        let output = cmd.output().expect("failed to execute check");
        assert!(output.status.success());

        let report_path = out_dir.join("report.json");
        let mut report: Value =
            serde_json::from_str(&fs::read_to_string(&report_path).expect("read report"))
                .expect("parse report");

        // Null out time-varying fields
        report["run"]["started_at"] = Value::Null;
        report["run"]["ended_at"] = Value::Null;
        report["run"]["duration_ms"] = Value::Null;
        report["tool"]["version"] = Value::Null;

        reports.push(report);
    }

    assert_eq!(
        reports[0]["schema"], reports[1]["schema"],
        "schema should be identical"
    );
    assert_eq!(
        reports[0]["verdict"]["status"], reports[1]["verdict"]["status"],
        "verdict status should be identical"
    );
    assert_eq!(
        reports[0]["artifacts"], reports[1]["artifacts"],
        "artifacts should be identical"
    );
    assert_eq!(
        reports[0]["findings"].as_array().map(|a| a.len()),
        reports[1]["findings"].as_array().map(|a| a.len()),
        "findings count should be identical"
    );
}

/// Test that findings have fingerprints in cockpit output
#[test]
fn test_cockpit_output_has_fingerprints() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let config_path = create_config(temp_dir.path(), &["fp-bench"]);
    // No baseline → findings with fingerprints

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("fp-bench")
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

    let findings = report["findings"].as_array().expect("findings array");
    for finding in findings {
        assert!(
            finding.get("fingerprint").is_some(),
            "finding should have fingerprint: {:?}",
            finding
        );
        let fp = finding["fingerprint"].as_str().unwrap();
        assert_eq!(
            fp.len(),
            64,
            "fingerprint should be 64 chars, got {} chars: {}",
            fp.len(),
            fp
        );
        assert!(
            fp.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "fingerprint should be lowercase hex, got: {}",
            fp
        );
    }
}
