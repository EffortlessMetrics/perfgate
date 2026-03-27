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

#[test]
fn cli_help_cargo_bench() {
    perfgate_cmd()
        .args(["cargo-bench", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cargo bench"))
        .stdout(predicate::str::contains("--out"))
        .stdout(predicate::str::contains("--bench"))
        .stdout(predicate::str::contains("--compare"));
}

// ── insta full-output snapshot tests ─────────────────────────────────

fn help_output(args: &[&str]) -> String {
    let output = perfgate_cmd()
        .args(args)
        .output()
        .expect("failed to run perfgate");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("non-UTF-8 help output");
    normalize_help(&text)
}

/// Normalize clap help text for platform-independent snapshots.
///
/// Clap formats help differently depending on terminal width, which varies
/// between Windows and Linux even with piped output. This function collapses
/// formatting differences into a canonical single-line representation.
fn normalize_help(raw: &str) -> String {
    let trailing_newline = raw.ends_with('\n');

    // 1. Normalize binary name
    let text = raw.replace("perfgate.exe", "perfgate");

    // 2. Collapse multi-line option descriptions and wrapped continuations.
    //    On narrow terminals, clap puts descriptions on the next line indented
    //    by 10 spaces; on wide terminals they stay on the same line.
    let text = text.replace("\n          ", " ");

    // 3. Join non-indented wrapped continuation lines (about-text wrapping).
    //    When terminal width is finite, long about-text paragraphs wrap and
    //    the continuation starts at column 0.
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let is_continuation = !line.is_empty()
            && !line.starts_with(' ')
            && !lines.is_empty()
            && !lines.last().unwrap().is_empty()
            && !line.starts_with("Usage:")
            && !line.starts_with("Options:")
            && !line.starts_with("Commands:")
            && !line.starts_with("Arguments:");
        if is_continuation {
            let last = lines.last_mut().unwrap();
            last.push(' ');
            last.push_str(line);
        } else {
            lines.push(line.to_string());
        }
    }

    // 4. Normalize each line: preserve leading indent, collapse runs of
    //    multiple spaces in the content to a single space.
    let normalized: Vec<String> = lines
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.is_empty() {
                return String::new();
            }
            let indent = &line[..line.len() - trimmed.len()];
            let mut content = String::new();
            let mut in_spaces = false;
            for ch in trimmed.chars() {
                if ch == ' ' {
                    if !in_spaces {
                        content.push(' ');
                        in_spaces = true;
                    }
                } else {
                    content.push(ch);
                    in_spaces = false;
                }
            }
            if content.ends_with(' ') {
                content.pop();
            }
            format!("{indent}{content}")
        })
        .collect();

    let mut result = normalized.join("\n");
    if trailing_newline {
        result.push('\n');
    }
    result
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
