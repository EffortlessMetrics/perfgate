//! Integration tests for `perfgate doctor`.

use predicates::prelude::*;
use std::fs;
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

fn write_config(dir: &std::path::Path) -> std::path::PathBuf {
    let config_path = dir.join("perfgate.toml");
    let command = success_command()
        .iter()
        .map(|part| format!("\"{}\"", part))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        &config_path,
        format!(
            r#"[defaults]
repeat = 1
warmup = 0
baseline_dir = "baselines"

[[bench]]
name = "doctor-bench"
command = [{command}]
"#
        ),
    )
    .expect("write config");
    config_path
}

fn write_zero_bench_config(dir: &std::path::Path) -> std::path::PathBuf {
    let config_path = dir.join("perfgate.toml");
    fs::write(
        &config_path,
        r#"[defaults]
repeat = 1
warmup = 0
baseline_dir = "baselines"
"#,
    )
    .expect("write config");
    config_path
}

fn write_baseline_marker(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("baselines")).expect("create baselines");
    fs::write(
        dir.join("baselines/doctor-bench.json"),
        r#"{"schema":"perfgate.run.v1"}"#,
    )
    .expect("write baseline marker");
}

fn write_signal_receipt(path: &std::path::Path, bench: &str, sample_count: usize, cv: f64) {
    let started_at = chrono::Utc::now() - chrono::Duration::days(1);
    let samples = (0..sample_count)
        .map(|_| {
            serde_json::json!({
                "wall_ms": 100,
                "exit_code": 0,
                "warmup": false,
                "timed_out": false
            })
        })
        .collect::<Vec<_>>();
    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": { "name": "perfgate", "version": "0.18.0" },
        "run": {
            "id": format!("{bench}-run"),
            "started_at": started_at.to_rfc3339(),
            "ended_at": (started_at + chrono::Duration::seconds(1)).to_rfc3339(),
            "host": {
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH
            }
        },
        "bench": {
            "name": bench,
            "command": success_command(),
            "repeat": sample_count as u32,
            "warmup": 0
        },
        "samples": samples,
        "stats": {
            "wall_ms": {
                "median": 100,
                "min": 98,
                "max": 102,
                "mean": 100.0,
                "stddev": 100.0 * cv
            }
        }
    });
    fs::create_dir_all(path.parent().expect("receipt parent")).expect("create receipt parent");
    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize receipt"),
    )
    .expect("write receipt");
}

fn write_signal_compare(path: &std::path::Path, bench: &str) {
    let compare = serde_json::json!({
        "schema": "perfgate.compare.v1",
        "tool": { "name": "perfgate", "version": "0.18.0" },
        "bench": {
            "name": bench,
            "command": success_command(),
            "repeat": 7,
            "warmup": 0
        },
        "baseline_ref": { "path": "baselines/doctor-bench.json", "run_id": "baseline-run" },
        "current_ref": { "path": "artifacts/perfgate/doctor-bench/run.json", "run_id": "current-run" },
        "budgets": {
            "wall_ms": {
                "threshold": 0.20,
                "warn_threshold": 0.10,
                "direction": "lower"
            }
        },
        "deltas": {
            "wall_ms": {
                "baseline": 100.0,
                "current": 101.0,
                "ratio": 1.01,
                "pct": 0.01,
                "regression": 0.01,
                "cv": 0.03,
                "status": "pass"
            }
        },
        "verdict": {
            "status": "pass",
            "counts": { "pass": 1, "warn": 0, "fail": 0, "skip": 0 },
            "reasons": []
        }
    });
    fs::create_dir_all(path.parent().expect("compare parent")).expect("create compare parent");
    fs::write(
        path,
        serde_json::to_string_pretty(&compare).expect("serialize compare"),
    )
    .expect("write compare");
}

#[test]
fn doctor_reports_local_setup_without_running_benchmarks() {
    let dir = tempdir().expect("tempdir");
    let config = write_config(dir.path());
    let out_dir = dir.path().join("artifacts/perfgate");

    perfgate_cmd()
        .args(["doctor", "--config"])
        .arg(&config)
        .arg("--out-dir")
        .arg(&out_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("perfgate doctor"))
        .stdout(predicate::str::contains("OK   version"))
        .stdout(predicate::str::contains("OK   config"))
        .stdout(predicate::str::contains("OK   benchmarks"))
        .stdout(predicate::str::contains("WARN baselines"))
        .stdout(predicate::str::contains("OK   artifact directory"))
        .stdout(predicate::str::contains("State: benches_no_baselines"))
        .stdout(predicate::str::contains(
            "Meaning: Benchmarks are configured, but setup is incomplete",
        ))
        .stdout(predicate::str::contains("perfgate check --config"))
        .stdout(predicate::str::contains(
            "do not loosen thresholds to fix missing baseline setup",
        ))
        .stdout(predicate::str::contains("Summary: 0 failed"));
}

#[test]
fn doctor_signal_reports_safe_to_gate_when_receipts_are_mature() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_signal_receipt(
        &dir.path().join("baselines/doctor-bench.json"),
        "doctor-bench",
        7,
        0.03,
    );
    write_signal_receipt(
        &dir.path().join("artifacts/perfgate/doctor-bench/run.json"),
        "doctor-bench",
        7,
        0.03,
    );
    write_signal_compare(
        &dir.path()
            .join("artifacts/perfgate/doctor-bench/compare.json"),
        "doctor-bench",
    );

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["doctor", "signal", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("perfgate doctor signal"))
        .stdout(predicate::str::contains("bench: doctor-bench"))
        .stdout(predicate::str::contains("samples: 7 measured samples"))
        .stdout(predicate::str::contains("cv: 3.0%"))
        .stdout(predicate::str::contains("recent drift: pass"))
        .stdout(predicate::str::contains("recommendation: safe_to_gate"))
        .stdout(predicate::str::contains("perfgate check --config"))
        .stdout(predicate::str::contains("--require-baseline"))
        .stdout(predicate::str::contains("Advisory only: no config"));
}

#[test]
fn doctor_signal_recommends_paired_mode_for_noisy_signal() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_signal_receipt(
        &dir.path().join("baselines/doctor-bench.json"),
        "doctor-bench",
        7,
        0.20,
    );
    write_signal_receipt(
        &dir.path().join("artifacts/perfgate/doctor-bench/run.json"),
        "doctor-bench",
        7,
        0.20,
    );

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["doctor", "signal", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recommendation: use_paired_mode"))
        .stdout(predicate::str::contains("ordinary runs are noisy"))
        .stdout(predicate::str::contains("perfgate paired"));
}

#[test]
fn doctor_signal_treats_missing_baseline_as_incomplete_setup() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_signal_receipt(
        &dir.path().join("artifacts/perfgate/doctor-bench/run.json"),
        "doctor-bench",
        7,
        0.03,
    );

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["doctor", "signal", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("baseline: baselines"))
        .stdout(predicate::str::contains("(missing)"))
        .stdout(predicate::str::contains("recommendation: no_decision_yet"))
        .stdout(predicate::str::contains("setup or receipts are incomplete"))
        .stdout(predicate::str::contains("perfgate baseline promote"));
}

#[test]
fn doctor_reports_no_config_state_and_next_command() {
    let dir = tempdir().expect("tempdir");
    let missing_config = dir.path().join("missing-perfgate.toml");

    perfgate_cmd()
        .args(["doctor", "--config"])
        .arg(&missing_config)
        .arg("--out-dir")
        .arg(dir.path().join("artifacts/perfgate"))
        .assert()
        .success()
        .stdout(predicate::str::contains("FAIL config"))
        .stdout(predicate::str::contains("State: no_config"))
        .stdout(predicate::str::contains(
            "perfgate init --ci github --profile standard",
        ))
        .stdout(predicate::str::contains(
            "do not copy another repo's baselines before initializing this repo",
        ));
}

#[test]
fn doctor_reports_configured_no_benches_state() {
    let dir = tempdir().expect("tempdir");
    let config = write_zero_bench_config(dir.path());

    perfgate_cmd()
        .args(["doctor", "--config"])
        .arg(&config)
        .arg("--out-dir")
        .arg(dir.path().join("artifacts/perfgate"))
        .assert()
        .success()
        .stdout(predicate::str::contains("FAIL benchmarks"))
        .stdout(predicate::str::contains("State: configured_no_benches"))
        .stdout(predicate::str::contains("add a reviewed [[bench]] command"))
        .stdout(predicate::str::contains(
            "do not promote a baseline until the benchmark command measures the workload you care about",
        ));
}

#[test]
fn doctor_reports_ready_local_when_baselines_exist() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_baseline_marker(dir.path());

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["doctor", "--config", "perfgate.toml"])
        .arg("--out-dir")
        .arg(dir.path().join("artifacts/perfgate"))
        .assert()
        .success()
        .stdout(predicate::str::contains("OK   baselines"))
        .stdout(predicate::str::contains("State: ready_local"))
        .stdout(predicate::str::contains(
            "perfgate check --config perfgate.toml --all --require-baseline",
        ))
        .stdout(predicate::str::contains(
            "do not enable required CI before committing reviewed baselines",
        ));
}

#[test]
fn doctor_reports_ready_ci_when_workflow_and_baselines_exist() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_baseline_marker(dir.path());
    let workflow_path = dir.path().join(".github/workflows/perfgate.yml");
    fs::create_dir_all(workflow_path.parent().expect("workflow parent"))
        .expect("create workflow dir");
    fs::write(&workflow_path, "name: perfgate\n").expect("write workflow");

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["doctor", "--config", "perfgate.toml"])
        .arg("--out-dir")
        .arg(dir.path().join("artifacts/perfgate"))
        .assert()
        .success()
        .stdout(predicate::str::contains("State: ready_ci"))
        .stdout(predicate::str::contains(
            "Local baselines and the generated GitHub Action workflow are present.",
        ))
        .stdout(predicate::str::contains(
            "do not debug CI before trying the local reproduction command",
        ));
}

#[test]
fn doctor_strict_fails_when_required_setup_is_missing() {
    let dir = tempdir().expect("tempdir");
    let missing_config = dir.path().join("missing-perfgate.toml");

    perfgate_cmd()
        .args(["doctor", "--strict", "--config"])
        .arg(&missing_config)
        .arg("--out-dir")
        .arg(dir.path().join("artifacts/perfgate"))
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAIL config"))
        .stdout(predicate::str::contains("Summary: 1 failed"))
        .stderr(predicate::str::contains("doctor found 1 failed check"));
}

#[test]
fn doctor_uses_current_directory_for_relative_config_paths_like_check() {
    let dir = tempdir().expect("tempdir");
    let project_dir = dir.path().join("project");
    fs::create_dir(&project_dir).expect("create project dir");
    fs::create_dir(project_dir.join("baselines")).expect("create project baselines");
    fs::write(project_dir.join("bench.cmd"), "@echo off\r\nexit /b 0\r\n").expect("write bench");
    fs::write(
        project_dir.join("baselines/doctor-bench.json"),
        r#"{"schema":"perfgate.run.v1"}"#,
    )
    .expect("write baseline marker");
    let config = project_dir.join("perfgate.toml");
    fs::write(
        &config,
        r#"[defaults]
baseline_dir = "baselines"

[[bench]]
name = "doctor-bench"
command = ["./bench.cmd"]
"#,
    )
    .expect("write config");

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["doctor", "--config"])
        .arg(&config)
        .arg("--out-dir")
        .arg(dir.path().join("artifacts/perfgate"))
        .assert()
        .success()
        .stdout(predicate::str::contains("FAIL benchmarks"))
        .stdout(predicate::str::contains("WARN baselines"))
        .stdout(predicate::str::contains("0/1 local baseline found"));
}

#[cfg(unix)]
#[test]
fn doctor_rejects_non_executable_relative_programs_on_unix() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("tempdir");
    let script = dir.path().join("bench.sh");
    fs::write(&script, "#!/bin/sh\nexit 0\n").expect("write script");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o644)).expect("chmod script");
    let config = dir.path().join("perfgate.toml");
    fs::write(
        &config,
        r#"[[bench]]
name = "doctor-bench"
command = ["./bench.sh"]
"#,
    )
    .expect("write config");

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["doctor", "--config"])
        .arg(&config)
        .arg("--out-dir")
        .arg(dir.path().join("artifacts/perfgate"))
        .assert()
        .success()
        .stdout(predicate::str::contains("FAIL benchmarks"));
}
