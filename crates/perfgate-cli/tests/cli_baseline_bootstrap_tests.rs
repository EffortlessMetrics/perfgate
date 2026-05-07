//! Integration tests for local baseline bootstrap commands.

use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::{fixtures_dir, perfgate_cmd};

fn write_config(dir: &Path) {
    fs::write(
        dir.join("perfgate.toml"),
        r#"[defaults]
out_dir = "artifacts/perfgate"
baseline_dir = "baselines"

[[bench]]
name = "test-benchmark"
command = ["echo", "hello"]
"#,
    )
    .expect("write config");
}

#[test]
fn baseline_status_reports_missing_then_found_local_baseline() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "status", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Baseline status"))
        .stdout(predicate::str::contains("MISSING test-benchmark"))
        .stdout(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --bench test-benchmark",
        ));

    fs::create_dir_all(temp_dir.path().join("baselines")).expect("create baselines");
    fs::copy(
        fixtures_dir().join("baseline.json"),
        temp_dir.path().join("baselines/test-benchmark.json"),
    )
    .expect("copy baseline fixture");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "status", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("FOUND   test-benchmark"))
        .stdout(predicate::str::contains(
            "Summary: 1/1 local baseline found",
        ));
}

#[test]
fn baseline_init_creates_gitkeep_for_configured_baseline_dir() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "init", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote baselines"))
        .stdout(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --bench <bench>",
        ));

    assert!(
        temp_dir.path().join("baselines/.gitkeep").exists(),
        "baseline init should create baselines/.gitkeep"
    );
}

#[test]
fn baseline_promote_uses_check_all_artifact_convention() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());
    let run_dir = temp_dir.path().join("artifacts/perfgate/test-benchmark");
    fs::create_dir_all(&run_dir).expect("create run dir");
    fs::copy(
        fixtures_dir().join("baseline.json"),
        run_dir.join("run.json"),
    )
    .expect("copy run fixture");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "promote",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Promoted baseline for test-benchmark",
        ));

    let baseline_path = temp_dir.path().join("baselines/test-benchmark.json");
    assert!(
        baseline_path.exists(),
        "baseline promote should write the configured baseline path"
    );
    let promoted: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(baseline_path).expect("read promoted baseline"))
            .expect("promoted baseline is json");
    assert_eq!(promoted["schema"].as_str(), Some("perfgate.run.v1"));
}

#[test]
fn baseline_promote_also_accepts_single_bench_artifact_convention() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());
    let run_dir = temp_dir.path().join("artifacts/perfgate");
    fs::create_dir_all(&run_dir).expect("create run dir");
    fs::copy(
        fixtures_dir().join("baseline.json"),
        run_dir.join("run.json"),
    )
    .expect("copy run fixture");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "promote",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("artifacts/perfgate"))
        .stderr(predicate::str::contains("run.json"));

    assert!(
        temp_dir
            .path()
            .join("baselines/test-benchmark.json")
            .exists(),
        "baseline promote should accept the single-bench artifact path"
    );
}

#[test]
fn baseline_promote_missing_default_artifact_teaches_next_command() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "promote",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("run receipt not found"))
        .stderr(predicate::str::contains(
            "perfgate check --config perfgate.toml --all",
        ));
}
