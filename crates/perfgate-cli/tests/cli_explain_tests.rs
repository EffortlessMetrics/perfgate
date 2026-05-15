//! Integration tests for `perfgate explain`.

use predicates::prelude::*;
use std::fs;

mod common;
use common::{fixtures_dir, generate_compare_receipt, perfgate_cmd};

#[test]
fn explain_compare_legacy_path_still_works() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let compare = temp_dir.path().join("compare.json");
    generate_compare_receipt(
        &fixtures_dir().join("baseline.json"),
        &fixtures_dir().join("current_fail.json"),
        &compare,
    )
    .expect("generate compare receipt");

    perfgate_cmd()
        .args(["explain", "--compare"])
        .arg(&compare)
        .assert()
        .success()
        .stdout(predicate::str::contains("Performance Analysis"))
        .stdout(predicate::str::contains("Performance Regressions Detected"));
}

#[test]
fn explain_artifacts_identifies_known_receipts_and_next_steps() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let out_dir = temp_dir.path().join("artifacts/perfgate");
    let bench_dir = out_dir.join("parser");
    fs::create_dir_all(&bench_dir).expect("create artifact dir");
    fs::write(bench_dir.join("run.json"), "{}").expect("write run");
    fs::write(bench_dir.join("compare.json"), "{}").expect("write compare");
    fs::write(bench_dir.join("report.json"), "{}").expect("write report");
    fs::write(bench_dir.join("comment.md"), "# comment").expect("write comment");
    fs::write(bench_dir.join("repair_context.json"), "{}").expect("write repair context");
    fs::write(out_dir.join("decision.index.json"), "{}").expect("write decision index");

    perfgate_cmd()
        .args(["explain", "artifacts", "--out-dir"])
        .arg(&out_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: artifacts_found"))
        .stdout(predicate::str::contains("parser"))
        .stdout(predicate::str::contains("run.json"))
        .stdout(predicate::str::contains("raw measurement receipt"))
        .stdout(predicate::str::contains("compare.json"))
        .stdout(predicate::str::contains(
            "baseline/current comparison receipt",
        ))
        .stdout(predicate::str::contains("repair_context.json"))
        .stdout(predicate::str::contains(
            "local reproduction and repair hints",
        ))
        .stdout(predicate::str::contains(
            "perfgate decision bundle --index artifacts/perfgate/decision.index.json",
        ))
        .stdout(predicate::str::contains(
            "perfgate check --config perfgate.toml --all --require-baseline",
        ));
}

#[test]
fn explain_artifacts_handles_missing_directory_as_setup_state() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let out_dir = temp_dir.path().join("missing-artifacts");

    perfgate_cmd()
        .args(["explain", "artifacts", "--out-dir"])
        .arg(&out_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: no_artifacts"))
        .stdout(predicate::str::contains(
            "run a check or decision command first",
        ))
        .stdout(predicate::str::contains(
            "perfgate check --config perfgate.toml --all",
        ));
}
