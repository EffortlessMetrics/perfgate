//! End-to-end fixture for the paved first-run CLI path.

use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::perfgate_cmd;

fn write_minimal_rust_repo(dir: &Path) {
    fs::write(
        dir.join("Cargo.toml"),
        r#"
[package]
name = "example"
version = "0.1.0"
edition = "2021"

[[bench]]
name = "parser"
harness = false
"#,
    )
    .expect("write Cargo.toml");
}

#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

fn command_toml_array(command: &[&str]) -> String {
    command
        .iter()
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

fn tune_generated_config_for_fast_fixture(config_path: &Path) {
    let generated = fs::read_to_string(config_path).expect("read generated config");
    assert!(generated.contains("repeat = 7"));
    assert!(generated.contains("warmup = 1"));
    assert!(generated.contains(r#"command = ["cargo", "bench", "--bench", "parser"]"#));

    // The e2e target is the paved perfgate command sequence, not Cargo's bench
    // implementation. Keep init discovery covered, then use a fast command so
    // this integration test stays deterministic on every runner.
    let tuned = generated
        .replace("repeat = 7", "repeat = 1")
        .replace("warmup = 1", "warmup = 0")
        .replace(
            r#"command = ["cargo", "bench", "--bench", "parser"]"#,
            &format!(r#"command = [{}]"#, command_toml_array(&success_command())),
        );
    fs::write(config_path, tuned).expect("write tuned config");
}

#[test]
fn first_run_paved_road_creates_artifacts_and_baselines() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_minimal_rust_repo(temp_dir.path());

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--ci", "github", "--profile", "standard"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Discovered 1 benchmark(s):"))
        .stderr(predicate::str::contains(
            "perfgate check --config perfgate.toml --all",
        ))
        .stderr(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --all",
        ));

    let root = temp_dir.path();
    tune_generated_config_for_fast_fixture(&root.join("perfgate.toml"));

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["doctor", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("OK   config"))
        .stdout(predicate::str::contains("OK   benchmarks"))
        .stdout(predicate::str::contains("WARN baselines"))
        .stdout(predicate::str::contains("Summary: 0 failed"));

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["check", "--config", "perfgate.toml", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("parser"));

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "promote", "--config", "perfgate.toml", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains("Promoted baseline for parser"))
        .stderr(predicate::str::contains("Promoted 1 baseline"));

    assert!(root.join("perfgate.toml").exists());
    assert!(root.join(".github/workflows/perfgate.yml").exists());
    assert!(root.join(".perfgate/README.md").exists());
    assert!(root.join("baselines/.gitkeep").exists());
    assert!(root.join("baselines/parser.json").exists());
    assert!(root.join("artifacts/perfgate/parser/run.json").exists());
    assert!(root.join("artifacts/perfgate/parser/report.json").exists());
    assert!(root.join("artifacts/perfgate/parser/comment.md").exists());

    let baseline: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("baselines/parser.json")).expect("read baseline"),
    )
    .expect("baseline should be json");
    assert_eq!(baseline["schema"].as_str(), Some("perfgate.run.v1"));
    assert_eq!(baseline["bench"]["name"].as_str(), Some("parser"));
}
