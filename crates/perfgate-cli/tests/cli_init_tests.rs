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
