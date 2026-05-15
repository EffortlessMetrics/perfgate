//! Integration tests for `perfgate probe`.

mod common;

use common::perfgate_cmd;
use predicates::prelude::*;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn test_probe_init_writes_starter_template_files() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("probe-starter");

    perfgate_cmd()
        .arg("probe")
        .arg("init")
        .arg("--template")
        .arg("parser")
        .arg("--out-dir")
        .arg(&out_dir)
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Probe starter template 'parser' written",
        ))
        .stderr(predicate::str::contains("Review the generated snippets"));

    let readme = fs::read_to_string(out_dir.join("README.md")).expect("read template readme");
    let baseline =
        fs::read_to_string(out_dir.join("probes-baseline.jsonl")).expect("read baseline jsonl");
    let current =
        fs::read_to_string(out_dir.join("probes-current.jsonl")).expect("read current jsonl");
    let scenario = fs::read_to_string(out_dir.join("scenario.toml")).expect("read scenario toml");
    let tradeoff = fs::read_to_string(out_dir.join("tradeoff.toml")).expect("read tradeoff toml");

    assert!(readme.contains("perfgate ingest probes"));
    assert!(baseline.contains("\"probe\":\"parser.tokenize\""));
    assert!(current.contains("\"probe\":\"parser.batch_loop\""));
    assert!(scenario.contains("name = \"large_file_parse\""));
    assert!(scenario.contains("probe_compare = \"artifacts/perfgate/probe-compare.json\""));
    assert!(tradeoff.contains("probe = \"parser.batch_loop\""));
    assert!(tradeoff.contains("max_regression = 0.03"));
}

#[test]
fn test_probe_init_refuses_to_overwrite_without_force() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("probe-starter");

    perfgate_cmd()
        .arg("probe")
        .arg("init")
        .arg("--template")
        .arg("batch")
        .arg("--out-dir")
        .arg(&out_dir)
        .assert()
        .success();

    perfgate_cmd()
        .arg("probe")
        .arg("init")
        .arg("--template")
        .arg("batch")
        .arg("--out-dir")
        .arg(&out_dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"))
        .stderr(predicate::str::contains("--force"));

    perfgate_cmd()
        .arg("probe")
        .arg("init")
        .arg("--template")
        .arg("batch")
        .arg("--out-dir")
        .arg(&out_dir)
        .arg("--force")
        .assert()
        .success();
}

#[test]
fn test_probe_compare_writes_probe_compare_receipt() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline-probes.json");
    let current_path = temp_dir.path().join("current-probes.json");
    let output_path = temp_dir.path().join("probe-compare.json");

    write_probe_receipt(
        &baseline_path,
        "baseline-run",
        &[
            ("parser.tokenize", "local", 12.0),
            ("parser.ast_build", "dominant", 40.0),
        ],
    );
    write_probe_receipt(
        &current_path,
        "current-run",
        &[
            ("parser.tokenize", "local", 13.0),
            ("parser.ast_build", "dominant", 36.0),
        ],
    );

    perfgate_cmd()
        .arg("probe")
        .arg("compare")
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--current")
        .arg(&current_path)
        .arg("--out")
        .arg(&output_path)
        .assert()
        .success()
        .stderr(predicate::str::contains("Probe compare receipt written"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read probe compare receipt"),
    )
    .expect("probe compare receipt should be JSON");
    let typed: perfgate_types::ProbeCompareReceipt =
        serde_json::from_value(receipt.clone()).expect("probe compare receipt should deserialize");

    assert_eq!(typed.schema, "perfgate.probe_compare.v1");
    assert_eq!(typed.scenario.as_deref(), Some("large_file_parse"));
    assert_eq!(typed.probes.len(), 2);
    assert_eq!(typed.verdict.status, perfgate_types::VerdictStatus::Warn);
    assert_eq!(receipt["probes"][0]["deltas"]["wall_ms"]["baseline"], 40.0);
    assert_eq!(receipt["probes"][0]["deltas"]["wall_ms"]["current"], 36.0);
    assert_eq!(receipt["probes"][0]["status"], "pass");
    assert_eq!(receipt["probes"][1]["name"], "parser.tokenize");
    assert_eq!(receipt["probes"][1]["status"], "warn");
}

#[test]
fn test_probe_compare_warns_on_missing_probe() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline-probes.json");
    let current_path = temp_dir.path().join("current-probes.json");
    let output_path = temp_dir.path().join("probe-compare.json");

    write_probe_receipt(
        &baseline_path,
        "baseline-run",
        &[("parser.tokenize", "local", 12.0)],
    );
    write_probe_receipt(
        &current_path,
        "current-run",
        &[("parser.ast_build", "dominant", 36.0)],
    );

    perfgate_cmd()
        .arg("probe")
        .arg("compare")
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--current")
        .arg(&current_path)
        .arg("--out")
        .arg(&output_path)
        .assert()
        .success();

    let typed: perfgate_types::ProbeCompareReceipt = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read probe compare receipt"),
    )
    .expect("probe compare receipt should deserialize");

    assert_eq!(typed.verdict.status, perfgate_types::VerdictStatus::Warn);
    assert!(
        typed
            .warnings
            .iter()
            .any(|warning| warning.contains("missing from current"))
    );
}

fn write_probe_receipt(path: &Path, run_id: &str, probes: &[(&str, &str, f64)]) {
    let observations: Vec<_> = probes
        .iter()
        .map(|(name, scope, wall_ms)| {
            json!({
                "name": name,
                "parent": "parser.total",
                "scope": scope,
                "metrics": {
                    "wall_ms": {
                        "value": wall_ms,
                        "unit": "ms"
                    }
                }
            })
        })
        .collect();

    let receipt = json!({
        "schema": "perfgate.probe.v1",
        "tool": {"name": "perfgate-ingest", "version": "0.16.0"},
        "run": {
            "id": run_id,
            "started_at": "2026-05-08T00:00:00Z",
            "ended_at": "2026-05-08T00:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "parser",
            "command": ["cargo", "bench"],
            "repeat": probes.len(),
            "warmup": 0
        },
        "scenario": "large_file_parse",
        "probes": observations
    });

    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize probe receipt"),
    )
    .expect("failed to write probe receipt");
}
