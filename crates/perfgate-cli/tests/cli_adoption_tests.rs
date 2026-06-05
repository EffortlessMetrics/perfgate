//! Integration tests for adoption-pack catalog surfaces.

use predicates::prelude::*;
use serde_json::Value;

mod common;
use common::perfgate_cmd;

#[test]
fn adoption_packs_lists_reviewable_catalog() {
    perfgate_cmd()
        .args(["adoption", "packs"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Adoption packs are reviewable starting points",
        ))
        .stdout(predicate::str::contains(
            "They do not detect benchmarks magically",
        ))
        .stdout(predicate::str::contains("Pack: rust-cli"))
        .stdout(predicate::str::contains("Pack: rust-workspace"))
        .stdout(predicate::str::contains("Pack: python-service"))
        .stdout(predicate::str::contains("Pack: node-tool-action"))
        .stdout(predicate::str::contains("Pack: http-local-smoke"))
        .stdout(predicate::str::contains("Pack: generic-command"))
        .stdout(predicate::str::contains("Local reproduction:"))
        .stdout(predicate::str::contains("Do not infer:"));
}

#[test]
fn adoption_packs_can_show_one_pack() {
    perfgate_cmd()
        .args(["adoption", "packs", "--pack", "http-local-smoke"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Pack: http-local-smoke"))
        .stdout(predicate::str::contains("k6 summary JSON"))
        .stdout(predicate::str::contains("production capacity proof"))
        .stdout(predicate::str::contains("Pack: rust-cli").not());
}

#[test]
fn adoption_packs_rejects_unknown_pack() {
    perfgate_cmd()
        .args(["adoption", "packs", "--pack", "mobile-app"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn adoption_recommend_detects_rust_cli_shape() {
    let repo = tempfile::tempdir().expect("temp repo");
    std::fs::write(
        repo.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\n",
    )
    .expect("write manifest");
    std::fs::create_dir(repo.path().join("src")).expect("create src");
    std::fs::write(repo.path().join("src/main.rs"), "fn main() {}\n").expect("write main");

    perfgate_cmd()
        .args(["adoption", "recommend", "--path"])
        .arg(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Recommended adoption pack: rust-cli",
        ))
        .stdout(predicate::str::contains("Confidence: high"))
        .stdout(predicate::str::contains("found Cargo.toml"))
        .stdout(predicate::str::contains("Not inspected:"))
        .stdout(predicate::str::contains(
            "Next: perfgate adoption packs --pack rust-cli",
        ));
}

#[test]
fn adoption_recommend_json_detects_rust_workspace_shape() {
    let repo = tempfile::tempdir().expect("temp repo");
    std::fs::write(
        repo.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/demo\"]\n",
    )
    .expect("write manifest");

    let output = perfgate_cmd()
        .args(["adoption", "recommend", "--json", "--path"])
        .arg(repo.path())
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let value: Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(value["recommended_pack"], "rust-workspace");
    assert_eq!(value["confidence"], "high");
    assert!(
        value["inspected"]
            .as_array()
            .expect("inspected array")
            .iter()
            .any(|item| item == "Cargo.toml contains [workspace]")
    );
}

#[test]
fn adoption_recommend_falls_back_to_generic_command() {
    let repo = tempfile::tempdir().expect("temp repo");

    perfgate_cmd()
        .args(["adoption", "recommend", "--path"])
        .arg(repo.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Recommended adoption pack: generic-command",
        ))
        .stdout(predicate::str::contains("Confidence: low"))
        .stdout(predicate::str::contains(
            "no known framework markers were found",
        ));
}

#[test]
fn adoption_recommend_rejects_missing_path() {
    let repo = tempfile::tempdir().expect("temp repo");
    let missing = repo.path().join("missing");

    perfgate_cmd()
        .args(["adoption", "recommend", "--path"])
        .arg(missing)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "failed to resolve adoption recommend path",
        ));
}

#[test]
fn adoption_apply_dry_run_writes_review_artifacts_without_setup_mutation() {
    let repo = tempfile::tempdir().expect("temp repo");

    perfgate_cmd()
        .current_dir(repo.path())
        .args([
            "adoption",
            "apply",
            "--pack",
            "rust-cli",
            "--ci",
            "github",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Dry-run adoption artifacts written to target/perfgate-adoption",
        ))
        .stdout(predicate::str::contains("perfgate.toml.patch"))
        .stdout(predicate::str::contains("github-workflow.yml"))
        .stdout(predicate::str::contains("local-commands.md"))
        .stdout(predicate::str::contains("non-inferences.md"))
        .stdout(predicate::str::contains(
            "No perfgate.toml, workflow, baselines, thresholds, required gates, or server ledger settings were changed.",
        ));

    assert!(
        !repo.path().join("perfgate.toml").exists(),
        "dry-run must not write perfgate.toml"
    );
    assert!(
        !repo
            .path()
            .join(".github")
            .join("workflows")
            .join("perfgate.yml")
            .exists(),
        "dry-run must not write workflow"
    );
    assert!(
        !repo.path().join("baselines").join(".gitkeep").exists(),
        "dry-run must not write baselines"
    );
    assert!(
        !repo.path().join(".perfgate").join("README.md").exists(),
        "dry-run must not write onboarding README"
    );

    let out_dir = repo.path().join("target").join("perfgate-adoption");
    let config_patch =
        std::fs::read_to_string(out_dir.join("perfgate.toml.patch")).expect("config patch");
    let workflow = std::fs::read_to_string(out_dir.join("github-workflow.yml")).expect("workflow");
    let local_commands =
        std::fs::read_to_string(out_dir.join("local-commands.md")).expect("local commands");
    let non_inferences =
        std::fs::read_to_string(out_dir.join("non-inferences.md")).expect("non-inferences");

    assert!(config_patch.contains("# Pack: rust-cli"));
    assert!(config_patch.contains("name = \"cli-help\""));
    assert!(config_patch.contains("command = [\"cargo\", \"run\", \"-q\", \"--\", \"--help\"]"));
    assert!(workflow.contains("actions/checkout@v6"));
    assert!(workflow.contains("EffortlessMetrics/perfgate@v0"));
    assert!(workflow.contains("require_baseline: \"false\""));
    assert!(local_commands.contains("perfgate check --config perfgate.toml --all"));
    assert!(local_commands.contains("perfgate policy review-packet"));
    assert!(non_inferences.contains("policy should become required_gate"));
    assert!(non_inferences.contains("server ledger history is required for correctness"));
}

#[test]
fn adoption_apply_requires_dry_run() {
    perfgate_cmd()
        .args(["adoption", "apply", "--pack", "rust-cli", "--ci", "github"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("rerun with --dry-run"));
}

#[test]
fn adoption_apply_rejects_unknown_ci_platform() {
    perfgate_cmd()
        .args([
            "adoption",
            "apply",
            "--pack",
            "rust-cli",
            "--ci",
            "gitlab",
            "--dry-run",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}
