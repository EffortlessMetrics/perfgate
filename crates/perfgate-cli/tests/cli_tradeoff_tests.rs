//! Integration tests for `perfgate tradeoff`.

mod common;

use common::perfgate_cmd;
use predicates::prelude::*;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_tradeoff_evaluate_writes_tradeoff_receipt() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("perfgate.toml");
    let scenario_path = temp_dir.path().join("scenario.json");
    let output_path = temp_dir.path().join("tradeoff.json");

    write_tradeoff_config(&config_path, 1.10, "warn");
    write_scenario_receipt(&scenario_path, 80.0, "fail");

    perfgate_cmd()
        .arg("tradeoff")
        .arg("evaluate")
        .arg("--config")
        .arg(&config_path)
        .arg("--scenario")
        .arg(&scenario_path)
        .arg("--out")
        .arg(&output_path)
        .assert()
        .success()
        .stderr(predicate::str::contains("Tradeoff receipt written"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read tradeoff receipt"),
    )
    .expect("tradeoff receipt should be JSON");
    let typed: perfgate_types::TradeoffReceipt =
        serde_json::from_value(receipt.clone()).expect("tradeoff receipt should deserialize");

    assert_eq!(typed.schema, "perfgate.tradeoff.v1");
    assert_eq!(typed.scenario.as_deref(), Some("release_workload"));
    assert!(typed.decision.accepted_tradeoff);
    assert_eq!(typed.verdict.status, perfgate_types::VerdictStatus::Warn);
    assert_eq!(
        receipt["weighted_deltas"]["max_rss_kb"]["status"].as_str(),
        Some("warn")
    );
    assert_eq!(receipt["rules"][0]["status"].as_str(), Some("accepted"));
    assert_eq!(receipt["rules"][0]["requirements"][0]["satisfied"], true);
}

#[test]
fn test_tradeoff_evaluate_fails_when_rule_not_satisfied() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("perfgate.toml");
    let scenario_path = temp_dir.path().join("scenario.json");
    let output_path = temp_dir.path().join("tradeoff.json");

    write_tradeoff_config(&config_path, 1.20, "pass");
    write_scenario_receipt(&scenario_path, 96.0, "fail");

    perfgate_cmd()
        .arg("tradeoff")
        .arg("evaluate")
        .arg("--config")
        .arg(&config_path)
        .arg("--scenario")
        .arg(&scenario_path)
        .arg("--out")
        .arg(&output_path)
        .assert()
        .code(2);

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read tradeoff receipt"),
    )
    .expect("tradeoff receipt should be JSON");

    assert_eq!(receipt["schema"], "perfgate.tradeoff.v1");
    assert_eq!(receipt["decision"]["accepted_tradeoff"], false);
    assert_eq!(receipt["rules"][0]["status"], "rejected");
    assert!(
        receipt["verdict"]["reasons"]
            .as_array()
            .expect("reasons array")
            .iter()
            .any(|reason| reason == "tradeoff_rule_not_satisfied")
    );
}

#[test]
fn test_tradeoff_evaluate_uses_probe_requirement() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("perfgate.toml");
    let scenario_path = temp_dir.path().join("scenario.json");
    let probe_compare_path = temp_dir.path().join("probe-compare.json");
    let output_path = temp_dir.path().join("tradeoff.json");

    write_probe_tradeoff_config(&config_path, 1.10, "warn");
    write_probe_compare_receipt(&probe_compare_path, "parser.batch_loop", 80.0);
    write_scenario_receipt_with_probe_ref(&scenario_path, 96.0, "fail", &probe_compare_path);

    perfgate_cmd()
        .arg("tradeoff")
        .arg("evaluate")
        .arg("--config")
        .arg(&config_path)
        .arg("--scenario")
        .arg(&scenario_path)
        .arg("--out")
        .arg(&output_path)
        .assert()
        .success()
        .stderr(predicate::str::contains("Tradeoff receipt written"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read tradeoff receipt"),
    )
    .expect("tradeoff receipt should be JSON");
    let typed: perfgate_types::TradeoffReceipt =
        serde_json::from_value(receipt.clone()).expect("tradeoff receipt should deserialize");

    assert!(typed.decision.accepted_tradeoff);
    assert_eq!(typed.verdict.status, perfgate_types::VerdictStatus::Warn);
    assert_eq!(
        typed.rules[0].requirements[0].probe.as_deref(),
        Some("parser.batch_loop")
    );
    assert_eq!(typed.rules[0].requirements[0].observed_change, Some(-0.20));
    assert_eq!(typed.probes[0].name, "parser.batch_loop");
}

#[test]
fn test_tradeoff_evaluate_enforces_local_regression_cap() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("perfgate.toml");
    let scenario_path = temp_dir.path().join("scenario.json");
    let probe_compare_path = temp_dir.path().join("probe-compare.json");
    let output_path = temp_dir.path().join("tradeoff.json");

    write_probe_tradeoff_config_with_allow(&config_path, 1.10, "warn", 0.03);
    write_probe_compare_receipt_many(
        &probe_compare_path,
        &[
            ("parser.batch_loop", 80.0, "dominant"),
            ("parser.tokenize", 105.0, "local"),
        ],
    );
    write_scenario_receipt_with_probe_ref(&scenario_path, 96.0, "fail", &probe_compare_path);

    perfgate_cmd()
        .arg("tradeoff")
        .arg("evaluate")
        .arg("--config")
        .arg(&config_path)
        .arg("--scenario")
        .arg(&scenario_path)
        .arg("--out")
        .arg(&output_path)
        .assert()
        .code(2);

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read tradeoff receipt"),
    )
    .expect("tradeoff receipt should be JSON");

    assert_eq!(receipt["decision"]["accepted_tradeoff"], false);
    assert_eq!(receipt["rules"][0]["status"], "rejected");
    assert_eq!(
        receipt["rules"][0]["allowances"][0]["probe"],
        "parser.tokenize"
    );
    assert_eq!(
        receipt["rules"][0]["allowances"][0]["satisfied"].as_bool(),
        Some(false)
    );
    assert!(
        receipt["rules"][0]["allowances"][0]["reason"]
            .as_str()
            .expect("allowance reason")
            .contains("exceeds cap")
    );
}

#[test]
fn test_tradeoff_evaluate_rejects_config_without_tradeoffs() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let config_path = temp_dir.path().join("perfgate.toml");
    let scenario_path = temp_dir.path().join("scenario.json");

    fs::write(&config_path, "[defaults]\nthreshold = 0.20\n").expect("failed to write config");
    write_scenario_receipt(&scenario_path, 80.0, "fail");

    perfgate_cmd()
        .arg("tradeoff")
        .arg("evaluate")
        .arg("--config")
        .arg(&config_path)
        .arg("--scenario")
        .arg(&scenario_path)
        .arg("--out")
        .arg(temp_dir.path().join("tradeoff.json"))
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "no [[tradeoff]] entries configured",
        ));
}

fn write_tradeoff_config(path: &Path, min_improvement_ratio: f64, downgrade_to: &str) {
    fs::write(
        path,
        format!(
            r#"[[tradeoff]]
name = "memory_for_speed"
if_failed = "max_rss_kb"
downgrade_to = "{downgrade_to}"

[[tradeoff.require]]
metric = "wall_ms"
min_improvement_ratio = {min_improvement_ratio}
"#
        ),
    )
    .expect("failed to write config");
}

fn write_probe_tradeoff_config(path: &Path, min_improvement_ratio: f64, downgrade_to: &str) {
    fs::write(
        path,
        format!(
            r#"[[tradeoff]]
name = "memory_for_probe_speed"
if_failed = "max_rss_kb"
downgrade_to = "{downgrade_to}"

[[tradeoff.require]]
metric = "wall_ms"
probe = "parser.batch_loop"
min_improvement_ratio = {min_improvement_ratio}
"#
        ),
    )
    .expect("failed to write config");
}

fn write_probe_tradeoff_config_with_allow(
    path: &Path,
    min_improvement_ratio: f64,
    downgrade_to: &str,
    max_regression: f64,
) {
    fs::write(
        path,
        format!(
            r#"[[tradeoff]]
name = "memory_for_probe_speed"
if_failed = "max_rss_kb"
downgrade_to = "{downgrade_to}"

[[tradeoff.require]]
metric = "wall_ms"
probe = "parser.batch_loop"
min_improvement_ratio = {min_improvement_ratio}

[[tradeoff.allow]]
metric = "wall_ms"
probe = "parser.tokenize"
max_regression = {max_regression}
"#
        ),
    )
    .expect("failed to write config");
}

fn write_scenario_receipt(path: &Path, wall_current: f64, memory_status: &str) {
    write_scenario_receipt_inner(path, wall_current, memory_status, None);
}

fn write_scenario_receipt_with_probe_ref(
    path: &Path,
    wall_current: f64,
    memory_status: &str,
    probe_compare_path: &Path,
) {
    write_scenario_receipt_inner(path, wall_current, memory_status, Some(probe_compare_path));
}

fn write_scenario_receipt_inner(
    path: &Path,
    wall_current: f64,
    memory_status: &str,
    probe_compare_path: Option<&Path>,
) {
    let memory_fail = u32::from(memory_status == "fail");
    let memory_warn = u32::from(memory_status == "warn");
    let memory_pass = u32::from(memory_status == "pass");
    let reasons: Vec<&str> = if memory_status == "fail" {
        vec!["max_rss_kb_fail"]
    } else if memory_status == "warn" {
        vec!["max_rss_kb_warn"]
    } else {
        Vec::new()
    };
    let components = probe_compare_path
        .map(|path| {
            json!([{
                "name": "large_file_parse",
                "weight": 1.0,
                "benchmark": "large-file",
                "probe_compare_ref": {
                    "path": path.display().to_string(),
                    "run_id": "probe-compare-run"
                },
                "deltas": {
                    "wall_ms": {
                        "baseline": 100.0,
                        "current": wall_current,
                        "ratio": wall_current / 100.0,
                        "pct": (wall_current - 100.0) / 100.0,
                        "regression": 0.0,
                        "status": "pass"
                    }
                },
                "probes": ["parser.batch_loop"],
                "status": "pass"
            }])
        })
        .unwrap_or_else(|| json!([]));
    let receipt = json!({
        "schema": "perfgate.scenario.v1",
        "tool": {"name": "perfgate", "version": "0.16.0"},
        "run": {
            "id": "scenario-run",
            "started_at": "2026-05-08T00:00:00Z",
            "ended_at": "2026-05-08T00:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "scenario": {
            "name": "release_workload",
            "weight": 1.0
        },
        "components": components,
        "weighted_deltas": {
            "wall_ms": {
                "baseline": 100.0,
                "current": wall_current,
                "ratio": wall_current / 100.0,
                "pct": (wall_current - 100.0) / 100.0,
                "regression": 0.0,
                "status": "pass"
            },
            "max_rss_kb": {
                "baseline": 100.0,
                "current": 120.0,
                "ratio": 1.20,
                "pct": 0.20,
                "regression": 0.20,
                "status": memory_status
            }
        },
        "verdict": {
            "status": memory_status,
            "counts": {
                "pass": 1 + memory_pass,
                "warn": memory_warn,
                "fail": memory_fail,
                "skip": 0
            },
            "reasons": reasons
        }
    });

    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize scenario fixture"),
    )
    .expect("failed to write scenario fixture");
}

fn write_probe_compare_receipt(path: &Path, probe: &str, wall_current: f64) {
    write_probe_compare_receipt_many(path, &[(probe, wall_current, "dominant")]);
}

fn write_probe_compare_receipt_many(path: &Path, probes: &[(&str, f64, &str)]) {
    let receipt = json!({
        "schema": "perfgate.probe_compare.v1",
        "tool": {"name": "perfgate", "version": "0.16.0"},
        "run": {
            "id": "probe-compare-run",
            "started_at": "2026-05-08T00:00:00Z",
            "ended_at": "2026-05-08T00:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "scenario": "release_workload",
        "probes": probes.iter().map(|(probe, wall_current, scope)| {
            let regression = if *wall_current > 100.0 {
                (*wall_current - 100.0) / 100.0
            } else {
                0.0
            };
            json!({
                "name": probe,
                "scope": scope,
                "baseline_count": 1,
                "current_count": 1,
                "deltas": {
                    "wall_ms": {
                        "baseline": 100.0,
                        "current": wall_current,
                        "ratio": wall_current / 100.0,
                        "pct": (wall_current - 100.0) / 100.0,
                        "regression": regression,
                        "status": if regression > 0.0 { "warn" } else { "pass" }
                    }
                },
                "status": if regression > 0.0 { "warn" } else { "pass" }
            })
        }).collect::<Vec<_>>(),
        "verdict": {
            "status": "pass",
            "counts": {"pass": 1, "warn": 0, "fail": 0, "skip": 0},
            "reasons": []
        }
    });

    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize probe compare fixture"),
    )
    .expect("failed to write probe compare fixture");
}
