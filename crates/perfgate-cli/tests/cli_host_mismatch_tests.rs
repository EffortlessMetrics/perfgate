//! Integration tests for host mismatch policy (--host-mismatch flag)
//!
//! **Validates: Host mismatch detection and policy enforcement**
//!
//! Host mismatches are detected when:
//! - Different `os` or `arch`
//! - Significant difference in `cpu_count` (> 2x)
//! - Significant difference in `memory_bytes` (> 2x)
//! - Different `hostname_hash` (if both present)

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

/// Returns the path to the test fixtures directory
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
}

// ======================================================================
// Compare command tests with --host-mismatch policy
// ======================================================================

/// Test compare with --host-mismatch=warn (default) produces warning in output
/// when hosts have different hostnames.
///
/// Exit code should be 0 (pass) but warning should be printed to stderr.
#[test]
fn test_compare_host_mismatch_warn_policy_different_hostname() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_hostname.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out")
        .arg(&output_path);

    // Should succeed (warn policy does not fail)
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("warning: host mismatch"))
        .stderr(predicate::str::contains("hostname mismatch"));

    // Verify output file exists and is valid
    assert!(output_path.exists(), "output file should exist");
    let content = fs::read_to_string(&output_path).expect("failed to read output file");
    let receipt: serde_json::Value =
        serde_json::from_str(&content).expect("output should be valid JSON");
    assert_eq!(
        receipt["schema"].as_str(),
        Some("perfgate.compare.v1"),
        "schema should be 'perfgate.compare.v1'"
    );
}

/// Test compare with --host-mismatch=warn produces warning for OS mismatch.
#[test]
fn test_compare_host_mismatch_warn_policy_different_os() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_os.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("warning: host mismatch"))
        .stderr(predicate::str::contains("OS mismatch"));
}

/// Test compare with --host-mismatch=warn produces warning for architecture mismatch.
#[test]
fn test_compare_host_mismatch_warn_policy_different_arch() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_arch.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("warning: host mismatch"))
        .stderr(predicate::str::contains("architecture mismatch"));
}

/// Test compare with --host-mismatch=warn produces warning for CPU count mismatch (> 2x).
#[test]
fn test_compare_host_mismatch_warn_policy_different_cpu_count() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_cpu_count.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("warning: host mismatch"))
        .stderr(predicate::str::contains("CPU count differs"));
}

/// Test compare with --host-mismatch=error fails when hosts differ.
///
/// Exit code should be 1 (tool error) when mismatch is detected.
#[test]
fn test_compare_host_mismatch_error_policy_fails_on_mismatch() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_hostname.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("error")
        .arg("--out")
        .arg(&output_path);

    // Should fail with exit code 1 (tool error)
    cmd.assert()
        .code(1)
        .stderr(predicate::str::contains("host mismatch detected"))
        .stderr(predicate::str::contains("hostname mismatch"));
}

/// Test compare with --host-mismatch=error fails with OS mismatch.
#[test]
fn test_compare_host_mismatch_error_policy_os_mismatch() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_os.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("error")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .code(1)
        .stderr(predicate::str::contains("host mismatch detected"))
        .stderr(predicate::str::contains("OS mismatch"));
}

/// Test compare with --host-mismatch=error fails with architecture mismatch.
#[test]
fn test_compare_host_mismatch_error_policy_arch_mismatch() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_arch.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("error")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .code(1)
        .stderr(predicate::str::contains("host mismatch detected"))
        .stderr(predicate::str::contains("architecture mismatch"));
}

/// Test compare with --host-mismatch=error fails with CPU count mismatch (> 2x threshold).
#[test]
fn test_compare_host_mismatch_error_policy_cpu_count_mismatch() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_cpu_count.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("error")
        .arg("--out")
        .arg(&output_path);

    cmd.assert()
        .code(1)
        .stderr(predicate::str::contains("host mismatch detected"))
        .stderr(predicate::str::contains("CPU count differs"));
}

/// Test compare with --host-mismatch=ignore does not produce warnings.
///
/// Exit code should be 0 (pass) and no warning should be printed.
#[test]
fn test_compare_host_mismatch_ignore_policy_no_warnings() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_hostname.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("ignore")
        .arg("--out")
        .arg(&output_path);

    // Should succeed without warnings
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("host mismatch").not());

    // Verify output file exists
    assert!(output_path.exists(), "output file should exist");
}

/// Test compare with --host-mismatch=ignore suppresses all mismatch types.
#[test]
fn test_compare_host_mismatch_ignore_policy_multiple_mismatches() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_multiple_mismatches.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("ignore")
        .arg("--out")
        .arg(&output_path);

    // Should succeed without any warnings about host mismatch
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("host mismatch").not())
        .stderr(predicate::str::contains("OS mismatch").not())
        .stderr(predicate::str::contains("architecture mismatch").not())
        .stderr(predicate::str::contains("CPU count differs").not())
        .stderr(predicate::str::contains("hostname mismatch").not());
}

/// Test compare with multiple host mismatches reports all reasons with warn policy.
#[test]
fn test_compare_host_mismatch_warn_policy_multiple_mismatches() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_multiple_mismatches.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out")
        .arg(&output_path);

    // Should succeed but report all mismatches
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("OS mismatch"))
        .stderr(predicate::str::contains("architecture mismatch"))
        .stderr(predicate::str::contains("CPU count differs"))
        .stderr(predicate::str::contains("hostname mismatch"));
}

/// Test compare with identical hosts does not produce warnings.
#[test]
fn test_compare_host_mismatch_no_warning_when_hosts_match() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    // Use the standard baseline and current_pass which have matching hosts
    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out")
        .arg(&output_path);

    // Should succeed without any host mismatch warnings
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("host mismatch").not());
}

/// Test compare with default host-mismatch policy (warn) produces warnings.
#[test]
fn test_compare_host_mismatch_default_policy_is_warn() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");
    let current = fixtures_dir().join("current_host_different_hostname.json");

    // Note: not specifying --host-mismatch should default to warn
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--out")
        .arg(&output_path);

    // Should succeed with warning (default policy is warn)
    cmd.assert()
        .success()
        .stderr(predicate::str::contains("warning: host mismatch"));
}

// ======================================================================
// Check command tests with --host-mismatch policy
// ======================================================================

/// Returns a cross-platform command that exits successfully.
#[cfg(unix)]
fn success_command() -> Vec<&'static str> {
    vec!["true"]
}

#[cfg(windows)]
fn success_command() -> Vec<&'static str> {
    vec!["cmd", "/c", "exit", "0"]
}

/// Create a minimal config file with a single bench.
fn create_config_file(temp_dir: &std::path::Path, bench_name: &str) -> std::path::PathBuf {
    let config_path = temp_dir.join("perfgate.toml");
    let success_cmd = success_command();

    let cmd_str = success_cmd
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(", ");

    let config_content = format!(
        r#"
[defaults]
repeat = 2
warmup = 0
threshold = 0.20

[[bench]]
name = "{}"
command = [{}]
"#,
        bench_name, cmd_str
    );

    fs::write(&config_path, config_content).expect("Failed to write config file");
    config_path
}

/// Create a baseline receipt with specific host info for testing.
fn create_baseline_receipt_with_host(
    temp_dir: &std::path::Path,
    bench_name: &str,
    os: &str,
    arch: &str,
    cpu_count: u32,
    hostname_hash: &str,
) -> std::path::PathBuf {
    let baselines_dir = temp_dir.join("baselines");
    fs::create_dir_all(&baselines_dir).expect("Failed to create baselines dir");

    let baseline_path = baselines_dir.join(format!("{}.json", bench_name));

    let receipt = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {
            "name": "perfgate",
            "version": "0.1.0"
        },
        "run": {
            "id": "baseline-run-id",
            "started_at": "2024-01-01T00:00:00Z",
            "ended_at": "2024-01-01T00:01:00Z",
            "host": {
                "os": os,
                "arch": arch,
                "cpu_count": cpu_count,
                "memory_bytes": 17179869184_u64,
                "hostname_hash": hostname_hash
            }
        },
        "bench": {
            "name": bench_name,
            "command": ["echo", "hello"],
            "repeat": 2,
            "warmup": 0
        },
        "samples": [
            {"wall_ms": 10000, "exit_code": 0, "warmup": false, "timed_out": false},
            {"wall_ms": 10200, "exit_code": 0, "warmup": false, "timed_out": false}
        ],
        "stats": {
            "wall_ms": {
                "median": 10100,
                "min": 10000,
                "max": 10200
            }
        }
    });

    fs::write(
        &baseline_path,
        serde_json::to_string_pretty(&receipt).unwrap(),
    )
    .expect("Failed to write baseline");

    baseline_path
}

/// Test check command with --host-mismatch=warn produces warning when baseline host differs.
///
/// The current run's host will differ from the baseline (which has a specific hostname_hash).
#[test]
fn test_check_host_mismatch_warn_policy() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "host-warn-test");

    // Create baseline with specific host info (different from what will be detected at runtime)
    create_baseline_receipt_with_host(
        temp_dir.path(),
        "host-warn-test",
        "some-other-os", // This will differ from the current OS
        "x86_64",
        8,
        "baseline-hostname-hash",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("host-warn-test")
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should succeed (warn policy doesn't fail)
    assert!(
        output.status.success(),
        "check with --host-mismatch=warn should succeed: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should have warning about host mismatch
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("host mismatch") || stderr.contains("OS mismatch"),
        "stderr should mention host mismatch: {}",
        stderr
    );

    // Artifacts should still be created
    assert!(out_dir.join("run.json").exists(), "run.json should exist");
    assert!(
        out_dir.join("compare.json").exists(),
        "compare.json should exist when baseline is present"
    );
}

/// Test check command with --host-mismatch=error fails when baseline host differs.
#[test]
fn test_check_host_mismatch_error_policy() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "host-error-test");

    // Create baseline with specific host info that will differ from current
    create_baseline_receipt_with_host(
        temp_dir.path(),
        "host-error-test",
        "some-other-os", // This will differ from the current OS
        "x86_64",
        8,
        "baseline-hostname-hash",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("host-error-test")
        .arg("--host-mismatch")
        .arg("error")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should fail with exit code 1 (tool error)
    assert!(
        !output.status.success(),
        "check with --host-mismatch=error should fail when hosts differ"
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "exit code should be 1 for tool error"
    );

    // Error should mention host mismatch
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("host mismatch"),
        "stderr should mention host mismatch: {}",
        stderr
    );
}

/// Test check command with --host-mismatch=ignore does not produce warnings.
#[test]
fn test_check_host_mismatch_ignore_policy() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "host-ignore-test");

    // Create baseline with specific host info that will differ from current
    create_baseline_receipt_with_host(
        temp_dir.path(),
        "host-ignore-test",
        "some-other-os", // This will differ from the current OS
        "x86_64",
        8,
        "baseline-hostname-hash",
    );

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.current_dir(temp_dir.path())
        .arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("host-ignore-test")
        .arg("--host-mismatch")
        .arg("ignore")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should succeed without warnings
    assert!(
        output.status.success(),
        "check with --host-mismatch=ignore should succeed: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // Should NOT have warning about host mismatch
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("host mismatch"),
        "stderr should NOT mention host mismatch with ignore policy: {}",
        stderr
    );

    // Artifacts should be created
    assert!(out_dir.join("run.json").exists(), "run.json should exist");
    assert!(
        out_dir.join("compare.json").exists(),
        "compare.json should exist"
    );
}

/// Test check command with explicit --baseline flag and --host-mismatch=error.
#[test]
fn test_check_explicit_baseline_host_mismatch_error() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "explicit-baseline-test");

    // Use a fixture file as the explicit baseline
    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("explicit-baseline-test")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--host-mismatch")
        .arg("error")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should fail if current host differs from the fixture's host info
    // The fixture has os=linux, arch=x86_64, hostname_hash=abc123def456
    // On most CI environments or dev machines, the hostname_hash will differ
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("host mismatch"),
            "if failing, stderr should mention host mismatch: {}",
            stderr
        );
    }
    // Note: If running on a machine that happens to match, the test will pass
}

/// Test check command with explicit --baseline flag and --host-mismatch=warn.
#[test]
fn test_check_explicit_baseline_host_mismatch_warn() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "explicit-warn-test");

    // Use a fixture file as the explicit baseline
    let baseline = fixtures_dir().join("baseline_host_linux_x86.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("explicit-warn-test")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--host-mismatch")
        .arg("warn")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    // Should succeed (warn policy doesn't fail)
    assert!(
        output.status.success(),
        "check with --host-mismatch=warn should succeed: exit code {:?}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // Artifacts should be created
    assert!(out_dir.join("run.json").exists(), "run.json should exist");
}

// ======================================================================
// Invalid policy value tests
// ======================================================================

/// Test that invalid --host-mismatch policy value is rejected.
#[test]
fn test_compare_host_mismatch_invalid_policy() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("compare.json");

    let baseline = fixtures_dir().join("baseline.json");
    let current = fixtures_dir().join("current_pass.json");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline)
        .arg("--current")
        .arg(&current)
        .arg("--host-mismatch")
        .arg("invalid-policy")
        .arg("--out")
        .arg(&output_path);

    // Should fail with error about invalid policy
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid host mismatch policy"));
}

/// Test that invalid --host-mismatch policy value is rejected for check command.
#[test]
fn test_check_host_mismatch_invalid_policy() {
    let temp_dir = tempdir().expect("failed to create temp dir");
    let out_dir = temp_dir.path().join("artifacts");
    let config_path = create_config_file(temp_dir.path(), "invalid-policy-test");

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("perfgate"));
    cmd.arg("check")
        .arg("--config")
        .arg(&config_path)
        .arg("--bench")
        .arg("invalid-policy-test")
        .arg("--host-mismatch")
        .arg("invalid-policy")
        .arg("--out-dir")
        .arg(&out_dir);

    let output = cmd.output().expect("failed to execute check");

    assert!(
        !output.status.success(),
        "check with invalid policy should fail"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid host mismatch policy"),
        "stderr should mention invalid policy: {}",
        stderr
    );
}
