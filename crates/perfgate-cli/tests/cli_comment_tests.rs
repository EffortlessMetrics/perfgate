//! CLI integration tests for the GitHub comment command.

use predicates::prelude::*;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

mod common;
use common::{fixtures_dir, generate_compare_receipt, perfgate_cmd};

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
