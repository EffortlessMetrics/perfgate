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
      "exit_codes": [0, 7, 0],
      "mean": 0.110,
      "stddev": 0.010,
      "median": 0.110,
      "min": 0.100,
      "max": 0.120,
      "user": 0.020,
      "system": 0.005
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
        .stderr(predicate::str::contains("Ingested"))
        .stderr(predicate::str::contains("Evidence source: hyperfine_json"))
        .stderr(predicate::str::contains(
            "CPU timing: hyperfine user+system",
        ))
        .stderr(predicate::str::contains(
            "hyperfine command timing may include shell",
        ));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["schema"], "perfgate.run.v1");
    assert_eq!(receipt["bench"]["name"], "hyperfine-smoke");
    assert_eq!(receipt["bench"]["command"][0], "cargo bench");
    assert_eq!(receipt["run"]["host"]["os"], "unknown");
    assert_eq!(receipt["tool"]["name"], "perfgate-ingest");
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(3));
    assert_eq!(receipt["samples"][1]["exit_code"], 7);
    assert_eq!(receipt["stats"]["wall_ms"]["mean"], 110.0);
    assert_eq!(receipt["stats"]["wall_ms"]["stddev"], 10.0);
    assert_eq!(receipt["stats"]["cpu_ms"]["mean"], 25.0);
}

#[test]
fn test_ingest_hyperfine_exit_code_mismatch_fails_actionably() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("hyperfine-bad-exit-codes.json");

    fs::write(
        &input_path,
        r#"{
  "results": [
    {
      "command": "cargo bench",
      "times": [0.100, 0.120, 0.110],
      "exit_codes": [0, 0],
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
        .arg(&input_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("exit_codes length"));
}

#[test]
fn test_ingest_criterion_jsonl_writes_run_receipt() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("criterion.jsonl");
    let output_path = temp_dir.path().join("run.json");

    fs::write(
        &input_path,
        r#"{"reason":"warmup","id":"ignored"}
{"reason":"benchmark-complete","id":"parser/large","iteration_count":[10,20,30],"measured_values":[50000000.0,100000000.0,150000000.0],"unit":"ns","throughput":[{"per_iteration":5000,"unit":"elements"}],"mean":{"estimate":5000000.0,"lower_bound":4900000.0,"upper_bound":5100000.0,"unit":"ns"},"median":{"estimate":4950000.0,"lower_bound":4890000.0,"upper_bound":5010000.0,"unit":"ns"}}"#,
    )
    .expect("failed to write Criterion input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("criterion")
        .arg("--input")
        .arg(&input_path)
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Evidence source: criterion"))
        .stderr(predicate::str::contains("Criterion measured samples"))
        .stderr(predicate::str::contains("Host context: unknown"))
        .stderr(predicate::str::contains(
            "Criterion statistics are not perfgate maturity policy",
        ));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["schema"], "perfgate.run.v1");
    assert_eq!(receipt["bench"]["name"], "parser/large");
    assert_eq!(receipt["bench"]["repeat"], 3);
    assert_eq!(receipt["bench"]["work_units"], 5000);
    assert_eq!(receipt["run"]["host"]["os"], "unknown");
    assert_eq!(
        receipt["bench"]["command"][0],
        "(ingested Criterion benchmark)"
    );
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(3));
    assert_eq!(receipt["samples"][0]["wall_ms"], 5);
    assert_eq!(receipt["stats"]["wall_ms"]["median"], 5);
    assert_eq!(receipt["stats"]["wall_ms"]["mean"], 5.0);
}

#[test]
fn test_ingest_criterion_estimates_marks_summary_only() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("estimates.json");
    let output_path = temp_dir.path().join("run.json");

    fs::write(
        &input_path,
        r#"{
  "mean": {"point_estimate": 5000000.0},
  "median": {"point_estimate": 4950000.0},
  "std_dev": {"point_estimate": 200000.0}
}"#,
    )
    .expect("failed to write Criterion estimates input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("criterion")
        .arg("--input")
        .arg(&input_path)
        .arg("--name")
        .arg("criterion-summary")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Sample model: summary-only"))
        .stderr(predicate::str::contains(
            "Criterion statistics are not perfgate maturity policy",
        ));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["bench"]["name"], "criterion-summary");
    assert_eq!(receipt["bench"]["repeat"], 0);
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(0));
    assert_eq!(receipt["stats"]["wall_ms"]["median"], 5);
}

#[test]
fn test_ingest_criterion_unsupported_unit_fails_actionably() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("criterion-bad-unit.json");

    fs::write(
        &input_path,
        r#"{"reason":"benchmark-complete","id":"bad","iteration_count":[1],"measured_values":[42.0],"unit":"cycles"}"#,
    )
    .expect("failed to write Criterion input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("criterion")
        .arg("--input")
        .arg(&input_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unsupported or ambiguous"));
}

#[test]
fn test_ingest_pytest_benchmark_writes_run_receipt() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("pytest-benchmark.json");
    let output_path = temp_dir.path().join("run.json");

    fs::write(
        &input_path,
        r#"{
  "machine_info": {
    "system": "Linux",
    "machine": "x86_64",
    "cpu": {"count": 8},
    "python_implementation": "CPython",
    "python_version": "3.11.0"
  },
  "benchmarks": [
    {
      "name": "test_parser",
      "fullname": "tests/test_perf.py::test_parser",
      "options": {"timer": "perf_counter", "warmup": false},
      "stats": {
        "min": 0.010,
        "max": 0.014,
        "mean": 0.012,
        "stddev": 0.001,
        "rounds": 3,
        "iterations": 1,
        "median": 0.012,
        "ops": 83.333333,
        "data": [0.010, 0.012, 0.014]
      }
    }
  ],
  "version": "4.0.0"
}"#,
    )
    .expect("failed to write pytest-benchmark input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("pytest-benchmark")
        .arg("--input")
        .arg(&input_path)
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains(
            "Evidence source: pytest_benchmark_json",
        ))
        .stderr(predicate::str::contains(
            "pytest-benchmark stats.data entries were preserved",
        ))
        .stderr(predicate::str::contains(
            "passing pytest tests are correctness evidence",
        ));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["schema"], "perfgate.run.v1");
    assert_eq!(receipt["bench"]["name"], "tests/test_perf.py::test_parser");
    assert_eq!(
        receipt["bench"]["command"][0],
        "(ingested pytest-benchmark JSON)"
    );
    assert_eq!(receipt["bench"]["repeat"], 3);
    assert_eq!(receipt["run"]["host"]["os"], "Linux");
    assert_eq!(receipt["run"]["host"]["arch"], "x86_64");
    assert_eq!(receipt["run"]["host"]["cpu_count"], 8);
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(3));
    assert_eq!(receipt["samples"][0]["wall_ms"], 10);
    assert_eq!(receipt["stats"]["wall_ms"]["mean"], 12.0);
    assert_eq!(receipt["stats"]["throughput_per_s"]["median"], 83.333333);
}

#[test]
fn test_ingest_pytest_benchmark_summary_only_marks_limited_noise() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("pytest-summary.json");
    let output_path = temp_dir.path().join("run.json");

    fs::write(
        &input_path,
        r#"{
  "benchmarks": [
    {
      "name": "test_summary",
      "stats": {
        "min": 0.010,
        "max": 0.020,
        "mean": 0.015,
        "stddev": 0.002,
        "rounds": 12,
        "iterations": 1,
        "median": 0.015
      }
    }
  ]
}"#,
    )
    .expect("failed to write pytest-benchmark summary input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("pytest")
        .arg("--input")
        .arg(&input_path)
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Sample model: summary-only"))
        .stderr(predicate::str::contains("Host context: unknown or partial"));

    let receipt: Value = serde_json::from_str(
        &fs::read_to_string(&output_path).expect("failed to read ingest output"),
    )
    .expect("ingest output should be JSON");

    assert_eq!(receipt["bench"]["name"], "test_summary");
    assert_eq!(receipt["bench"]["repeat"], 12);
    assert_eq!(receipt["samples"].as_array().map(Vec::len), Some(0));
    assert_eq!(receipt["run"]["host"]["os"], "unknown");
    assert_eq!(receipt["stats"]["wall_ms"]["median"], 15);
}

#[test]
fn test_ingest_pytest_benchmark_data_rounds_mismatch_fails_actionably() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let input_path = temp_dir.path().join("pytest-bad-data.json");

    fs::write(
        &input_path,
        r#"{
  "benchmarks": [
    {
      "name": "bad",
      "stats": {
        "min": 0.010,
        "max": 0.020,
        "mean": 0.015,
        "stddev": 0.002,
        "rounds": 3,
        "iterations": 1,
        "median": 0.015,
        "data": [0.010, 0.020]
      }
    }
  ]
}"#,
    )
    .expect("failed to write pytest-benchmark input");

    let mut cmd = perfgate_cmd();
    cmd.arg("ingest")
        .arg("--format")
        .arg("pytest-benchmark")
        .arg("--input")
        .arg(&input_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("data length"));
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
