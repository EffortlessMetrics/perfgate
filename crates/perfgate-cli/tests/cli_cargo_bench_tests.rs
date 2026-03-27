//! Integration tests for the `cargo-bench` subcommand.
//!
//! These tests verify the CLI interface and output structure, using a mock
//! Criterion directory instead of running actual cargo bench.

use predicates::prelude::*;

mod common;
use common::perfgate_cmd;

#[test]
fn cargo_bench_help_shows_expected_options() {
    perfgate_cmd()
        .args(["cargo-bench", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--bench"))
        .stdout(predicate::str::contains("--compare"))
        .stdout(predicate::str::contains("--out"))
        .stdout(predicate::str::contains("--target-dir"))
        .stdout(predicate::str::contains("--pretty"));
}

#[test]
fn cargo_bench_fails_when_cargo_bench_fails() {
    // cargo bench will fail because there are no bench targets in the temp dir
    let tmp = tempfile::TempDir::new().unwrap();
    let out = tmp.path().join("out.json");

    // Use a non-existent directory as the working directory to make cargo bench fail
    perfgate_cmd()
        .args([
            "cargo-bench",
            "--target-dir",
            tmp.path().to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cargo bench"));
}
