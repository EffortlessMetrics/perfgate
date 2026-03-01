//! Snapshot-style tests for CLI help text.
//!
//! Verifies that each subcommand prints help with expected key strings,
//! catching accidental CLI interface changes.

use predicates::prelude::*;

mod common;
use common::perfgate_cmd;

#[test]
fn cli_help_main() {
    perfgate_cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("perfgate"))
        .stdout(predicate::str::contains(
            "Perf budgets and baseline diffs for CI",
        ))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("compare"))
        .stdout(predicate::str::contains("check"))
        .stdout(predicate::str::contains("paired"))
        .stdout(predicate::str::contains("md"))
        .stdout(predicate::str::contains("export"))
        .stdout(predicate::str::contains("promote"))
        .stdout(predicate::str::contains("report"))
        .stdout(predicate::str::contains("github-annotations"));
}

#[test]
fn cli_help_run() {
    perfgate_cmd()
        .args(["run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run a command repeatedly"))
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--repeat"))
        .stdout(predicate::str::contains("--out"));
}

#[test]
fn cli_help_compare() {
    perfgate_cmd()
        .args(["compare", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Compare a current receipt against a baseline",
        ))
        .stdout(predicate::str::contains("--baseline"))
        .stdout(predicate::str::contains("--current"))
        .stdout(predicate::str::contains("--threshold"))
        .stdout(predicate::str::contains("--out"));
}

#[test]
fn cli_help_check() {
    perfgate_cmd()
        .args(["check", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Config-driven one-command workflow",
        ))
        .stdout(predicate::str::contains("--config"))
        .stdout(predicate::str::contains("--bench"))
        .stdout(predicate::str::contains("--out-dir"));
}

#[test]
fn cli_help_paired() {
    perfgate_cmd()
        .args(["paired", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run paired benchmark"))
        .stdout(predicate::str::contains("--name"))
        .stdout(predicate::str::contains("--repeat"))
        .stdout(predicate::str::contains("--out"));
}

#[test]
fn cli_help_md() {
    perfgate_cmd()
        .args(["md", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Render a Markdown summary"))
        .stdout(predicate::str::contains("--compare"));
}

#[test]
fn cli_help_export() {
    perfgate_cmd()
        .args(["export", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Export a run or compare receipt"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--out"));
}

#[test]
fn cli_help_promote() {
    perfgate_cmd()
        .args(["promote", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Promote a run receipt"))
        .stdout(predicate::str::contains("--current"))
        .stdout(predicate::str::contains("--to"));
}

#[test]
fn cli_help_report() {
    perfgate_cmd()
        .args(["report", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Generate a cockpit-compatible report",
        ))
        .stdout(predicate::str::contains("--compare"))
        .stdout(predicate::str::contains("--out"));
}

#[test]
fn cli_help_github_annotations() {
    perfgate_cmd()
        .args(["github-annotations", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("GitHub Actions annotations"))
        .stdout(predicate::str::contains("--compare"));
}

// ── insta full-output snapshot tests ─────────────────────────────────

fn help_output(args: &[&str]) -> String {
    let output = perfgate_cmd()
        .args(args)
        .output()
        .expect("failed to run perfgate");
    assert!(output.status.success());
    String::from_utf8(output.stdout).expect("non-UTF-8 help output")
}

#[test]
fn snapshot_help_main() {
    insta::assert_snapshot!("help_main", help_output(&["--help"]));
}

#[test]
fn snapshot_help_run() {
    insta::assert_snapshot!("help_run", help_output(&["run", "--help"]));
}

#[test]
fn snapshot_help_compare() {
    insta::assert_snapshot!("help_compare", help_output(&["compare", "--help"]));
}

#[test]
fn snapshot_help_check() {
    insta::assert_snapshot!("help_check", help_output(&["check", "--help"]));
}

#[test]
fn snapshot_help_promote() {
    insta::assert_snapshot!("help_promote", help_output(&["promote", "--help"]));
}
