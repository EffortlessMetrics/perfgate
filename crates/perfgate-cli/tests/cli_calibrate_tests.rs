//! Integration tests for `perfgate calibrate`.

use std::fs;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

fn write_config(root: &std::path::Path) -> std::path::PathBuf {
    let config_path = root.join("perfgate.toml");
    fs::write(
        &config_path,
        r#"
[defaults]
repeat = 5
warmup = 0
threshold = 0.20
out_dir = "artifacts/perfgate"

[[bench]]
name = "parser"
command = ["echo", "parser"]
"#,
    )
    .expect("write config");
    config_path
}

fn write_run_receipt(path: &std::path::Path, bench_name: &str, mean: f64, stddev: f64) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create receipt parent");
    }
    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {
            "name": "perfgate",
            "version": "0.18.0"
        },
        "run": {
            "id": "run-id",
            "started_at": "2026-01-01T00:00:00Z",
            "ended_at": "2026-01-01T00:00:01Z",
            "host": {
                "os": "linux",
                "arch": "x86_64"
            }
        },
        "bench": {
            "name": bench_name,
            "command": ["echo", "parser"],
            "repeat": 3,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false},
            {"wall_ms": 104, "exit_code": 0, "warmup": false, "timed_out": false},
            {"wall_ms": 96, "exit_code": 0, "warmup": false, "timed_out": false}
        ],
        "stats": {
            "wall_ms": {
                "median": 100,
                "min": 96,
                "max": 104,
                "mean": mean,
                "stddev": stddev
            }
        }
    });
    fs::write(path, serde_json::to_string_pretty(&receipt).unwrap()).expect("write receipt");
}

#[test]
fn calibrate_suggests_thresholds_from_existing_run_receipt() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_config(temp_dir.path());
    let out_dir = temp_dir.path().join("artifacts").join("perfgate");
    let run_path = out_dir.join("parser").join("run.json");
    write_run_receipt(&run_path, "parser", 100.0, 4.0);
    write_run_receipt(
        &temp_dir.path().join("baselines").join("parser.json"),
        "parser",
        100.0,
        4.0,
    );
    let original_config = fs::read_to_string(&config_path).expect("read config");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("calibrate")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("parser");

    let output = cmd.output().expect("run calibrate");
    assert!(
        output.status.success(),
        "calibrate should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Bench: parser"), "stdout: {stdout}");
    assert!(
        stdout.contains("Samples: 3 measured samples"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("CV: 4.0%"), "stdout: {stdout}");
    assert!(
        stdout.contains("Suggested fail threshold: 10.0%"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("Suggested warn threshold: 5.0%"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("noise_policy = \"warn\""),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("run with --emit-patch"), "stdout: {stdout}");
    assert!(
        stdout.contains("Advisory only: no config was written."),
        "stdout: {stdout}"
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("read config after calibrate"),
        original_config,
        "calibrate must not edit config in the advisory-only version"
    );
}

#[test]
fn calibrate_emit_patch_prints_reviewable_toml_without_editing_config() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_config(temp_dir.path());
    let out_dir = temp_dir.path().join("artifacts").join("perfgate");
    let run_path = out_dir.join("parser").join("run.json");
    write_run_receipt(&run_path, "parser", 100.0, 4.0);
    let original_config = fs::read_to_string(&config_path).expect("read config");

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("calibrate")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("parser")
        .arg("--emit-patch");

    let output = cmd.output().expect("run calibrate");
    assert!(
        output.status.success(),
        "calibrate should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Reviewable TOML patch:"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("# Suggested from 3 measured samples on linux-x86_64."),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("# CV: 4.0%; review before applying"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("repeat = 10"), "stdout: {stdout}");
    assert!(stdout.contains("Reasons:"), "stdout: {stdout}");
    assert!(stdout.contains("When not to apply:"), "stdout: {stdout}");
    assert!(
        stdout.contains("benchmark is not the workload reviewers want to gate"),
        "stdout: {stdout}"
    );
    assert_eq!(
        fs::read_to_string(&config_path).expect("read config after calibrate"),
        original_config,
        "calibrate --emit-patch must not edit config"
    );
}

#[test]
fn calibrate_handles_missing_run_receipt_with_next_check_command() {
    let temp_dir = tempdir().expect("temp dir");
    let config_path = write_config(temp_dir.path());

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("calibrate")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("parser");

    let output = cmd.output().expect("run calibrate");
    assert!(
        output.status.success(),
        "calibrate should succeed without receipts: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Samples: unavailable"), "stdout: {stdout}");
    assert!(stdout.contains("CV: unavailable"), "stdout: {stdout}");
    assert!(
        stdout.contains("perfgate check --config"),
        "stdout should include the next check command: {stdout}"
    );
    assert!(
        stdout.contains("Advisory only: no config was written."),
        "stdout: {stdout}"
    );
}
