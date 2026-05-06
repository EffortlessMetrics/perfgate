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
