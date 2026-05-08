//! Integration tests for `perfgate ingest`.

mod common;

use common::perfgate_cmd;
use predicates::prelude::*;
use serde_json::Value;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_ingest_hyperfine_writes_run_receipt() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("hyperfine.json");
    let output_path = temp_dir.path().join("run.json");

    fs::write(
        &input_path,
        r#"{
  "results": [
    {
      "command": "cargo bench",
      "times": [0.100, 0.120, 0.110],
      "mean": 0.110,
      "stddev": 0.010,
      "median": 0.110,
      "min": 0.100,
      "max": 0.120
    }
  ]
}"#,
    )
    .expect("failed to write hyperfine input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("hyperfine")
        .arg("--input")
        .arg(&input_path)
        .arg("--name")
        .arg("hyperfine-smoke")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Ingested"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["schema"], "perfgate.run.v1");
    assert_eq!(receipt["bench"]["name"], "hyperfine-smoke");
    assert_eq!(receipt["tool"]["name"], "perfgate-ingest");
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(3));
}

#[test]
fn test_ingest_probes_writes_probe_receipt() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("probes.jsonl");
    let output_path = temp_dir.path().join("probe.json");

    fs::write(
        &input_path,
        r#"{"probe":"parser.tokenize","scope":"local","wall_ms":12.4,"alloc_bytes":184320,"items":10000}
{"name":"parser.ast_build","parent":"parser.total","metrics":{"wall_ms":{"value":44.8,"unit":"ms"}}}
"#,
    )
    .expect("failed to write probe input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("probes")
        .arg("--file")
        .arg(&input_path)
        .arg("--bench")
        .arg("parser")
        .arg("--scenario")
        .arg("large_file_parse")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Ingested probes"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read probe output"),
    )
    .expect("probe output should be JSON");

    assert_eq!(receipt["schema"], "perfgate.probe.v1");
    assert_eq!(receipt["tool"]["name"], "perfgate-ingest");
    assert_eq!(receipt["bench"]["name"], "parser");
    assert_eq!(receipt["scenario"], "large_file_parse");
    assert_eq!(receipt["probes"].as_array().map(Vec::len), Some(2));
    assert_eq!(receipt["probes"][0]["name"], "parser.tokenize");
    assert_eq!(receipt["probes"][0]["scope"], "local");
    assert_eq!(receipt["probes"][0]["metrics"]["wall_ms"]["value"], 12.4);
    assert_eq!(receipt["probes"][0]["metrics"]["wall_ms"]["unit"], "ms");
    assert_eq!(
        receipt["probes"][0]["metrics"]["alloc_bytes"]["value"],
        184320.0
    );
    assert_eq!(receipt["probes"][1]["parent"], "parser.total");
}
