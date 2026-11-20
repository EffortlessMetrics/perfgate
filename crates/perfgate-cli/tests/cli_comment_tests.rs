//! CLI integration tests for the GitHub comment command.

use predicates::prelude::*;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

mod common;
use common::{fixtures_dir, generate_compare_receipt, perfgate_cmd};

fn tradeoff_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("schema")
        .join("v0.16")
        .join("perfgate.tradeoff.v1.json")
}

fn generate_warn_compare(dir: &Path) -> PathBuf {
    let compare_path = dir.join("compare.json");
    generate_compare_receipt(
        &fixtures_dir().join("baseline.json"),
        &fixtures_dir().join("current_warn.json"),
        &compare_path,
    )
    .expect("failed to generate compare receipt");
    assert!(compare_path.exists(), "compare receipt should exist");
    compare_path
}

#[test]
fn comment_dry_run_renders_compare_receipt() {
    let dir = tempdir().expect("failed to create temp dir");
    let compare_path = generate_warn_compare(dir.path());

    let mut cmd = perfgate_cmd();
    cmd.arg("comment")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--dry-run");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("<!-- perfgate -->"))
        .stdout(predicate::str::contains("perfgate: **warn**"))
        .stdout(predicate::str::contains("Raw comparison data"));
}

#[test]
fn comment_uses_the_direction_from_compare() {
    let dir = tempdir().unwrap();
    let compare_path = dir.path().join("compare.json");
    for (current, expected) in [
        (
            "current_warn.json",
            "+19.00% | ▲ 19.0% improved | 20.0% (higher)",
        ),
        (
            "current_pass.json",
            "-5.00% | ▼ 5.0% regressed | 20.0% (higher)",
        ),
    ] {
        perfgate_cmd()
            .arg("compare")
            .arg("--baseline")
            .arg(fixtures_dir().join("baseline.json"))
            .arg("--current")
            .arg(fixtures_dir().join(current))
            .args(["--direction", "wall_ms=higher", "--out"])
            .arg(&compare_path)
            .assert()
            .success();

        perfgate_cmd()
            .args(["comment", "--dry-run", "--compare"])
            .arg(&compare_path)
            .assert()
            .success()
            .stdout(predicate::str::contains(expected));
    }
}

#[test]
fn comment_dry_run_renders_report_receipt() {
    let dir = tempdir().expect("failed to create temp dir");
    let compare_path = generate_warn_compare(dir.path());
    let report_path = dir.path().join("report.json");

    let mut report_cmd = perfgate_cmd();
    report_cmd
        .arg("report")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--out")
        .arg(&report_path);
    report_cmd.assert().success();

    let mut cmd = perfgate_cmd();
    cmd.arg("comment")
        .arg("--report")
        .arg(&report_path)
        .arg("--dry-run");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("<!-- perfgate -->"))
        .stdout(predicate::str::contains("perfgate: **warn**"));
}

#[test]
fn comment_dry_run_renders_tradeoff_receipt() {
    let mut cmd = perfgate_cmd();
    cmd.arg("comment")
        .arg("--tradeoff")
        .arg(tradeoff_fixture_path())
        .arg("--dry-run");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("<!-- perfgate -->"))
        .stdout(predicate::str::contains("perfgate tradeoff: pass"))
        .stdout(predicate::str::contains("large_file_parse"))
        .stdout(predicate::str::contains(
            "tokenizer-slower-if-parser-faster",
        ))
        .stdout(predicate::str::contains("Raw tradeoff data"));
}
