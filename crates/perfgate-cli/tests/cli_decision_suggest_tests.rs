//! Integration tests for `perfgate decision suggest`.

use std::fs;
use std::path::Path;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

fn write_basic_config(root: &Path) -> std::path::PathBuf {
    let config_path = root.join("perfgate.toml");
    fs::write(
        &config_path,
        r#"
[defaults]
out_dir = "artifacts/perfgate"

[[bench]]
name = "parser"
command = ["echo", "parser"]
"#,
    )
    .expect("write config");
    config_path
}

fn write_decision_ready_config(root: &Path) -> std::path::PathBuf {
    let config_path = root.join("perfgate.toml");
    fs::write(
        &config_path,
        r#"
[defaults]
out_dir = "artifacts/perfgate"

[[bench]]
name = "parser"
command = ["echo", "parser"]

[[scenario]]
name = "release"
bench = "parser"
weight = 1.0
compare = "artifacts/perfgate/parser/compare.json"
probe_baseline = "artifacts/perfgate/parser/probes-baseline.json"
probe_current = "artifacts/perfgate/parser/probes-current.json"
probe_compare = "artifacts/perfgate/parser/probe-compare.json"

[[tradeoff]]
name = "parser tradeoff"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
min_improvement_ratio = 1.05

[[tradeoff.allow]]
probe = "parser.tokenize"
metric = "wall_ms"
max_regression = 0.03
"#,
    )
    .expect("write config");
    config_path
}

fn write_compare_receipt(path: &Path, status: &str, baseline: f64, current: f64) {
    let regression = if current > baseline {
        (current - baseline) / baseline
    } else {
        0.0
    };
    write_compare_receipt_for_metric(path, "wall_ms", status, baseline, current, regression);
}

fn write_compare_receipt_for_metric(
    path: &Path,
    metric: &str,
    status: &str,
    baseline: f64,
    current: f64,
    regression: f64,
) {
    write_compare_receipt_for_metric_with_noise(
        path,
        metric,
        status,
        baseline,
        current,
        regression,
        (None, None),
    );
}

fn write_compare_receipt_for_metric_with_noise(
    path: &Path,
    metric: &str,
    status: &str,
    baseline: f64,
    current: f64,
    regression: f64,
    noise: (Option<f64>, Option<f64>),
) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create compare parent");
    }
    let fail = u32::from(status == "fail");
    let warn = u32::from(status == "warn");
    let pass = u32::from(status == "pass");
    let receipt = serde_json::json!({
        "schema": "perfgate.compare.v1",
        "tool": {"name": "perfgate", "version": "0.18.0"},
        "bench": {
            "name": "parser",
            "command": ["echo", "parser"],
            "repeat": 1,
            "warmup": 0
        },
        "baseline_ref": {"path": "baselines/parser.json", "run_id": "baseline"},
        "current_ref": {"path": "artifacts/perfgate/parser/run.json", "run_id": "current"},
        "budgets": {},
        "deltas": {
            metric: {
                "baseline": baseline,
                "current": current,
                "ratio": current / baseline,
                "pct": (current - baseline) / baseline,
                "regression": regression,
                "cv": noise.0,
                "noise_threshold": noise.1,
                "status": status
            }
        },
        "verdict": {
            "status": status,
            "counts": {"pass": pass, "warn": warn, "fail": fail, "skip": 0},
            "reasons": []
        }
    });
    fs::write(path, serde_json::to_string_pretty(&receipt).unwrap()).expect("write compare");
}

#[test]
fn decision_suggest_tells_user_to_run_local_gate_first_without_compares() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_basic_config(temp_dir.path());

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("decision")
        .arg("suggest")
        .arg("--config")
        .arg(&config_path);

    let output = cmd.output().expect("run decision suggest");
    assert!(
        output.status.success(),
        "decision suggest should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Status: run_local_gate_first"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("Why:"), "stdout: {stdout}");
    assert!(
        stdout.contains("local gate evidence must exist first"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("Artifacts:"), "stdout: {stdout}");
    assert!(
        stdout.contains("no compare receipts found"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("perfgate check --config"),
        "stdout: {stdout}"
    );
}

#[test]
fn decision_suggest_detects_throughput_improvement_as_structured_candidate() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_basic_config(temp_dir.path());
    let out_dir = temp_dir.path().join("artifacts").join("perfgate");
    write_compare_receipt_for_metric(
        &out_dir.join("parser").join("compare.json"),
        "throughput_per_s",
        "pass",
        100.0,
        120.0,
        0.0,
    );

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("decision")
        .arg("suggest")
        .arg("--config")
        .arg(&config_path);

    let output = cmd.output().expect("run decision suggest");
    assert!(
        output.status.success(),
        "decision suggest should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Status: structured_decision_candidate"),
        "expected throughput improvement to flag structured_decision_candidate, stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "throughput_per_s improved by 20.0% (direction higher_is_better, threshold status pass)"
        ),
        "expected direction-aware reason line, stdout: {stdout}"
    );
    assert!(
        stdout
            .contains("metric movement exists, but scenario/tradeoff/probe evidence is incomplete"),
        "expected structured-decision reason, stdout: {stdout}"
    );
}

#[test]
fn decision_suggest_detects_throughput_regression_as_structured_candidate() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_basic_config(temp_dir.path());
    let out_dir = temp_dir.path().join("artifacts").join("perfgate");
    write_compare_receipt_for_metric(
        &out_dir.join("parser").join("compare.json"),
        "throughput_per_s",
        "fail",
        100.0,
        80.0,
        0.20,
    );

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("decision")
        .arg("suggest")
        .arg("--config")
        .arg(&config_path);

    let output = cmd.output().expect("run decision suggest");
    assert!(
        output.status.success(),
        "decision suggest should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Status: structured_decision_candidate"),
        "expected throughput regression to flag structured_decision_candidate, stdout: {stdout}"
    );
    assert!(
        stdout.contains(
            "throughput_per_s regressed by 20.0% (direction higher_is_better, threshold status fail)"
        ),
        "expected direction-aware regression reason line, stdout: {stdout}"
    );
}

#[test]
fn decision_suggest_explains_paired_mode_when_compare_is_noisy() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_basic_config(temp_dir.path());
    let out_dir = temp_dir.path().join("artifacts").join("perfgate");
    write_compare_receipt_for_metric_with_noise(
        &out_dir.join("parser").join("compare.json"),
        "wall_ms",
        "warn",
        100.0,
        104.0,
        0.04,
        (Some(0.18), Some(0.10)),
    );

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("decision")
        .arg("suggest")
        .arg("--config")
        .arg(&config_path);

    let output = cmd.output().expect("run decision suggest");
    assert!(
        output.status.success(),
        "decision suggest should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Status: paired_mode_recommended"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("at least one metric has CV above 10.0%"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("wall_ms noise is high: CV 18.0% exceeds 10.0%"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("perfgate paired"), "stdout: {stdout}");
}

#[test]
fn decision_suggest_reports_structured_decision_ready_when_policy_and_evidence_exist() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_decision_ready_config(temp_dir.path());
    let out_dir = temp_dir.path().join("artifacts").join("perfgate");
    write_compare_receipt(
        &out_dir.join("parser").join("compare.json"),
        "fail",
        100.0,
        115.0,
    );
    fs::write(out_dir.join("parser").join("probes-baseline.json"), "{}").expect("write probe");
    fs::write(out_dir.join("parser").join("probes-current.json"), "{}").expect("write probe");
    fs::write(out_dir.join("parser").join("probe-compare.json"), "{}").expect("write probe");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("decision")
        .arg("suggest")
        .arg("--config")
        .arg(&config_path);

    let output = cmd.output().expect("run decision suggest");
    assert!(
        output.status.success(),
        "decision suggest should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Status: structured_decision_ready"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("perfgate decision evaluate --config"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("probe evidence: configured, 3 receipts"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("1 scenario and 1 tradeoff rule are configured"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("optional ledger history can record the decision"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("probe: artifacts/perfgate/parser/probes-baseline.json"),
        "stdout: {stdout}"
    );
}
