//! Integration test for the runnable performance decision example.

use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

#[test]
fn performance_decision_example_runs_end_to_end() {
    let temp_dir = tempdir().expect("create temp dir");
    let root = temp_dir.path();
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate should live under crates/perfgate-cli");
    let source = repo_root.join("examples/performance-decision");
    let destination = root.join("examples/performance-decision");
    copy_dir_all(&source, &destination);

    perfgate_cmd()
        .current_dir(root)
        .args([
            "ingest",
            "probes",
            "--file",
            "examples/performance-decision/probes-baseline.jsonl",
            "--out",
            "artifacts/perfgate/large-file/probes-baseline.json",
        ])
        .assert()
        .success();

    perfgate_cmd()
        .current_dir(root)
        .args([
            "ingest",
            "probes",
            "--file",
            "examples/performance-decision/probes-current.jsonl",
            "--out",
            "artifacts/perfgate/large-file/probes-current.json",
        ])
        .assert()
        .success();

    perfgate_cmd()
        .current_dir(root)
        .args([
            "decision",
            "evaluate",
            "--config",
            "examples/performance-decision/perfgate.toml",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Probe compare receipt written"))
        .stderr(predicate::str::contains("Scenario receipt written"))
        .stderr(predicate::str::contains("Tradeoff receipt written"))
        .stderr(predicate::str::contains("Decision markdown written"))
        .stderr(predicate::str::contains("Decision artifact index written"));

    let probe_compare: perfgate_types::ProbeCompareReceipt = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/large-file/probe-compare.json"))
            .expect("read probe compare receipt"),
    )
    .expect("probe compare receipt should deserialize");
    assert!(
        probe_compare
            .probes
            .iter()
            .any(|probe| probe.name == "parser.tokenize")
    );
    assert!(
        probe_compare
            .probes
            .iter()
            .any(|probe| probe.name == "parser.batch_loop")
    );

    let scenario: perfgate_types::ScenarioReceipt = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/scenario.json"))
            .expect("read scenario receipt"),
    )
    .expect("scenario receipt should deserialize");
    assert_eq!(scenario.schema, "perfgate.scenario.v1");
    assert!(
        scenario.components[0]
            .probes
            .iter()
            .any(|probe| probe == "parser.batch_loop")
    );

    let tradeoff: perfgate_types::TradeoffReceipt = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/tradeoff.json"))
            .expect("read tradeoff receipt"),
    )
    .expect("tradeoff receipt should deserialize");
    assert_eq!(tradeoff.schema, "perfgate.tradeoff.v1");
    assert!(tradeoff.decision.accepted_tradeoff);
    assert_eq!(tradeoff.decision.status, perfgate_types::MetricStatus::Warn);
    assert_eq!(tradeoff.rules[0].name, "memory_for_probe_speed");
    assert!(tradeoff.rules[0].accepted);
    assert_eq!(
        tradeoff.rules[0].requirements[0].probe.as_deref(),
        Some("parser.batch_loop")
    );
    assert!(
        tradeoff
            .probes
            .iter()
            .any(|probe| probe.name == "parser.batch_loop")
    );
    assert!(
        tradeoff
            .probes
            .iter()
            .any(|probe| probe.name == "parser.tokenize")
    );

    let decision =
        fs::read_to_string(root.join("artifacts/perfgate/decision.md")).expect("read decision md");
    assert!(decision.contains("perfgate tradeoff: warn"));
    assert!(decision.contains("tradeoff 'memory_for_probe_speed' accepted"));
    assert!(decision.contains("Weighted Workload"));
    assert!(decision.contains("Probe Evidence"));
    assert!(decision.contains("parser.tokenize"));
    assert!(decision.contains("+2.07%"));
    assert!(decision.contains("parser.batch_loop"));
    assert!(decision.contains("-10.40%"));
    assert!(decision.contains("Accepted / Rejected Tradeoffs"));
    assert!(decision.contains("Evidence Files"));
    assert!(decision.contains("Local Reproduction"));

    let decision_index: perfgate_types::DecisionArtifactIndex = serde_json::from_str(
        &fs::read_to_string(root.join("artifacts/perfgate/decision.index.json"))
            .expect("read decision artifact index"),
    )
    .expect("decision artifact index should deserialize");
    assert_eq!(decision_index.schema, "perfgate.decision_index.v1");
    assert_eq!(
        decision_index.probe_compares,
        vec!["artifacts/perfgate/large-file/probe-compare.json"]
    );
    assert_eq!(
        decision_index.compare_receipts,
        vec![
            "examples/performance-decision/receipts/large-file.compare.json",
            "examples/performance-decision/receipts/small-edit.compare.json"
        ]
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
            .any(|artifact| artifact.path == "artifacts/perfgate/large-file/probe-compare.json")
    );
    assert!(bundle.artifacts.iter().any(|artifact| artifact.path
        == "examples/performance-decision/receipts/large-file.compare.json"));
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create destination directory");
    for entry in fs::read_dir(source).expect("read source directory") {
        let entry = entry.expect("read source directory entry");
        let entry_source = entry.path();
        let entry_destination = destination.join(entry.file_name());
        if entry.file_type().expect("read source entry type").is_dir() {
            copy_dir_all(&entry_source, &entry_destination);
        } else {
            fs::copy(&entry_source, &entry_destination).expect("copy source file");
        }
    }
}
