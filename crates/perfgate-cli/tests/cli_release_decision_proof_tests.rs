//! Release-proof fixture for the shipped first-run-to-decision workflow.

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

fn write_minimal_rust_repo(dir: &Path) {
    fs::write(
        dir.join("Cargo.toml"),
        r#"
[package]
name = "example"
version = "0.1.0"
edition = "2021"

[[bench]]
name = "parser"
harness = false
"#,
    )
    .expect("write Cargo.toml");
}

fn tune_generated_config_for_decision_fixture(config_path: &Path) {
    let generated = fs::read_to_string(config_path).expect("read generated config");
    assert!(generated.contains("repeat = 7"));
    assert!(generated.contains("warmup = 1"));
    assert!(generated.contains(r#"command = ["cargo", "bench", "--bench", "parser"]"#));

    let tuned = generated
        .replace("repeat = 7", "repeat = 1")
        .replace("warmup = 1", "warmup = 0")
        .replace(
            r#"command = ["cargo", "bench", "--bench", "parser"]"#,
            &format!(r#"command = [{}]"#, command_toml_array(&success_command())),
        );

    let decision_config = r#"

[[scenario]]
name = "release_workload"
weight = 1.0
bench = "parser"
probe_baseline = "artifacts/perfgate/parser/probes-baseline.json"
probe_current = "artifacts/perfgate/probes.json"
probe_compare = "artifacts/perfgate/parser/probe-compare.json"

[[tradeoff]]
name = "memory_for_probe_speed"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
probe = "parser.batch_loop"
min_improvement_ratio = 1.10

[[tradeoff.allow]]
metric = "wall_ms"
probe = "parser.tokenize"
max_regression = 0.03
"#;

    fs::write(config_path, format!("{tuned}{decision_config}")).expect("write tuned config");
}

#[test]
fn release_decision_workflow_runs_from_generated_setup() {
    let temp_dir = tempdir().expect("create temp dir");
    let root = temp_dir.path();
    write_minimal_rust_repo(root);

    perfgate_cmd()
        .current_dir(root)
        .args(["init", "--ci", "github", "--profile", "standard"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "perfgate check --config perfgate.toml --all",
        ))
        .stderr(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --all",
        ));

    tune_generated_config_for_decision_fixture(&root.join("perfgate.toml"));

    perfgate_cmd()
        .current_dir(root)
        .args(["doctor", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK   config"))
        .stdout(predicate::str::contains("OK   benchmarks"));

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
        .code(predicate::in_iter([0, 2]));

    let compare_path = root.join("artifacts/perfgate/parser/compare.json");
    assert!(compare_path.exists(), "check should write compare receipt");
    // The live check proves the release path and canonical artifact placement.
    // The controlled compare keeps the accepted tradeoff deterministic across OS
    // metric collectors and fast no-op commands.
    write_controlled_compare_receipt(&compare_path);

    let probes_jsonl = root.join("artifacts/probes.jsonl");
    let probes_baseline_jsonl = root.join("artifacts/probes-baseline.jsonl");
    fs::create_dir_all(root.join("artifacts")).expect("create probe jsonl dir");
    fs::write(
        &probes_baseline_jsonl,
        r#"{"probe":"parser.tokenize","scope":"local","wall_ms":12.10,"items":10000}
{"probe":"parser.batch_loop","scope":"dominant","wall_ms":100.00,"items":10000}
"#,
    )
    .expect("write baseline probes jsonl");
    fs::write(
        &probes_jsonl,
        r#"{"probe":"parser.tokenize","scope":"local","wall_ms":12.35,"items":10000}
{"probe":"parser.batch_loop","scope":"dominant","wall_ms":89.60,"items":10000}
"#,
    )
    .expect("write current probes jsonl");

    perfgate_cmd()
        .current_dir(root)
        .args([
            "ingest",
            "probes",
            "--file",
            "artifacts/probes-baseline.jsonl",
            "--bench",
            "parser",
            "--scenario",
            "release_workload",
            "--out",
            "artifacts/perfgate/parser/probes-baseline.json",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Ingested probes"));

    perfgate_cmd()
        .current_dir(root)
        .args([
            "ingest",
            "probes",
            "--file",
            "artifacts/probes.jsonl",
            "--bench",
            "parser",
            "--scenario",
            "release_workload",
            "--out",
            "artifacts/perfgate/probes.json",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Ingested probes"));

    perfgate_cmd()
        .current_dir(root)
        .args(["decision", "evaluate", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Probe compare receipt written"))
        .stderr(predicate::str::contains("Scenario receipt written"))
        .stderr(predicate::str::contains("Tradeoff receipt written"))
        .stderr(predicate::str::contains("Decision markdown written"))
        .stderr(predicate::str::contains("Decision artifact index written"));

    let expected_artifacts = [
        "artifacts/perfgate/parser/run.json",
        "artifacts/perfgate/parser/compare.json",
        "artifacts/perfgate/probes.json",
        "artifacts/perfgate/parser/probe-compare.json",
        "artifacts/perfgate/scenario.json",
        "artifacts/perfgate/tradeoff.json",
        "artifacts/perfgate/decision.md",
        "artifacts/perfgate/decision.index.json",
    ];
    for artifact in expected_artifacts {
        assert!(root.join(artifact).exists(), "{artifact} should exist");
    }

    let scenario: perfgate_types::ScenarioReceipt = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/scenario.json"))
            .expect("read scenario receipt"),
    )
    .expect("scenario receipt should deserialize");
    assert_eq!(scenario.schema, "perfgate.scenario.v1");
    assert_eq!(scenario.scenario.name, "release_workload");
    assert_eq!(
        scenario.components[0]
            .probe_compare_ref
            .as_ref()
            .and_then(|reference| reference.path.as_deref()),
        Some("artifacts/perfgate/parser/probe-compare.json")
    );
    assert!(
        scenario.components[0]
            .probes
            .iter()
            .any(|probe| probe == "parser.tokenize")
    );

    let tradeoff: perfgate_types::TradeoffReceipt = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/tradeoff.json"))
            .expect("read tradeoff receipt"),
    )
    .expect("tradeoff receipt should deserialize");
    assert_eq!(tradeoff.schema, "perfgate.tradeoff.v1");
    assert!(tradeoff.decision.accepted_tradeoff);
    assert!(!tradeoff.decision.review_required);
    assert_eq!(tradeoff.decision.status, perfgate_types::MetricStatus::Warn);
    assert_eq!(tradeoff.rules[0].name, "memory_for_probe_speed");
    assert!(tradeoff.rules[0].accepted);
    assert_eq!(
        tradeoff.rules[0].requirements[0].probe.as_deref(),
        Some("parser.batch_loop")
    );
    assert!(
        tradeoff.rules[0].allowances[0]
            .observed_regression
            .is_some_and(|regression| regression < 0.03)
    );

    let decision =
        fs::read_to_string(root.join("artifacts/perfgate/decision.md")).expect("read decision md");
    assert!(decision.contains("perfgate tradeoff: warn"));
    assert!(decision.contains("tradeoff 'memory_for_probe_speed' accepted"));
    assert!(decision.contains("parser.tokenize"));
    assert!(decision.contains("parser.batch_loop"));
    assert!(decision.contains("Local Reproduction"));

    let decision_index: perfgate_types::DecisionArtifactIndex = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/decision.index.json"))
            .expect("read decision artifact index"),
    )
    .expect("decision artifact index should deserialize");
    assert_eq!(decision_index.schema, "perfgate.decision_index.v1");
    assert_eq!(decision_index.scenario, "artifacts/perfgate/scenario.json");
    assert_eq!(decision_index.tradeoff, "artifacts/perfgate/tradeoff.json");
    assert_eq!(decision_index.decision, "artifacts/perfgate/decision.md");
    assert_eq!(
        decision_index.probe_compares,
        vec!["artifacts/perfgate/parser/probe-compare.json"]
    );
    assert_eq!(
        decision_index.compare_receipts,
        vec!["artifacts/perfgate/parser/compare.json"]
    );
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
                "current": 96.0,
                "ratio": 0.96,
                "pct": -0.04,
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
