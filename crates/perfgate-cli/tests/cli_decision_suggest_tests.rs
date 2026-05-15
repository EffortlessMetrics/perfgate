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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create compare parent");
    }
    let fail = u32::from(status == "fail");
    let warn = u32::from(status == "warn");
    let pass = u32::from(status == "pass");
    let regression = if current > baseline {
        (current - baseline) / baseline
    } else {
        0.0
    };
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
            "wall_ms": {
                "baseline": baseline,
                "current": current,
                "ratio": current / baseline,
                "pct": (current - baseline) / baseline,
                "regression": regression,
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
}
