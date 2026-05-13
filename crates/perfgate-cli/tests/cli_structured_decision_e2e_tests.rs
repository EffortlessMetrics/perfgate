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

fn write_probe_config(
    path: &Path,
    probe_baseline_path: &Path,
    probe_current_path: &Path,
    probe_compare_path: &Path,
) {
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
probe_baseline = "{}"
probe_current = "{}"
probe_compare = "{}"

[[tradeoff]]
name = "memory_for_probe_speed"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
probe = "parser.batch_loop"
min_improvement_ratio = 1.10
"#,
            command_toml_array(&success_command()),
            toml_path(probe_baseline_path),
            toml_path(probe_current_path),
            toml_path(probe_compare_path)
        ),
    )
    .expect("write probe config");
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
        // The live check can produce a policy failure on tiny one-shot
        // commands; this fixture only needs it to emit the compare receipt.
        .code(predicate::in_iter([0, 2]));

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
    let probe_baseline_path = root.join("artifacts/perfgate/parser/probes-baseline.json");
    let probe_current_path = root.join("artifacts/perfgate/parser/probes-current.json");
    let probe_compare_path = root.join("artifacts/perfgate/parser/probe-compare.json");
    write_probe_config(
        &config_path,
        &probe_baseline_path,
        &probe_current_path,
        &probe_compare_path,
    );

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
        // The live check can produce a policy failure on tiny one-shot
        // commands; this fixture only needs it to emit the compare receipt.
        .code(predicate::in_iter([0, 2]));

    let compare_path = root.join("artifacts/perfgate/parser/compare.json");
    assert!(compare_path.exists(), "check should write compare receipt");
    write_controlled_compare_receipt_with_wall(&compare_path, 96.0);
    write_probe_receipt(&probe_baseline_path, 100.0);
    write_probe_receipt(&probe_current_path, 80.0);

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

    let scenario_path = root.join("artifacts/perfgate/scenario.json");
    let tradeoff_path = root.join("artifacts/perfgate/tradeoff.json");
    let decision_path = root.join("artifacts/perfgate/decision.md");
    let decision_index_path = root.join("artifacts/perfgate/decision.index.json");
    assert!(
        probe_compare_path.exists(),
        "decision evaluate should write configured probe compare receipt"
    );

    let probe_compare: perfgate_types::ProbeCompareReceipt = serde_json::from_str(
        &fs::read_to_string(&probe_compare_path).expect("read probe compare receipt"),
    )
    .expect("probe compare receipt should deserialize");
    assert_eq!(probe_compare.schema, "perfgate.probe_compare.v1");
    let expected_probe_baseline = toml_path(&probe_baseline_path);
    let expected_probe_current = toml_path(&probe_current_path);
    assert_eq!(
        probe_compare
            .baseline_ref
            .as_ref()
            .and_then(|r| r.path.as_deref()),
        Some(expected_probe_baseline.as_str())
    );
    assert_eq!(
        probe_compare
            .current_ref
            .as_ref()
            .and_then(|r| r.path.as_deref()),
        Some(expected_probe_current.as_str())
    );

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
    assert_eq!(
        tradeoff.rules[0].requirements[0].probe.as_deref(),
        Some("parser.batch_loop")
    );
    assert_eq!(tradeoff.probes[0].name, "parser.batch_loop");

    let decision = fs::read_to_string(decision_path).expect("read decision md");
    assert!(decision.contains("perfgate tradeoff: warn"));
    assert!(decision.contains("tradeoff 'memory_for_probe_speed' accepted"));
    assert!(decision.contains("Probe Evidence"));

    let decision_index: perfgate_types::DecisionArtifactIndex = serde_json::from_str(
        &fs::read_to_string(decision_index_path).expect("read decision artifact index"),
    )
    .expect("decision artifact index should deserialize");
    assert_eq!(decision_index.schema, "perfgate.decision_index.v1");
    assert_eq!(decision_index.scenario, "artifacts/perfgate/scenario.json");
    assert_eq!(decision_index.tradeoff, "artifacts/perfgate/tradeoff.json");
    assert_eq!(decision_index.decision, "artifacts/perfgate/decision.md");
    assert_eq!(
        decision_index.probe_compares,
        vec![toml_path(&probe_compare_path)]
    );
    assert_eq!(
        decision_index.compare_receipts,
        vec!["artifacts/perfgate/parser/compare.json".to_string()]
    );

    perfgate_cmd()
        .current_dir(root)
        .args([
            "decision",
            "bundle",
            "--index",
            "artifacts/perfgate/decision.index.json",
            "--out",
            "artifacts/perfgate/decision-bundle.json",
            "--git-ref",
            "main",
            "--git-sha",
            "abc123",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Decision bundle written"));

    let bundle: perfgate_types::DecisionBundleReceipt = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/decision-bundle.json"))
            .expect("read decision bundle"),
    )
    .expect("decision bundle should deserialize");
    assert_eq!(bundle.schema, "perfgate.decision_bundle.v1");
    assert_eq!(bundle.metadata.git_ref.as_deref(), Some("main"));
    assert_eq!(bundle.metadata.git_sha.as_deref(), Some("abc123"));
    assert_eq!(bundle.index.schema, "perfgate.decision_index.v1");
    assert!(
        bundle
            .artifacts
            .iter()
            .any(|artifact| artifact.kind
                == perfgate_types::DecisionBundleArtifactKind::DecisionIndex)
    );
    assert!(
        bundle
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == perfgate_types::DecisionBundleArtifactKind::Scenario)
    );
    assert!(
        bundle
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == perfgate_types::DecisionBundleArtifactKind::Tradeoff)
    );
    assert!(
        bundle.artifacts.iter().any(|artifact| artifact.kind
            == perfgate_types::DecisionBundleArtifactKind::DecisionMarkdown)
    );
    assert!(
        bundle
            .artifacts
            .iter()
            .any(|artifact| artifact.kind
                == perfgate_types::DecisionBundleArtifactKind::ProbeCompare)
    );
    assert!(bundle.artifacts.iter().any(
        |artifact| artifact.kind == perfgate_types::DecisionBundleArtifactKind::CompareReceipt
    ));
}

fn write_controlled_compare_receipt(path: &Path) {
    write_controlled_compare_receipt_with_wall(path, 80.0);
}

fn write_controlled_compare_receipt_with_wall(path: &Path, wall_current: f64) {
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
                "current": wall_current,
                "ratio": wall_current / 100.0,
                "pct": (wall_current - 100.0) / 100.0,
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

fn write_probe_receipt(path: &Path, wall_ms: f64) {
    let receipt = json!({
        "schema": "perfgate.probe.v1",
        "tool": {"name": "perfgate", "version": "0.16.0"},
        "run": {
            "id": format!("probe-run-{wall_ms}"),
            "started_at": "2026-05-08T00:00:00Z",
            "ended_at": "2026-05-08T00:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "scenario": "release_workload",
        "probes": [{
            "name": "parser.batch_loop",
            "scope": "dominant",
            "metrics": {
                "wall_ms": {
                    "value": wall_ms,
                    "unit": "ms"
                }
            }
        }]
    });

    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize probe receipt"),
    )
    .expect("write probe receipt");
}

fn toml_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}
