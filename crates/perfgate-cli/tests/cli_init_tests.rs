//! Integration tests for `perfgate init`.

use std::fs;

mod common;
use common::perfgate_cmd;

#[test]
fn init_github_profile_standard_writes_paved_road_files() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    fs::write(
        temp_dir.path().join("Cargo.toml"),
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

    let output = perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--ci", "github", "--profile", "standard"])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(output).expect("stderr is utf8");

    let config_path = temp_dir.path().join("perfgate.toml");
    let workflow_path = temp_dir.path().join(".github/workflows/perfgate.yml");
    let gitkeep_path = temp_dir.path().join("baselines/.gitkeep");
    let onboarding_path = temp_dir.path().join(".perfgate/README.md");

    assert!(config_path.exists(), "perfgate.toml should be generated");
    assert!(
        workflow_path.exists(),
        "GitHub workflow should be generated"
    );
    assert!(
        gitkeep_path.exists(),
        "baseline placeholder should be generated"
    );
    assert!(
        onboarding_path.exists(),
        "onboarding README should be generated"
    );

    let config = fs::read_to_string(&config_path).expect("read generated config");
    assert!(config.contains("repeat = 7"));
    assert!(config.contains("threshold = 0.20"));
    assert!(config.contains("warn_factor = 0.50"));
    assert!(config.contains("noise_policy = \"warn\""));
    assert!(config.contains("out_dir = \"artifacts/perfgate\""));
    assert!(config.contains("baseline_dir = \"baselines\""));
    assert!(config.contains("name = \"parser\""));

    let workflow = fs::read_to_string(&workflow_path).expect("read generated workflow");
    assert!(workflow.contains("EffortlessMetrics/perfgate@v0"));
    assert!(workflow.contains("config: perfgate.toml"));
    assert!(workflow.contains("require_baseline: \"true\""));

    let onboarding = fs::read_to_string(&onboarding_path).expect("read onboarding README");
    assert!(onboarding.contains("artifacts/perfgate/"));
    assert!(onboarding.contains("baselines/"));
    assert!(onboarding.contains("perfgate check --config perfgate.toml --all"));

    assert!(stderr.contains("Discovered 1 benchmark(s):"));
    assert!(stderr.contains("Wrote baselines"));
    assert!(stderr.contains("Next:"));
    assert!(stderr.contains("perfgate check --config perfgate.toml --all"));
    assert!(stderr.contains("perfgate baseline promote --config perfgate.toml --all"));
}

#[test]
fn init_accepts_legacy_preset_alias() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--preset", "tier1-fast"])
        .assert()
        .success();

    let config =
        fs::read_to_string(temp_dir.path().join("perfgate.toml")).expect("read generated config");
    assert!(config.contains("repeat = 3"));
    assert!(config.contains("threshold = 0.30"));
}

#[test]
fn init_without_discovered_benchmarks_points_to_bench_entry_first() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let output = perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--ci", "github", "--profile", "standard"])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(output).expect("stderr is utf8");

    assert!(stderr.contains("No benchmarks discovered"));
    assert!(stderr.contains("Add at least one [[bench]] entry to perfgate.toml."));
    assert!(stderr.contains("your-benchmark-command"));
    assert!(stderr.contains("[\"node\", \"scripts/bench.js\"]"));
    assert!(stderr.contains("perfgate check --config perfgate.toml --all"));

    let config =
        fs::read_to_string(temp_dir.path().join("perfgate.toml")).expect("read generated config");
    assert!(!config.contains("[[bench]]"));

    let onboarding = fs::read_to_string(temp_dir.path().join(".perfgate/README.md"))
        .expect("read onboarding README");
    assert!(onboarding.contains("Add at least one `[[bench]]` entry"));
    assert!(onboarding.contains("your-benchmark-command"));
    assert!(onboarding.contains("[\"node\", \"scripts/bench.js\"]"));
    assert!(onboarding.contains("perfgate check --config perfgate.toml --all"));
}

#[test]
fn init_suggest_benches_generates_language_neutral_commented_candidates() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    let output = perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "init",
            "--ci",
            "github",
            "--profile",
            "standard",
            "--suggest-benches",
        ])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8(output).expect("stderr is utf8");

    let config =
        fs::read_to_string(temp_dir.path().join("perfgate.toml")).expect("read generated config");
    assert!(config.contains("# Benchmark suggestions (generic-command)"));
    assert!(config.contains("# Review and edit before committing."));
    assert!(config.contains("# [[bench]]"));
    assert!(config.contains("# name = \"command-smoke\""));
    assert!(config.contains("# command = [\"./scripts/bench.sh\"]"));
    assert!(config.contains("# command = [\"your-benchmark-command\", \"--flag\"]"));
    assert!(
        !config.contains("\n[[bench]]"),
        "suggestions must stay commented until reviewed"
    );

    assert!(stderr.contains("Appended reviewable benchmark suggestions (generic-command)"));
    assert!(stderr.contains("Review and edit suggestions before committing baselines."));
}

#[test]
fn init_suggest_benches_accepts_explicit_rust_cli_profile() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--suggest-benches", "rust-cli"])
        .assert()
        .success();

    let config =
        fs::read_to_string(temp_dir.path().join("perfgate.toml")).expect("read generated config");
    assert!(config.contains("# Benchmark suggestions (rust-cli)"));
    assert!(config.contains("# name = \"cli-help\""));
    assert!(config.contains("# command = [\"cargo\", \"run\", \"-q\", \"--\", \"--help\"]"));
    assert!(
        !config.contains("\n[[bench]]"),
        "explicit suggestions must stay commented until reviewed"
    );
}

#[test]
fn init_suggest_benches_auto_detects_node_repo() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    fs::write(
        temp_dir.path().join("package.json"),
        r#"{"scripts":{"bench":"node scripts/bench.js"}}"#,
    )
    .expect("write package.json");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["init", "--suggest-benches"])
        .assert()
        .success();

    let config =
        fs::read_to_string(temp_dir.path().join("perfgate.toml")).expect("read generated config");
    assert!(config.contains("# Benchmark suggestions (node)"));
    assert!(config.contains("# command = [\"node\", \"scripts/bench.js\"]"));
    assert!(config.contains("# command = [\"npm\", \"run\", \"bench\"]"));
}
