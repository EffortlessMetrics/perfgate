//! End-to-end fixture for the structured decision evidence path.

use predicates::prelude::*;
use serde_json::json;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

fn command_toml_array(command: &[&str]) -> String {
    command
        .iter()
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn write_config(path: &Path) {
    fs::write(
        path,
        format!(
            r#"[defaults]
repeat = 1
warmup = 0
threshold = 10.0
warn_factor = 0.50
noise_threshold = 1.0
noise_policy = "warn"
baseline_dir = "baselines"
out_dir = "artifacts/perfgate"

[[bench]]
name = "parser"
command = [{}]

[[scenario]]
name = "release_workload"
weight = 1.0
bench = "parser"

[[tradeoff]]
name = "memory_for_speed"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
min_improvement_ratio = 1.10
"#,
            command_toml_array(&success_command())
        ),
    )
    .expect("write config");
}

#[test]
fn structured_decision_path_produces_scenario_and_tradeoff_receipts() {
    let temp_dir = tempdir().expect("create temp dir");
    let root = temp_dir.path();
    let config_path = root.join("perfgate.toml");
    write_config(&config_path);

    perfgate_cmd()
        .current_dir(root)
        .args(["check", "--config", "perfgate.toml", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("parser"));

    perfgate_cmd()
        .current_dir(root)
        .args(["baseline", "promote", "--config", "perfgate.toml", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Promoted baseline for parser"));

    perfgate_cmd()
        .current_dir(root)
        .args([
            "check",
            "--config",
            "perfgate.toml",
            "--all",
            "--require-baseline",
        ])
        .assert()
        .success();

    let compare_path = root.join("artifacts/perfgate/parser/compare.json");
    assert!(compare_path.exists(), "check should write compare receipt");
    // The live check proves canonical artifact placement. A controlled compare
    // keeps the tradeoff decision deterministic across OS metric collectors.
    write_controlled_compare_receipt(&compare_path);

    let scenario_path = root.join("artifacts/perfgate/scenario.json");
    perfgate_cmd()
        .current_dir(root)
        .args([
            "scenario",
            "evaluate",
            "--config",
            "perfgate.toml",
            "--out",
            "artifacts/perfgate/scenario.json",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Scenario receipt written"));

    let scenario: perfgate_types::ScenarioReceipt =
        serde_json::from_str(&fs::read_to_string(&scenario_path).expect("read scenario receipt"))
            .expect("scenario receipt should deserialize");
    assert_eq!(scenario.schema, "perfgate.scenario.v1");
    assert_eq!(scenario.scenario.name, "release_workload");
    assert_eq!(scenario.components.len(), 1);
    assert_eq!(scenario.components[0].benchmark.as_deref(), Some("parser"));
    assert!(scenario.weighted_deltas.contains_key("wall_ms"));
    assert_eq!(
        scenario.weighted_deltas["max_rss_kb"].status,
        perfgate_types::MetricStatus::Fail
    );

    let tradeoff_path = root.join("artifacts/perfgate/tradeoff.json");
    perfgate_cmd()
        .current_dir(root)
        .args([
            "tradeoff",
            "evaluate",
            "--config",
            "perfgate.toml",
            "--scenario",
            "artifacts/perfgate/scenario.json",
            "--out",
            "artifacts/perfgate/tradeoff.json",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Tradeoff receipt written"));

    let tradeoff: perfgate_types::TradeoffReceipt =
        serde_json::from_str(&fs::read_to_string(&tradeoff_path).expect("read tradeoff receipt"))
            .expect("tradeoff receipt should deserialize");
    assert_eq!(tradeoff.schema, "perfgate.tradeoff.v1");
    assert_eq!(tradeoff.scenario.as_deref(), Some("release_workload"));
    assert_eq!(tradeoff.configured_rules.len(), 1);
    assert_eq!(tradeoff.rules.len(), 1);
    assert_eq!(tradeoff.rules[0].name, "memory_for_speed");
    assert_eq!(
        tradeoff.rules[0].status,
        perfgate_types::TradeoffDecisionStatus::Accepted
    );
    assert!(tradeoff.rules[0].accepted);
    assert_eq!(tradeoff.rules[0].requirements.len(), 1);
    assert!(tradeoff.rules[0].requirements[0].satisfied);
    assert!(tradeoff.decision.accepted_tradeoff);
    assert_eq!(tradeoff.decision.status, perfgate_types::MetricStatus::Warn);
    assert_eq!(
        tradeoff.weighted_deltas["max_rss_kb"].status,
        perfgate_types::MetricStatus::Warn
    );
    assert_eq!(
        tradeoff.verdict.status,
        perfgate_types::VerdictStatus::Warn,
        "tradeoff command exit code should allow accepted warn verdicts"
    );

    perfgate_cmd()
        .current_dir(root)
        .args([
            "md",
            "--tradeoff",
            "artifacts/perfgate/tradeoff.json",
            "--out",
            "artifacts/perfgate/decision.md",
        ])
        .assert()
        .success();
    let decision =
        fs::read_to_string(root.join("artifacts/perfgate/decision.md")).expect("read decision md");
    assert!(decision.contains("perfgate tradeoff: warn"));
    assert!(decision.contains("tradeoff 'memory_for_speed' accepted"));
    assert!(decision.contains("memory_for_speed"));
}

#[test]
fn decision_evaluate_runs_structured_decision_workflow() {
    let temp_dir = tempdir().expect("create temp dir");
    let root = temp_dir.path();
    let config_path = root.join("perfgate.toml");
    write_config(&config_path);

    perfgate_cmd()
        .current_dir(root)
        .args(["check", "--config", "perfgate.toml", "--all"])
        .assert()
        .success();

    perfgate_cmd()
        .current_dir(root)
        .args(["baseline", "promote", "--config", "perfgate.toml", "--all"])
        .assert()
        .success();

    perfgate_cmd()
        .current_dir(root)
        .args([
            "check",
            "--config",
            "perfgate.toml",
            "--all",
            "--require-baseline",
        ])
        .assert()
        .success();

    let compare_path = root.join("artifacts/perfgate/parser/compare.json");
    assert!(compare_path.exists(), "check should write compare receipt");
    write_controlled_compare_receipt(&compare_path);

    perfgate_cmd()
        .current_dir(root)
        .args(["decision", "evaluate", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Scenario receipt written"))
        .stderr(predicate::str::contains("Tradeoff receipt written"))
        .stderr(predicate::str::contains("Decision markdown written"));

    let scenario_path = root.join("artifacts/perfgate/scenario.json");
    let tradeoff_path = root.join("artifacts/perfgate/tradeoff.json");
    let decision_path = root.join("artifacts/perfgate/decision.md");

    let scenario: perfgate_types::ScenarioReceipt =
        serde_json::from_str(&fs::read_to_string(scenario_path).expect("read scenario receipt"))
            .expect("scenario receipt should deserialize");
    assert_eq!(scenario.schema, "perfgate.scenario.v1");

    let tradeoff: perfgate_types::TradeoffReceipt =
        serde_json::from_str(&fs::read_to_string(tradeoff_path).expect("read tradeoff receipt"))
            .expect("tradeoff receipt should deserialize");
    assert_eq!(tradeoff.schema, "perfgate.tradeoff.v1");
    assert!(tradeoff.decision.accepted_tradeoff);
    assert_eq!(tradeoff.decision.status, perfgate_types::MetricStatus::Warn);

    let decision = fs::read_to_string(decision_path).expect("read decision md");
    assert!(decision.contains("perfgate tradeoff: warn"));
    assert!(decision.contains("tradeoff 'memory_for_speed' accepted"));
}

fn write_controlled_compare_receipt(path: &Path) {
    let receipt = json!({
        "schema": "perfgate.compare.v1",
        "tool": {"name": "perfgate", "version": "0.16.0"},
        "bench": {
            "name": "parser",
            "command": success_command(),
            "repeat": 1,
            "warmup": 0
        },
        "baseline_ref": {
            "path": "baselines/parser.json",
            "run_id": "parser-baseline"
        },
        "current_ref": {
            "path": "artifacts/perfgate/parser/run.json",
            "run_id": "parser-current"
        },
        "budgets": {},
        "deltas": {
            "wall_ms": {
                "baseline": 100.0,
                "current": 80.0,
                "ratio": 0.80,
                "pct": -0.20,
                "regression": 0.0,
                "status": "pass"
            },
            "max_rss_kb": {
                "baseline": 100.0,
                "current": 1200.0,
                "ratio": 12.0,
                "pct": 11.0,
                "regression": 11.0,
                "status": "fail"
            }
        },
        "verdict": {
            "status": "fail",
            "counts": {
                "pass": 1,
                "warn": 0,
                "fail": 1,
                "skip": 0
            },
            "reasons": ["max_rss_kb_fail"]
        }
    });

    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize controlled compare receipt"),
    )
    .expect("write controlled compare receipt");
}
