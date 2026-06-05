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

fn write_two_bench_config(dir: &Path) {
    fs::write(
        dir.join("perfgate.toml"),
        r#"[defaults]
out_dir = "artifacts/perfgate"
baseline_dir = "baselines"

[[bench]]
name = "test-benchmark"
command = ["echo", "hello"]

[[bench]]
name = "second-benchmark"
command = ["echo", "world"]
"#,
    )
    .expect("write config");
}

fn write_run_fixture(path: &Path, bench: &str) {
    let mut receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(fixtures_dir().join("baseline.json")).unwrap())
            .expect("parse run fixture");
    receipt["bench"]["name"] = serde_json::json!(bench);
    fs::create_dir_all(path.parent().expect("run fixture has parent")).expect("create run parent");
    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize run fixture"),
    )
    .expect("write run fixture");
}

fn write_baseline_maturity_fixture(
    path: &Path,
    bench: &str,
    sample_count: usize,
    cv: f64,
    days_old: i64,
) {
    let mut receipt: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(fixtures_dir().join("baseline.json")).unwrap())
            .expect("parse run fixture");
    receipt["bench"]["name"] = serde_json::json!(bench);
    receipt["run"]["host"]["os"] = serde_json::json!(std::env::consts::OS);
    receipt["run"]["host"]["arch"] = serde_json::json!(std::env::consts::ARCH);
    let started_at = chrono::Utc::now() - chrono::Duration::days(days_old);
    receipt["run"]["started_at"] = serde_json::json!(started_at.to_rfc3339());
    receipt["run"]["ended_at"] =
        serde_json::json!((started_at + chrono::Duration::seconds(1)).to_rfc3339());
    receipt["stats"]["wall_ms"]["mean"] = serde_json::json!(100.0);
    receipt["stats"]["wall_ms"]["stddev"] = serde_json::json!(100.0 * cv);
    receipt["samples"] = serde_json::json!(
        (0..sample_count)
            .map(|_| serde_json::json!({
                "wall_ms": 100,
                "exit_code": 0,
                "warmup": false,
                "timed_out": false
            }))
            .collect::<Vec<_>>()
    );
    fs::create_dir_all(path.parent().expect("baseline fixture has parent"))
        .expect("create baseline parent");
    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize baseline fixture"),
    )
    .expect("write baseline fixture");
}

fn write_imported_summary_fixture(path: &Path, bench: &str) {
    let started_at = chrono::Utc::now() - chrono::Duration::days(1);
    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": { "name": "perfgate-ingest", "version": "0.21.0" },
        "run": {
            "id": format!("{bench}-imported"),
            "started_at": started_at.to_rfc3339(),
            "ended_at": (started_at + chrono::Duration::seconds(1)).to_rfc3339(),
            "host": {
                "os": "unknown",
                "arch": "unknown"
            }
        },
        "bench": {
            "name": bench,
            "command": [
                "(ingested k6 summary JSON)",
                "sample_model=summary_only",
                "capacity_proof=not_production"
            ],
            "repeat": 0,
            "warmup": 0
        },
        "samples": [],
        "stats": {
            "wall_ms": {
                "median": 120,
                "min": 100,
                "max": 150,
                "mean": 122.0,
                "stddev": 12.0
            },
            "throughput_per_s": {
                "median": 50.0,
                "min": 45.0,
                "max": 55.0,
                "mean": 50.0
            }
        }
    });
    fs::create_dir_all(path.parent().expect("imported fixture has parent"))
        .expect("create imported parent");
    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize imported fixture"),
    )
    .expect("write imported fixture");
}

#[test]
fn baseline_status_fails_when_config_is_missing() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "status", "--config", "missing.toml"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "baseline config missing.toml not found",
        ))
        .stderr(predicate::str::contains(
            "perfgate init --ci github --profile standard --suggest-benches",
        ))
        .stderr(predicate::str::contains("No benchmarks are configured").not());
}

#[test]
fn baseline_doctor_fails_when_config_is_missing() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "doctor", "--config", "missing.toml"])
        .assert()
        .failure()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "baseline config missing.toml not found",
        ))
        .stderr(predicate::str::contains(
            "perfgate init --ci github --profile standard --suggest-benches",
        ))
        .stderr(predicate::str::contains("No benchmarks are configured").not());
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
            "perfgate baseline promote --config perfgate.toml --all",
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
fn baseline_doctor_reports_mature_and_missing_local_baselines() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_two_bench_config(temp_dir.path());
    write_baseline_maturity_fixture(
        &temp_dir.path().join("baselines/test-benchmark.json"),
        "test-benchmark",
        7,
        0.03,
        0,
    );

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "doctor", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Baseline doctor"))
        .stdout(predicate::str::contains("bench: test-benchmark"))
        .stdout(predicate::str::contains("status: mature"))
        .stdout(predicate::str::contains("bench: second-benchmark"))
        .stdout(predicate::str::contains("status: missing"))
        .stdout(predicate::str::contains("Summary: 1 mature"))
        .stdout(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --all",
        ))
        .stdout(predicate::str::contains("Do not:"));
}

#[test]
fn baseline_doctor_classifies_high_noise_as_advisory() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());
    write_baseline_maturity_fixture(
        &temp_dir.path().join("baselines/test-benchmark.json"),
        "test-benchmark",
        7,
        0.20,
        0,
    );

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "doctor",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("status: high_noise"))
        .stdout(predicate::str::contains("keep advisory"))
        .stdout(predicate::str::contains("perfgate paired"));
}

#[test]
fn baseline_doctor_explains_imported_summary_evidence_limits() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());
    write_imported_summary_fixture(
        &temp_dir.path().join("baselines/test-benchmark.json"),
        "test-benchmark",
    );

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "doctor",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "source: imported (k6_summary_json)",
        ))
        .stdout(predicate::str::contains("sample model: summary_only"))
        .stdout(predicate::str::contains("host context: missing_or_partial"))
        .stdout(predicate::str::contains(
            "noise support: limited_summary_only",
        ))
        .stdout(predicate::str::contains(
            "summary-only evidence has limited noise support",
        ))
        .stdout(predicate::str::contains(
            "missing host context is not host-compatible proof",
        ))
        .stdout(predicate::str::contains("status: host_mismatched"));
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
            "perfgate baseline promote --config perfgate.toml --all",
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
fn baseline_promote_all_uses_check_all_artifact_convention() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_two_bench_config(temp_dir.path());
    write_run_fixture(
        &temp_dir
            .path()
            .join("artifacts/perfgate/test-benchmark/run.json"),
        "test-benchmark",
    );
    write_run_fixture(
        &temp_dir
            .path()
            .join("artifacts/perfgate/second-benchmark/run.json"),
        "second-benchmark",
    );

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "promote", "--config", "perfgate.toml", "--all"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "Promoted baseline for test-benchmark",
        ))
        .stderr(predicate::str::contains(
            "Promoted baseline for second-benchmark",
        ))
        .stderr(predicate::str::contains("Promoted 2 baselines"));

    assert!(
        temp_dir
            .path()
            .join("baselines/test-benchmark.json")
            .exists(),
        "baseline promote --all should write the first configured baseline"
    );
    assert!(
        temp_dir
            .path()
            .join("baselines/second-benchmark.json")
            .exists(),
        "baseline promote --all should write the second configured baseline"
    );
}

#[test]
fn baseline_promote_plan_is_non_mutating_and_reviewable() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());
    write_run_fixture(
        &temp_dir
            .path()
            .join("artifacts/perfgate/test-benchmark/run.json"),
        "test-benchmark",
    );
    let config_path = temp_dir.path().join("perfgate.toml");
    let config_before = fs::read_to_string(&config_path).expect("read config before plan");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "promote-plan",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Baseline promote plan"))
        .stdout(predicate::str::contains("bench: test-benchmark"))
        .stdout(predicate::str::contains("candidate source:"))
        .stdout(predicate::str::contains(
            "promotion safety: review required",
        ))
        .stdout(predicate::str::contains("Review checklist:"))
        .stdout(predicate::str::contains("Command after review:"))
        .stdout(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --bench test-benchmark",
        ))
        .stdout(predicate::str::contains("Do not:"));

    assert_eq!(
        fs::read_to_string(&config_path).expect("read config after plan"),
        config_before,
        "baseline promote-plan must not mutate config"
    );
    assert!(
        !temp_dir
            .path()
            .join("baselines/test-benchmark.json")
            .exists(),
        "baseline promote-plan must not write a baseline"
    );
}

#[test]
fn baseline_promote_plan_blocks_missing_candidate_without_writing() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "promote-plan",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("candidate source: missing"))
        .stdout(predicate::str::contains("promotion safety: blocked"))
        .stdout(predicate::str::contains(
            "perfgate check --config perfgate.toml --bench test-benchmark",
        ))
        .stdout(predicate::str::contains("do not promote"));

    assert!(
        !temp_dir
            .path()
            .join("baselines/test-benchmark.json")
            .exists(),
        "missing candidate promote-plan must not write a baseline"
    );
}

#[test]
fn baseline_promote_plan_marks_existing_baseline_force_review() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());
    write_run_fixture(
        &temp_dir
            .path()
            .join("artifacts/perfgate/test-benchmark/run.json"),
        "test-benchmark",
    );
    write_baseline_maturity_fixture(
        &temp_dir.path().join("baselines/test-benchmark.json"),
        "test-benchmark",
        8,
        0.02,
        1,
    );

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "promote-plan",
            "--config",
            "perfgate.toml",
            "--bench",
            "test-benchmark",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("existing baseline: mature"))
        .stdout(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --bench test-benchmark",
        ))
        .stdout(predicate::str::contains("--force"));
}

#[test]
fn baseline_promote_refuses_overwrite_without_force() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    write_config(temp_dir.path());
    write_run_fixture(
        &temp_dir
            .path()
            .join("artifacts/perfgate/test-benchmark/run.json"),
        "test-benchmark",
    );
    fs::create_dir_all(temp_dir.path().join("baselines")).expect("create baselines");
    fs::write(
        temp_dir.path().join("baselines/test-benchmark.json"),
        r#"{"schema":"perfgate.run.v1"}"#,
    )
    .expect("write existing baseline");

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args(["baseline", "promote", "--config", "perfgate.toml", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("baseline already exists"))
        .stderr(predicate::str::contains("--force"));

    perfgate_cmd()
        .current_dir(temp_dir.path())
        .args([
            "baseline",
            "promote",
            "--config",
            "perfgate.toml",
            "--all",
            "--force",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("Promoted 1 baseline"));
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
