//! Integration tests for advisory policy rollout surfaces.

use predicates::prelude::*;

mod common;
use common::perfgate_cmd;

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
