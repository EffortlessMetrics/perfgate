//! Integration tests for `perfgate diff` command
//!
//! **Validates: Git-aware zero-argument comparison workflow**

use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

/// Returns a cross-platform command that exits successfully.
#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

/// Create a minimal config file with a single bench.
fn create_config(dir: &std::path::Path, bench_name: &str) -> std::path::PathBuf {
    let config_path = dir.join("perfgate.toml");
    let cmd = success_command();
    let cmd_str = cmd
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let config_content = format!(
        r#"
[defaults]
repeat = 2
warmup = 0
threshold = 0.20

[[bench]]
name = "{}"
command = [{}]
"#,
        bench_name, cmd_str
    );

    fs::write(&config_path, config_content).expect("write config");
    config_path
}

#[test]
fn diff_help_shows_usage() {
    perfgate_cmd()
        .args(["diff", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Quick"))
        .stdout(predicate::str::contains("--bench"))
        .stdout(predicate::str::contains("--quick"))
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn diff_no_config_fails_gracefully() {
    let tmp = tempdir().unwrap();

    perfgate_cmd()
        .args([
            "diff",
            "--config",
            &tmp.path().join("nonexistent.toml").display().to_string(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn diff_with_config_no_baseline_shows_skip() {
    let tmp = tempdir().unwrap();
    let config = create_config(tmp.path(), "my-bench");

    perfgate_cmd()
        .args(["diff", "--config", &config.display().to_string()])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-bench"))
        .stdout(predicate::str::contains("no baseline found"));
}

#[test]
fn diff_with_config_json_output() {
    let tmp = tempdir().unwrap();
    let config = create_config(tmp.path(), "my-bench");

    let output = perfgate_cmd()
        .args(["diff", "--config", &config.display().to_string(), "--json"])
        .output()
        .expect("run diff");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8");
    let parsed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(parsed["exit_code"], 0);
    assert!(parsed["benchmarks"].is_array());
    assert_eq!(parsed["benchmarks"][0]["bench"], "my-bench");
    assert_eq!(parsed["benchmarks"][0]["no_baseline"], true);
}

#[test]
fn diff_bench_filter_unknown_name_fails() {
    let tmp = tempdir().unwrap();
    let config = create_config(tmp.path(), "my-bench");

    perfgate_cmd()
        .args([
            "diff",
            "--config",
            &config.display().to_string(),
            "--bench",
            "nonexistent",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn diff_bench_filter_runs_single_bench() {
    let tmp = tempdir().unwrap();
    let config = create_config(tmp.path(), "my-bench");

    perfgate_cmd()
        .args([
            "diff",
            "--config",
            &config.display().to_string(),
            "--bench",
            "my-bench",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-bench"));
}

#[test]
fn diff_quick_mode_runs_successfully() {
    let tmp = tempdir().unwrap();
    let config = create_config(tmp.path(), "my-bench");

    perfgate_cmd()
        .args(["diff", "--config", &config.display().to_string(), "--quick"])
        .assert()
        .success()
        .stdout(predicate::str::contains("my-bench"));
}

#[test]
fn diff_with_baseline_shows_comparison() {
    let tmp = tempdir().unwrap();
    let config = create_config(tmp.path(), "my-bench");

    // First, run a benchmark to create a baseline
    let baselines_dir = tmp.path().join("baselines");
    fs::create_dir_all(&baselines_dir).expect("create baselines dir");

    let run_out = tmp.path().join("run.json");
    perfgate_cmd()
        .args([
            "run",
            "--name",
            "my-bench",
            "--repeat",
            "2",
            "--out",
            &run_out.display().to_string(),
            "--",
        ])
        .args(success_command())
        .assert()
        .success();

    // Promote the run to baseline
    let baseline_path = baselines_dir.join("my-bench.json");
    perfgate_cmd()
        .args([
            "promote",
            "--current",
            &run_out.display().to_string(),
            "--to",
            &baseline_path.display().to_string(),
        ])
        .assert()
        .success();

    // Now run diff from the temp dir (so baselines/ resolves correctly).
    // We don't assert on exit code because the fast command has high variance
    // and may trigger a regression verdict.
    let output = perfgate_cmd()
        .current_dir(tmp.path())
        .args(["diff", "--config", &config.display().to_string()])
        .output()
        .expect("run diff");

    let stdout = String::from_utf8(output.stdout).expect("UTF-8 stdout");
    assert!(
        stdout.contains("my-bench"),
        "expected bench name in output: {}",
        stdout
    );
    assert!(
        stdout.contains("verdict"),
        "expected verdict in output: {}",
        stdout
    );
}
