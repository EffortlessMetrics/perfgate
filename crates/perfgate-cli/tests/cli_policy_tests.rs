//! Integration tests for advisory policy rollout surfaces.

use predicates::prelude::*;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

mod common;
use common::perfgate_cmd;

#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

fn write_config(dir: &Path) {
    let command = success_command()
        .iter()
        .map(|part| format!("\"{}\"", part))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        dir.join("perfgate.toml"),
        format!(
            r#"[defaults]
repeat = 7
warmup = 0
baseline_dir = "baselines"
out_dir = "artifacts/perfgate"

[[bench]]
name = "policy-bench"
command = [{command}]
"#
        ),
    )
    .expect("write config");
}

fn write_run_receipt(path: &Path, bench: &str, sample_count: usize, cv: f64) {
    let started_at = chrono::Utc::now() - chrono::Duration::days(1);
    let samples = (0..sample_count)
        .map(|_| {
            serde_json::json!({
                "wall_ms": 100,
                "exit_code": 0,
                "warmup": false,
                "timed_out": false
            })
        })
        .collect::<Vec<_>>();
    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": { "name": "perfgate", "version": "0.20.0" },
        "run": {
            "id": format!("{bench}-run"),
            "started_at": started_at.to_rfc3339(),
            "ended_at": (started_at + chrono::Duration::seconds(1)).to_rfc3339(),
            "host": {
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH
            }
        },
        "bench": {
            "name": bench,
            "command": success_command(),
            "repeat": sample_count as u32,
            "warmup": 0
        },
        "samples": samples,
        "stats": {
            "wall_ms": {
                "median": 100,
                "min": 98,
                "max": 102,
                "mean": 100.0,
                "stddev": 100.0 * cv
            }
        }
    });
    fs::create_dir_all(path.parent().expect("receipt parent")).expect("create receipt parent");
    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize receipt"),
    )
    .expect("write receipt");
}

fn write_compare_receipt(path: &Path, bench: &str, cv: f64) {
    let compare = serde_json::json!({
        "schema": "perfgate.compare.v1",
        "tool": { "name": "perfgate", "version": "0.20.0" },
        "bench": {
            "name": bench,
            "command": success_command(),
            "repeat": 7,
            "warmup": 0
        },
        "baseline_ref": { "path": format!("baselines/{bench}.json"), "run_id": "baseline-run" },
        "current_ref": { "path": format!("artifacts/perfgate/{bench}/run.json"), "run_id": "current-run" },
        "budgets": {
            "wall_ms": {
                "threshold": 0.20,
                "warn_threshold": 0.10,
                "direction": "lower"
            }
        },
        "deltas": {
            "wall_ms": {
                "baseline": 100.0,
                "current": 101.0,
                "ratio": 1.01,
                "pct": 0.01,
                "regression": 0.01,
                "cv": cv,
                "status": "pass"
            }
        },
        "verdict": {
            "status": "pass",
            "counts": { "pass": 1, "warn": 0, "fail": 0, "skip": 0 },
            "reasons": []
        }
    });
    fs::create_dir_all(path.parent().expect("compare parent")).expect("create compare parent");
    fs::write(
        path,
        serde_json::to_string_pretty(&compare).expect("serialize compare"),
    )
    .expect("write compare");
}

#[test]
fn policy_profiles_lists_reviewable_catalog() {
    perfgate_cmd()
        .args(["policy", "profiles"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Policy profiles are reviewable starting points",
        ))
        .stdout(predicate::str::contains(
            "They do not promote baselines, loosen thresholds, or make checks blocking.",
        ))
        .stdout(predicate::str::contains("Profile: rust-cli-standard"))
        .stdout(predicate::str::contains("Profile: rust-workspace-advisory"))
        .stdout(predicate::str::contains("Profile: node-command-advisory"))
        .stdout(predicate::str::contains("Profile: python-command-advisory"))
        .stdout(predicate::str::contains("Profile: http-local-smoke"))
        .stdout(predicate::str::contains(
            "Profile: generic-command-advisory",
        ))
        .stdout(predicate::str::contains("Profile: agent-heavy-repo"))
        .stdout(predicate::str::contains("Profile: server-ledger-optional"));
}

#[test]
fn policy_profiles_can_show_one_profile() {
    perfgate_cmd()
        .args(["policy", "profiles", "--profile", "rust-workspace-advisory"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Profile: rust-workspace-advisory"))
        .stdout(predicate::str::contains("compile and test setup noise"))
        .stdout(predicate::str::contains(
            "large workspace checks should block by default",
        ))
        .stdout(predicate::str::contains("Profile: rust-cli-standard").not());
}

#[test]
fn policy_profiles_rejects_unknown_profile() {
    perfgate_cmd()
        .args(["policy", "profiles", "--profile", "unknown"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn policy_doctor_keeps_missing_baseline_as_advisory_setup() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["policy", "doctor", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("perfgate policy doctor"))
        .stdout(predicate::str::contains("bench: policy-bench"))
        .stdout(predicate::str::contains("current posture: smoke"))
        .stdout(predicate::str::contains("recommended posture: advisory"))
        .stdout(predicate::str::contains("baseline maturity: missing"))
        .stdout(predicate::str::contains(
            "signal confidence: no_decision_yet",
        ))
        .stdout(predicate::str::contains(
            "baseline promotion after workload review",
        ))
        .stdout(predicate::str::contains(
            "perfgate baseline promote --config perfgate.toml --bench policy-bench",
        ))
        .stdout(predicate::str::contains(
            "Advisory only: no config, baseline, threshold, policy, or server setting was changed.",
        ));
}

#[test]
fn policy_doctor_reports_mature_signal_as_gate_candidate_not_required_gate() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_run_receipt(
        &dir.path().join("baselines/policy-bench.json"),
        "policy-bench",
        7,
        0.03,
    );
    write_run_receipt(
        &dir.path().join("artifacts/perfgate/policy-bench/run.json"),
        "policy-bench",
        7,
        0.03,
    );
    write_compare_receipt(
        &dir.path()
            .join("artifacts/perfgate/policy-bench/compare.json"),
        "policy-bench",
        0.03,
    );

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["policy", "doctor", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("current posture: advisory"))
        .stdout(predicate::str::contains(
            "recommended posture: gate_candidate",
        ))
        .stdout(predicate::str::contains("baseline maturity: mature"))
        .stdout(predicate::str::contains("signal confidence: safe_to_gate"))
        .stdout(predicate::str::contains("required-gate reviewer approval"))
        .stdout(predicate::str::contains("reviewable policy patch"))
        .stdout(predicate::str::contains(
            "do not make this a required gate without reviewer approval",
        ))
        .stdout(predicate::str::contains(
            "Summary: 1 gate_candidate, 0 advisory, 0 smoke, 0 quarantined",
        ));
}

#[test]
fn policy_doctor_keeps_noisy_signal_advisory_with_paired_guidance() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_run_receipt(
        &dir.path().join("baselines/policy-bench.json"),
        "policy-bench",
        7,
        0.20,
    );
    write_run_receipt(
        &dir.path().join("artifacts/perfgate/policy-bench/run.json"),
        "policy-bench",
        7,
        0.20,
    );

    perfgate_cmd()
        .current_dir(dir.path())
        .args(["policy", "doctor", "--config", "perfgate.toml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("recommended posture: advisory"))
        .stdout(predicate::str::contains("baseline maturity: high_noise"))
        .stdout(predicate::str::contains(
            "paired-mode or calibration review",
        ))
        .stdout(predicate::str::contains("perfgate paired"))
        .stdout(predicate::str::contains(
            "do not make advisory evidence blocking by default",
        ));
}
