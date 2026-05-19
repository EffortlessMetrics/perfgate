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
    write_config_with_extra(dir, "");
}

fn write_config_with_extra(dir: &Path, extra: &str) {
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
{extra}
"#
        ),
    )
    .expect("write config");
}

fn write_run_receipt(path: &Path, bench: &str, sample_count: usize, cv: f64) {
    write_run_receipt_with_age(path, bench, sample_count, cv, 1);
}

fn write_run_receipt_with_age(
    path: &Path,
    bench: &str,
    sample_count: usize,
    cv: f64,
    age_days: i64,
) {
    let started_at = chrono::Utc::now() - chrono::Duration::days(age_days);
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
    write_compare_receipt_with_status(path, bench, cv, "pass", 0.01);
}

fn write_compare_receipt_with_status(path: &Path, bench: &str, cv: f64, status: &str, pct: f64) {
    let counts = match status {
        "fail" => serde_json::json!({ "pass": 0, "warn": 0, "fail": 1, "skip": 0 }),
        "warn" => serde_json::json!({ "pass": 0, "warn": 1, "fail": 0, "skip": 0 }),
        _ => serde_json::json!({ "pass": 1, "warn": 0, "fail": 0, "skip": 0 }),
    };
    let reasons = if status == "pass" {
        serde_json::json!([])
    } else {
        serde_json::json!(["wall_ms regression exceeds policy"])
    };
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
                "current": 100.0 * (1.0 + pct),
                "ratio": 1.0 + pct,
                "pct": pct,
                "regression": pct,
                "cv": cv,
                "status": status
            }
        },
        "verdict": {
            "status": status,
            "counts": counts,
            "reasons": reasons
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

#[test]
fn policy_emit_patch_prints_reviewable_fragment_without_writing_config() {
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
    let before = fs::read_to_string(dir.path().join("perfgate.toml")).expect("read config");

    perfgate_cmd()
        .current_dir(dir.path())
        .args([
            "policy",
            "emit-patch",
            "--config",
            "perfgate.toml",
            "--bench",
            "policy-bench",
            "--to",
            "gate_candidate",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("perfgate policy emit-patch"))
        .stdout(predicate::str::contains("proposed posture: gate_candidate"))
        .stdout(predicate::str::contains("Reviewable TOML fragment"))
        .stdout(predicate::str::contains("[bench.budgets.wall_ms]"))
        .stdout(predicate::str::contains("threshold = 0.20"))
        .stdout(predicate::str::contains("noise_policy = \"warn\""))
        .stdout(predicate::str::contains(
            "gate_candidate is review-ready evidence, not blocking policy",
        ))
        .stdout(predicate::str::contains(
            "Advisory only: no config, baseline, threshold, policy, or server setting was changed.",
        ));

    let after = fs::read_to_string(dir.path().join("perfgate.toml")).expect("read config");
    assert_eq!(before, after, "policy emit-patch must not write config");
}

#[test]
fn policy_emit_patch_marks_required_gate_as_review_required() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());

    perfgate_cmd()
        .current_dir(dir.path())
        .args([
            "policy",
            "emit-patch",
            "--config",
            "perfgate.toml",
            "--bench",
            "policy-bench",
            "--to",
            "required_gate",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("proposed posture: required_gate"))
        .stdout(predicate::str::contains(
            "required_gate needs explicit reviewer approval",
        ))
        .stdout(predicate::str::contains(
            "requested target exceeds current evidence recommendation",
        ))
        .stdout(predicate::str::contains(
            "baseline promotion after workload review",
        ))
        .stdout(predicate::str::contains(
            "resolve missing evidence before promoting beyond advisory",
        ));
}

#[test]
fn policy_review_packet_renders_mature_gate_candidate_without_writing_config() {
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
    fs::write(
        dir.path()
            .join("artifacts/perfgate/policy-bench/report.json"),
        "{}",
    )
    .expect("write report artifact");
    fs::write(
        dir.path()
            .join("artifacts/perfgate/policy-bench/comment.md"),
        "comment",
    )
    .expect("write comment artifact");
    let before = fs::read_to_string(dir.path().join("perfgate.toml")).expect("read config");

    perfgate_cmd()
        .current_dir(dir.path())
        .args([
            "policy",
            "review-packet",
            "--config",
            "perfgate.toml",
            "--bench",
            "policy-bench",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "# perfgate performance review packet",
        ))
        .stdout(predicate::str::contains("- Gate verdict: `pass`"))
        .stdout(predicate::str::contains(
            "- Recommended posture: `gate_candidate`",
        ))
        .stdout(predicate::str::contains(
            "- Baseline maturity: `mature`",
        ))
        .stdout(predicate::str::contains(
            "- Signal confidence: `safe_to_gate`",
        ))
        .stdout(predicate::str::contains(
            "- Reproduce locally: `perfgate check --config perfgate.toml --bench policy-bench --require-baseline`",
        ))
        .stdout(predicate::str::contains(
            "- Review policy patch: `perfgate policy emit-patch --config perfgate.toml --bench policy-bench --to gate_candidate`",
        ))
        .stdout(predicate::str::contains(
            "This packet does not change config, baselines, thresholds, policy, or server settings.",
        ));

    let after = fs::read_to_string(dir.path().join("perfgate.toml")).expect("read config");
    assert_eq!(before, after, "policy review-packet must not write config");
}

#[test]
fn policy_review_packet_can_write_markdown_artifact_for_setup_state() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    let out = dir
        .path()
        .join("artifacts/perfgate/policy-bench/policy-review.md");

    perfgate_cmd()
        .current_dir(dir.path())
        .args([
            "policy",
            "review-packet",
            "--config",
            "perfgate.toml",
            "--bench",
            "policy-bench",
            "--out",
            out.to_str().expect("utf8 temp path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Wrote policy review packet"));

    let packet = fs::read_to_string(out).expect("read review packet");
    assert!(packet.contains("- Gate verdict: `setup_incomplete_missing_baseline`"));
    assert!(packet.contains("- Current posture: `smoke`"));
    assert!(packet.contains("- Recommended posture: `advisory`"));
    assert!(packet.contains("baseline promotion after workload review"));
    assert!(packet.contains("do not loosen thresholds or promote baselines"));
}

fn policy_review_packet_stdout(dir: &Path) -> String {
    let output = perfgate_cmd()
        .current_dir(dir)
        .args([
            "policy",
            "review-packet",
            "--config",
            "perfgate.toml",
            "--bench",
            "policy-bench",
        ])
        .output()
        .expect("run policy review-packet");
    assert!(
        output.status.success(),
        "review packet should succeed: stderr {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout utf8")
}

fn write_mature_policy_artifacts(dir: &Path, status: &str) {
    write_run_receipt(
        &dir.join("baselines/policy-bench.json"),
        "policy-bench",
        7,
        0.03,
    );
    write_run_receipt(
        &dir.join("artifacts/perfgate/policy-bench/run.json"),
        "policy-bench",
        7,
        0.03,
    );
    let pct = if status == "pass" { 0.01 } else { 0.25 };
    write_compare_receipt_with_status(
        &dir.join("artifacts/perfgate/policy-bench/compare.json"),
        "policy-bench",
        0.03,
        status,
        pct,
    );
}

#[test]
fn policy_review_packet_agent_guardrail_missing_baseline() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());

    let packet = policy_review_packet_stdout(dir.path());

    assert!(packet.contains("## Agent Guardrails"));
    assert!(packet.contains("- Scenario: `missing_baseline`"));
    assert!(packet.contains("- Allowed: rerun the check and inspect run/report artifacts"));
    assert!(packet.contains("- Review required: baseline promotion after workload review"));
    assert!(packet.contains(
        "- Forbidden by default: do not promote a missing baseline blindly or loosen thresholds"
    ));
}

#[test]
fn policy_review_packet_agent_guardrail_noisy_signal() {
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

    let packet = policy_review_packet_stdout(dir.path());

    assert!(packet.contains("- Scenario: `noisy_signal`"));
    assert!(
        packet.contains("- Allowed: recommend paired mode, more samples, or calibration review")
    );
    assert!(packet.contains("- Review required: policy promotion or threshold changes"));
    assert!(packet.contains(
        "- Forbidden by default: do not treat noisy evidence as a confirmed regression or required gate"
    ));
}

#[test]
fn policy_review_packet_agent_guardrail_mature_promotion_candidate() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_mature_policy_artifacts(dir.path(), "pass");

    let packet = policy_review_packet_stdout(dir.path());

    assert!(packet.contains("- Scenario: `mature_promotion_candidate`"));
    assert!(packet.contains("- Allowed: emit a gate_candidate patch with reasons"));
    assert!(packet.contains("- Review required: required_gate approval"));
    assert!(
        packet.contains("- Forbidden by default: do not treat gate_candidate as already blocking")
    );
}

#[test]
fn policy_review_packet_agent_guardrail_regression() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_mature_policy_artifacts(dir.path(), "fail");

    let packet = policy_review_packet_stdout(dir.path());

    assert!(packet.contains("- Gate verdict: `fail`"));
    assert!(packet.contains("- Scenario: `regression`"));
    assert!(packet.contains("- Allowed: reproduce locally and inspect compare/report artifacts"));
    assert!(packet.contains(
        "- Review required: baseline refresh, threshold loosening, or tradeoff acceptance"
    ));
    assert!(packet.contains(
        "- Forbidden by default: do not update the baseline or loosen thresholds to make CI green"
    ));
}

#[test]
fn policy_review_packet_agent_guardrail_tradeoff_candidate() {
    let dir = tempdir().expect("tempdir");
    write_config_with_extra(
        dir.path(),
        r#"
[[scenario]]
name = "release"
bench = "policy-bench"
weight = 1.0

[[tradeoff]]
name = "memory_for_runtime"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
min_improvement_ratio = 1.05
"#,
    );
    write_mature_policy_artifacts(dir.path(), "fail");

    let packet = policy_review_packet_stdout(dir.path());

    assert!(packet.contains("- Decision suggestion: scenario and tradeoff evidence configured"));
    assert!(packet.contains("- Scenario: `tradeoff_candidate`"));
    assert!(packet.contains("- Allowed: run decision suggest or bundle decision evidence"));
    assert!(packet.contains("- Review required: accepting a tradeoff or recording team history"));
    assert!(packet.contains(
        "- Forbidden by default: do not accept bounded regressions without decision evidence and reviewer approval"
    ));
}

#[test]
fn policy_review_packet_agent_guardrail_stale_proof() {
    let dir = tempdir().expect("tempdir");
    write_config(dir.path());
    write_run_receipt_with_age(
        &dir.path().join("baselines/policy-bench.json"),
        "policy-bench",
        7,
        0.03,
        45,
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

    let packet = policy_review_packet_stdout(dir.path());

    assert!(packet.contains("- Baseline maturity: `stale`"));
    assert!(packet.contains("- Proof freshness: stale"));
    assert!(packet.contains("- Scenario: `stale_proof`"));
    assert!(packet.contains("- Allowed: refresh proof or rerun on the intended runner class"));
    assert!(packet.contains(
        "- Review required: claim promotion or required_gate changes from refreshed proof"
    ));
    assert!(packet.contains(
        "- Forbidden by default: do not cite stale proof as current support for blocking policy"
    ));
}
