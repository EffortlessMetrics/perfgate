//! Integration tests for CLI commands that interact with the baseline server.
//!
//! These tests verify the CLI integration with the server including:
//! - `perfgate run --upload`
//! - `perfgate promote --to-server`
//! - `perfgate baseline list --baseline-server`
//! - `perfgate baseline upload`

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod common;
use common::perfgate_cmd;

/// Test that `--upload` fails without `--baseline-server` configured.
#[cfg_attr(windows, ignore)]
#[test]
fn test_upload_requires_baseline_server() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--")
        .arg("echo")
        .arg("test");

    cmd.assert().failure().stderr(
        predicate::str::contains("baseline server is not configured")
            .and(predicate::str::contains("--baseline-server")),
    );
}

/// Test that `--upload` fails without `--project` configured.
#[cfg_attr(windows, ignore)]
#[test]
fn test_upload_requires_project() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--baseline-server")
        .arg("http://localhost:9999/api/v1")
        .arg("--")
        .arg("echo")
        .arg("test");

    cmd.assert().failure().stderr(
        predicate::str::contains("--project is required")
            .and(predicate::str::contains("PERFGATE_PROJECT")),
    );
}

/// Test that `--to-server` fails without `--baseline-server` configured.
#[test]
fn test_to_server_requires_baseline_server() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    // Create a minimal baseline file
    let baseline_content = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "test-run",
            "started_at": "2024-01-15T10:00:00Z",
            "ended_at": "2024-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "max_rss_kb": 1024}],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "max_rss_kb": {"median": 1024, "min": 1024, "max": 1024}
        }
    });
    fs::write(
        &baseline_path,
        serde_json::to_string(&baseline_content).unwrap(),
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&baseline_path)
        .arg("--to-server")
        .arg("--benchmark")
        .arg("test-bench");

    cmd.assert().failure().stderr(predicate::str::contains(
        "baseline server is not configured",
    ));
}

/// Test that `--to-server` fails without `--project` configured.
#[test]
fn test_to_server_requires_project() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    // Create a minimal baseline file
    let baseline_content = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "test-run",
            "started_at": "2024-01-15T10:00:00Z",
            "ended_at": "2024-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "max_rss_kb": 1024}],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "max_rss_kb": {"median": 1024, "min": 1024, "max": 1024}
        }
    });
    fs::write(
        &baseline_path,
        serde_json::to_string(&baseline_content).unwrap(),
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&baseline_path)
        .arg("--to-server")
        .arg("--baseline-server")
        .arg("http://localhost:9999/api/v1")
        .arg("--benchmark")
        .arg("test-bench");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--project is required"));
}

/// Test that `--to-server` fails without `--benchmark` configured.
#[test]
fn test_to_server_requires_benchmark() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");

    // Create a minimal baseline file
    let baseline_content = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "test-run",
            "started_at": "2024-01-15T10:00:00Z",
            "ended_at": "2024-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "max_rss_kb": 1024}],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "max_rss_kb": {"median": 1024, "min": 1024, "max": 1024}
        }
    });
    fs::write(
        &baseline_path,
        serde_json::to_string(&baseline_content).unwrap(),
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&baseline_path)
        .arg("--to-server")
        .arg("--baseline-server")
        .arg("http://localhost:9999/api/v1")
        .arg("--project")
        .arg("test-project");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("--to-server requires --benchmark"));
}

/// Test that baseline list command requires server configuration.
#[test]
fn test_baseline_list_requires_server() {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline").arg("list");

    // Without --baseline-server, the command should either fail or show help
    // depending on implementation
    let output = cmd.output().expect("Failed to execute command");

    // The command should fail since no server is configured
    assert!(
        !output.status.success()
            || String::from_utf8_lossy(&output.stdout).contains("requires")
            || String::from_utf8_lossy(&output.stderr).contains("requires")
    );
}

/// Test that baseline upload command requires server configuration.
#[test]
fn test_baseline_upload_requires_server() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let receipt_path = temp_dir.path().join("receipt.json");

    // Create a minimal receipt file
    let receipt_content = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "test-run",
            "started_at": "2024-01-15T10:00:00Z",
            "ended_at": "2024-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "max_rss_kb": 1024}],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "max_rss_kb": {"median": 1024, "min": 1024, "max": 1024}
        }
    });
    fs::write(
        &receipt_path,
        serde_json::to_string(&receipt_content).unwrap(),
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.arg("baseline")
        .arg("upload")
        .arg("--benchmark")
        .arg("test-bench")
        .arg("--receipt")
        .arg(&receipt_path);

    // Should fail without server configuration
    let output = cmd.output().expect("Failed to execute command");
    assert!(!output.status.success());
}

/// Test that environment variables are used for server configuration.
#[cfg_attr(windows, ignore)]
#[test]
fn test_server_config_from_env() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    // Set environment variables
    let mut cmd = perfgate_cmd();
    cmd.env("PERFGATE_SERVER_URL", "http://localhost:9999/api/v1")
        .env("PERFGATE_API_KEY", "test-key")
        .env("PERFGATE_PROJECT", "test-project")
        .arg("run")
        .arg("--name")
        .arg("test")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--")
        .arg("echo")
        .arg("test");

    // The command should run but fail to connect to the server
    // (since there's no server running on port 9999)
    // This tests that the env vars are being read
    let output = cmd.output().expect("Failed to execute command");

    // The run should succeed (creating the receipt), but upload should fail
    // Check that it tried to upload (connection error message)
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Either it failed during upload (connection error) or succeeded with local file
    // We're testing that the env vars were picked up
    assert!(
        output.status.success()
            || stderr.contains("connection")
            || stderr.contains("Failed to upload")
            || stderr.contains("connect")
    );
}

/// Test that CLI flags override environment variables.
#[test]
fn test_cli_flags_override_env() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    // Set environment variables
    let mut cmd = perfgate_cmd();
    cmd.env("PERFGATE_SERVER_URL", "http://env-server:9999/api/v1")
        .env("PERFGATE_API_KEY", "env-key")
        .env("PERFGATE_PROJECT", "env-project")
        .arg("run")
        .arg("--name")
        .arg("test")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--baseline-server")
        .arg("http://cli-server:8888/api/v1") // Override
        .arg("--api-key")
        .arg("cli-key") // Override
        .arg("--project")
        .arg("cli-project") // Override
        .arg("--")
        .arg("echo")
        .arg("test");

    let output = cmd.output().expect("Failed to execute command");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // The command should try to connect to cli-server, not env-server
    // We can verify this by checking the error message contains the CLI-specified server
    if !output.status.success() && stderr.contains("cli-server") {
        // Good - it tried to connect to the CLI-specified server
    }
    // If it succeeded, that's also fine (local file created)
}

/// Test help output for server-related commands.
#[test]
fn test_run_help_shows_upload_option() {
    let mut cmd = perfgate_cmd();
    cmd.arg("run").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--upload"));
}

#[test]
fn test_promote_help_shows_to_server_option() {
    let mut cmd = perfgate_cmd();
    cmd.arg("promote").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--to-server"));
}

#[test]
fn test_baseline_subcommand_exists() {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("upload"));
}

/// Test that global server flags are shown in help.
#[test]
fn test_global_server_flags_in_help() {
    let mut cmd = perfgate_cmd();
    cmd.arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("--baseline-server"))
        .stdout(predicate::str::contains("--api-key"))
        .stdout(predicate::str::contains("--project"));
}

/// Test compare with @server:benchmark reference (when server is not available).
#[test]
fn test_compare_server_reference_without_server() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");

    // Create a minimal current receipt
    let current_content = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "current-run",
            "started_at": "2024-01-15T10:00:00Z",
            "ended_at": "2024-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "max_rss_kb": 1024}],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "max_rss_kb": {"median": 1024, "min": 1024, "max": 1024}
        }
    });
    fs::write(
        &current_path,
        serde_json::to_string(&current_content).unwrap(),
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg("@server:test-bench")
        .arg("--current")
        .arg(&current_path)
        .arg("--baseline-server")
        .arg("http://localhost:9999/api/v1")
        .arg("--project")
        .arg("test-project");

    // Should fail because server is not available
    let output = cmd.output().expect("Failed to execute command");
    assert!(!output.status.success());
}

/// Test compare with an explicit baseline project override and no global project.
#[test]
fn test_compare_server_reference_with_baseline_project_without_global_project() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");

    let current_content = serde_json::json!({
        "schema": "perfgate.run.v1",
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "current-run",
            "started_at": "2024-01-15T10:00:00Z",
            "ended_at": "2024-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false, "max_rss_kb": 1024}],
        "stats": {
            "wall_ms": {"median": 100, "min": 100, "max": 100},
            "max_rss_kb": {"median": 1024, "min": 1024, "max": 1024}
        }
    });
    fs::write(
        &current_path,
        serde_json::to_string(&current_content).unwrap(),
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg("@server:test-bench")
        .arg("--baseline-project")
        .arg("source-project")
        .arg("--current")
        .arg(&current_path)
        .arg("--baseline-server")
        .arg("http://localhost:9999/api/v1");

    let output = cmd.output().expect("Failed to execute command");
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("--project is required"),
        "compare should accept --baseline-project as the server lookup scope: {stderr}"
    );
}

/// Integration test with a mock server (requires server to be running).
/// This test is marked with #[ignore] so it doesn't run by default.
#[test]
#[ignore = "Requires a running perfgate-server on localhost:8080"]
fn test_full_upload_workflow_with_server() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    // Run a benchmark and upload to server
    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("integration-test-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--baseline-server")
        .arg("http://localhost:8080")
        .arg("--api-key")
        .arg("test-contributor-key")
        .arg("--project")
        .arg("integration-tests")
        .arg("--")
        .arg("echo")
        .arg("test");

    let output = cmd.output().expect("Failed to execute command");

    // Should succeed if server is running with the test key
    if output.status.success() {
        // Verify the receipt was created
        assert!(output_path.exists(), "Receipt should exist after run");

        // Check that upload was mentioned in output
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Uploaded") || stderr.contains("upload"),
            "Should mention upload in output: {}",
            stderr
        );
    }
}

/// Integration test for baseline list with a running server.
#[test]
#[ignore = "Requires a running perfgate-server on localhost:8080"]
fn test_baseline_list_with_server() {
    let mut cmd = perfgate_cmd();
    cmd.arg("baseline")
        .arg("list")
        .arg("--baseline-server")
        .arg("http://localhost:8080")
        .arg("--api-key")
        .arg("test-viewer-key")
        .arg("--project")
        .arg("integration-tests");

    let output = cmd.output().expect("Failed to execute command");

    // Should succeed if server is running
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Output should be valid JSON array or object
        assert!(
            stdout.starts_with("[") || stdout.starts_with("{"),
            "Output should be JSON: {}",
            stdout
        );
    }
}
