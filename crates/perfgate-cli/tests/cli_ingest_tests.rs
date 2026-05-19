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
fn test_ingest_generic_command_json_writes_run_receipt() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("generic.json");
    let output_path = temp_dir.path().join("run.json");

    fs::write(
        &input_path,
        r#"{
  "source_kind": "generic_command_json",
  "benchmark": {
    "name": "node-parser",
    "command": ["node", "bench.js"],
    "work_units": 5000
  },
  "host": {"os": "linux", "arch": "x86_64", "cpu_count": 4},
  "metrics": {
    "wall_ms": {
      "unit": "ms",
      "direction": "lower_is_better",
      "samples": [
        {"value": 120.0, "exit_code": 0},
        {"value": 118.0, "exit_code": 0},
        {"value": 123.0, "exit_code": 0}
      ]
    },
    "throughput_per_s": {
      "unit": "ops/s",
      "direction": "higher_is_better",
      "summary": {
        "median": 41000.0,
        "min": 39000.0,
        "max": 42500.0,
        "mean": 40800.0,
        "stddev": 1300.0
      }
    }
  }
}"#,
    )
    .expect("failed to write generic input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("generic-command-json")
        .arg("--input")
        .arg(&input_path)
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains(
            "Evidence source: generic_command_json",
        ))
        .stderr(predicate::str::contains("no baseline was promoted"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["schema"], "perfgate.run.v1");
    assert_eq!(receipt["bench"]["name"], "node-parser");
    assert_eq!(receipt["bench"]["command"][0], "node");
    assert_eq!(receipt["bench"]["work_units"], 5000);
    assert_eq!(receipt["run"]["host"]["os"], "linux");
    assert_eq!(receipt["tool"]["name"], "perfgate-ingest");
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(3));
    assert_eq!(receipt["stats"]["wall_ms"]["median"], 120);
    assert_eq!(receipt["stats"]["throughput_per_s"]["median"], 41000.0);
}

#[test]
fn test_ingest_generic_command_json_summary_only_marks_limited_noise() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("generic-summary.json");
    let output_path = temp_dir.path().join("run.json");

    fs::write(
        &input_path,
        r#"{
  "benchmark": {"name": "summary-only"},
  "metrics": {
    "wall_ms": {
      "unit": "seconds",
      "direction": "lower_is_better",
      "summary": {
        "median": 0.240,
        "min": 0.200,
        "max": 0.300,
        "mean": 0.250,
        "stddev": 0.040,
        "sample_count": 9
      }
    }
  }
}"#,
    )
    .expect("failed to write generic summary input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("generic-json")
        .arg("--input")
        .arg(&input_path)
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Host context: unknown"))
        .stderr(predicate::str::contains("Sample model: summary-only"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["bench"]["repeat"], 9);
    assert_eq!(receipt["run"]["host"]["os"], "unknown");
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(0));
    assert_eq!(receipt["stats"]["wall_ms"]["median"], 240);
}

#[test]
fn test_ingest_generic_command_json_missing_wall_metric_fails_actionably() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("generic-missing-wall.json");

    fs::write(
        &input_path,
        r#"{
  "benchmark": {"name": "missing-wall"},
  "metrics": {
    "throughput_per_s": {
      "unit": "ops/s",
      "direction": "higher_is_better",
      "samples": [1000.0, 1100.0]
    }
  }
}"#,
    )
    .expect("failed to write generic input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("generic-command-json")
        .arg("--input")
        .arg(&input_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires a 'wall_ms' metric"));
}

#[test]
fn test_ingest_generic_command_json_ambiguous_unit_fails_actionably() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("generic-missing-unit.json");

    fs::write(
        &input_path,
        r#"{
  "benchmark": {"name": "missing-unit"},
  "metrics": {
    "wall_ms": {
      "direction": "lower_is_better",
      "samples": [100.0]
    }
  }
}"#,
    )
    .expect("failed to write generic input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("generic-command-json")
        .arg("--input")
        .arg(&input_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires explicit unit"));
}

#[test]
fn test_ingest_generic_command_json_ambiguous_direction_fails_actionably() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("generic-missing-direction.json");

    fs::write(
        &input_path,
        r#"{
  "benchmark": {"name": "missing-direction"},
  "metrics": {
    "wall_ms": {
      "unit": "ms",
      "samples": [100.0]
    }
  }
}"#,
    )
    .expect("failed to write generic input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("generic-command-json")
        .arg("--input")
        .arg(&input_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("requires explicit direction"));
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
