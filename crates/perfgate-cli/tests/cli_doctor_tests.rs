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
