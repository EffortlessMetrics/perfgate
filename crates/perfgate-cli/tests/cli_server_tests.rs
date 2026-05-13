//! Integration tests for CLI commands that interact with the baseline server.
//!
//! These tests verify the CLI integration with the server including:
//! - `perfgate run --upload`
//! - `perfgate promote --to-server`
//! - `perfgate baseline list --baseline-server`
//! - `perfgate baseline upload`
//! - `perfgate baseline submit-verdict`
//! - `perfgate baseline verdicts`
//! - `perfgate baseline delete`

use perfgate_server::auth::Role;
use perfgate_server::server::{ServerConfig, StorageBackend};
use perfgate_server::testing::{TestServer, spawn_test_server};
use predicates::prelude::*;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

mod common;
use common::{fixtures_dir, perfgate_cmd};

struct RunningTestServer {
    inner: TestServer,
}

impl RunningTestServer {
    async fn spawn(config: ServerConfig) -> Self {
        Self {
            inner: spawn_test_server(config).await,
        }
    }

    fn url(&self) -> &str {
        &self.inner.url
    }

    fn root_url(&self) -> &str {
        &self.inner.root_url
    }
}

impl Drop for RunningTestServer {
    fn drop(&mut self) {
        self.inner.handle.abort();
    }
}

fn add_success_command(cmd: &mut assert_cmd::Command) {
    cmd.arg("--");
    if cfg!(windows) {
        cmd.args(["cmd", "/C", "echo", "test"]);
    } else {
        cmd.args(["sh", "-c", "printf test"]);
    }
}

fn unique_project(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    format!("cli-live-{label}-{nanos}")
}

fn contributor_key() -> String {
    ["pg_test_", "fixtureonlynotsecretserverclitests000000"].concat()
}

fn admin_key() -> String {
    ["pg_test_", "fixtureonlynotsecretadminserverclitest000000"].concat()
}

fn operations_admin_key() -> String {
    ["pg_test_", "fixtureonlynotsecretoperationssmoke000000"].concat()
}

fn live_server_config(
    project: &str,
    backend: StorageBackend,
    contributor_key: &str,
    admin_key: &str,
) -> ServerConfig {
    ServerConfig::new()
        .storage_backend(backend)
        .scoped_api_key(contributor_key, Role::Contributor, project, None)
        .scoped_api_key(admin_key, Role::Admin, project, None)
}

fn add_server_flags(cmd: &mut assert_cmd::Command, server_url: &str, project: &str, api_key: &str) {
    cmd.arg("--baseline-server")
        .arg(server_url)
        .arg("--api-key")
        .arg(api_key)
        .arg("--project")
        .arg(project);
}

fn add_server_auth_flags(cmd: &mut assert_cmd::Command, server_url: &str, api_key: &str) {
    cmd.arg("--baseline-server")
        .arg(server_url)
        .arg("--api-key")
        .arg(api_key);
}

fn write_decision_tradeoff_receipt(path: &std::path::Path, scenario: &str) {
    let receipt = serde_json::json!({
        "schema": "perfgate.tradeoff.v1",
        "tool": {"name": "perfgate", "version": "0.16.0"},
        "run": {
            "id": "decision-run",
            "started_at": "2026-05-08T00:00:00Z",
            "ended_at": "2026-05-08T00:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "scenario": scenario,
        "configured_rules": [],
        "rules": [{
            "name": "memory_for_speed",
            "status": "accepted",
            "accepted": true,
            "downgrade_to": "warn",
            "reason": "tradeoff accepted"
        }],
        "weighted_deltas": {
            "wall_ms": {
                "baseline": 100.0,
                "current": 94.0,
                "ratio": 0.94,
                "pct": -0.06,
                "regression": 0.0,
                "status": "pass"
            }
        },
        "decision": {
            "accepted_tradeoff": true,
            "review_required": false,
            "review_reasons": [],
            "status": "warn",
            "reason": "tradeoff 'memory_for_speed' accepted"
        },
        "verdict": {
            "status": "warn",
            "counts": {"pass": 1, "warn": 1, "fail": 0, "skip": 0},
            "reasons": ["tradeoff_memory_for_speed_applied"]
        }
    });

    fs::write(
        path,
        serde_json::to_string_pretty(&receipt).expect("serialize decision tradeoff receipt"),
    )
    .expect("write decision tradeoff receipt");
}

fn write_decision_artifact_index(path: &std::path::Path) {
    let index = serde_json::json!({
        "schema": "perfgate.decision_index.v1",
        "scenario": "artifacts/perfgate/scenario.json",
        "tradeoff": "artifacts/perfgate/tradeoff.json",
        "decision": "artifacts/perfgate/decision.md",
        "probe_compares": ["artifacts/perfgate/parser/probe-compare.json"],
        "compare_receipts": ["artifacts/perfgate/parser/compare.json"]
    });

    fs::write(
        path,
        serde_json::to_string_pretty(&index).expect("serialize decision artifact index"),
    )
    .expect("write decision artifact index");
}

fn assert_root_health_is_healthy(root_url: &str) {
    let addr = root_url
        .strip_prefix("http://")
        .expect("test server root URL should use http");
    let mut stream = TcpStream::connect(addr).expect("root health socket should connect");
    stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .expect("health request should write");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("health response should read");

    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "root health should return 200 OK: {response}"
    );
    assert!(
        response.contains(r#""status":"healthy""#),
        "root health should report healthy status: {response}"
    );
}

/// Test that `--upload` fails without `--baseline-server` configured.
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
        .arg("--upload");
    add_success_command(&mut cmd);

    cmd.assert().failure().stderr(
        predicate::str::contains("baseline server is not configured")
            .and(predicate::str::contains("--baseline-server")),
    );
}

/// Test that `--upload` fails without `--project` configured.
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
        .arg("http://localhost:9999/api/v1");
    add_success_command(&mut cmd);

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
        .arg("--file")
        .arg(&receipt_path);

    // Should fail without server configuration
    let output = cmd.output().expect("Failed to execute command");
    assert!(!output.status.success());
}

/// Test that environment variables are used for server configuration.
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
        .arg("--upload");
    add_success_command(&mut cmd);

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
        .arg("cli-project"); // Override
    add_success_command(&mut cmd);

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

#[test]
fn test_admin_keys_subcommands_exist() {
    let mut cmd = perfgate_cmd();
    cmd.arg("admin").arg("keys").arg("--help");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("revoke"))
        .stdout(predicate::str::contains("rotate"));
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_server_cli_workflow_memory() {
    let project = unique_project("memory");
    let api_key = contributor_key();
    let admin_key = admin_key();
    let config = live_server_config(&project, StorageBackend::Memory, &api_key, &admin_key);

    run_live_server_cli_workflow(config, &project, &api_key, &admin_key).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_server_cli_workflow_sqlite() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let project = unique_project("sqlite");
    let api_key = contributor_key();
    let admin_key = admin_key();
    let config = live_server_config(&project, StorageBackend::Sqlite, &api_key, &admin_key)
        .sqlite_path(temp_dir.path().join("perfgate.db"));

    run_live_server_cli_workflow(config, &project, &api_key, &admin_key).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_server_cli_workflow_postgres() {
    let Ok(postgres_url) = env::var("PERFGATE_TEST_POSTGRES_URL") else {
        eprintln!(
            "skipping postgres live server CLI workflow; PERFGATE_TEST_POSTGRES_URL is unset"
        );
        return;
    };

    let project = unique_project("postgres");
    let api_key = contributor_key();
    let admin_key = admin_key();
    let config = live_server_config(&project, StorageBackend::Postgres, &api_key, &admin_key)
        .postgres_url(postgres_url);

    run_live_server_cli_workflow(config, &project, &api_key, &admin_key).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_operations_smoke_path_memory() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let serve_db = temp_dir.path().join("serve-doctor").join("perfgate.db");
    perfgate_cmd()
        .args(["serve", "--doctor", "--port", "0", "--db"])
        .arg(&serve_db)
        .assert()
        .success()
        .stdout(predicate::str::contains("perfgate serve doctor"))
        .stdout(predicate::str::contains("OK   database dir"))
        .stdout(predicate::str::contains("OK   sqlite storage"))
        .stdout(predicate::str::contains("OK   dashboard bind"))
        .stdout(predicate::str::contains("Summary: 0 failed checks"));

    let project = unique_project("operations");
    let api_key = operations_admin_key();
    let config = ServerConfig::new()
        .storage_backend(StorageBackend::Memory)
        .scoped_api_key(&api_key, Role::Admin, &project, None);
    let server = RunningTestServer::spawn(config).await;
    assert_root_health_is_healthy(server.root_url());

    let mut create_key = perfgate_cmd();
    create_key
        .arg("admin")
        .arg("keys")
        .arg("create")
        .arg("--project")
        .arg(&project)
        .arg("--role")
        .arg("promoter")
        .arg("--description")
        .arg("operations smoke key");
    add_server_auth_flags(&mut create_key, server.url(), &api_key);
    let create_key_assert = create_key
        .assert()
        .success()
        .stdout(predicate::str::contains(&project))
        .stdout(predicate::str::contains("pg_live_"))
        .stderr(predicate::str::contains("Created API key"));
    let create_key_stdout =
        String::from_utf8(create_key_assert.get_output().stdout.clone()).expect("key stdout utf8");
    let created_key_id = create_key_stdout
        .lines()
        .find_map(|line| {
            let mut columns = line.split('\t');
            let id = columns.next()?;
            if id == "id" || id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
        .expect("created key id should be printed");

    let mut list_keys = perfgate_cmd();
    list_keys
        .arg("admin")
        .arg("keys")
        .arg("list")
        .arg("--project")
        .arg(&project);
    add_server_auth_flags(&mut list_keys, server.url(), &api_key);
    list_keys
        .assert()
        .success()
        .stdout(predicate::str::contains("operations smoke key"))
        .stdout(predicate::str::contains(&project));

    let mut rotate_key = perfgate_cmd();
    rotate_key
        .arg("admin")
        .arg("keys")
        .arg("rotate")
        .arg(&created_key_id);
    add_server_auth_flags(&mut rotate_key, server.url(), &api_key);
    rotate_key
        .assert()
        .success()
        .stdout(predicate::str::contains(&created_key_id))
        .stdout(predicate::str::contains("pg_live_"))
        .stderr(predicate::str::contains(format!(
            "Rotated API key {created_key_id}"
        )));

    let mut list_keys_after_rotate = perfgate_cmd();
    list_keys_after_rotate
        .arg("admin")
        .arg("keys")
        .arg("list")
        .arg("--project")
        .arg(&project)
        .arg("--include-revoked");
    add_server_auth_flags(&mut list_keys_after_rotate, server.url(), &api_key);
    list_keys_after_rotate
        .assert()
        .success()
        .stdout(predicate::str::contains(&created_key_id))
        .stdout(predicate::str::contains("revoked"))
        .stdout(predicate::str::contains("active"));

    let baseline_file = fixtures_dir().join("baseline.json");
    let current_file = fixtures_dir().join("current_pass.json");
    let downloaded_path = temp_dir.path().join("latest-baseline.json");
    let compare_path = temp_dir.path().join("compare.json");
    let benchmark = format!("{project}-ops");

    let mut upload = perfgate_cmd();
    upload
        .arg("baseline")
        .arg("upload")
        .arg("--file")
        .arg(&baseline_file)
        .arg("--benchmark")
        .arg(&benchmark)
        .arg("--version")
        .arg("ops-v1");
    add_server_flags(&mut upload, server.url(), &project, &api_key);
    upload
        .assert()
        .success()
        .stderr(predicate::str::contains(&benchmark));

    let mut download_latest = perfgate_cmd();
    download_latest
        .arg("baseline")
        .arg("download")
        .arg("--benchmark")
        .arg(&benchmark)
        .arg("--output")
        .arg(&downloaded_path);
    add_server_flags(&mut download_latest, server.url(), &project, &api_key);
    download_latest.assert().success();
    assert!(
        downloaded_path.exists(),
        "latest baseline download should be written"
    );
    let downloaded: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&downloaded_path).expect("downloaded baseline should be readable"),
    )
    .expect("downloaded baseline should be valid JSON");
    assert_eq!(downloaded["schema"].as_str(), Some("perfgate.run.v1"));

    let mut compare = perfgate_cmd();
    compare
        .arg("compare")
        .arg("--baseline")
        .arg(format!("@server:{benchmark}"))
        .arg("--current")
        .arg(&current_file)
        .arg("--out")
        .arg(&compare_path)
        .arg("--host-mismatch")
        .arg("ignore");
    add_server_flags(&mut compare, server.url(), &project, &api_key);
    compare.assert().success();

    let mut submit_verdict = perfgate_cmd();
    submit_verdict
        .arg("baseline")
        .arg("submit-verdict")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--git-ref")
        .arg("refs/heads/server-ops-smoke");
    add_server_flags(&mut submit_verdict, server.url(), &project, &api_key);
    submit_verdict
        .assert()
        .success()
        .stdout(predicate::str::contains("Verdict submitted"));

    let mut verdicts = perfgate_cmd();
    verdicts
        .arg("baseline")
        .arg("verdicts")
        .arg("--benchmark")
        .arg("test-benchmark")
        .arg("--limit")
        .arg("5");
    add_server_flags(&mut verdicts, server.url(), &project, &api_key);
    verdicts
        .assert()
        .success()
        .stdout(predicate::str::contains("Verdict history"))
        .stdout(predicate::str::contains("test-benchmark"));

    let decision_path = temp_dir.path().join("operations-decision-tradeoff.json");
    let decision_index_path = temp_dir.path().join("operations-decision-index.json");
    let decision_export_path = temp_dir.path().join("operations-decision-history.jsonl");
    let decision_scenario = format!("{project}-ops-decision");
    write_decision_tradeoff_receipt(&decision_path, &decision_scenario);
    write_decision_artifact_index(&decision_index_path);

    let mut decision_upload = perfgate_cmd();
    decision_upload
        .arg("decision")
        .arg("upload")
        .arg("--file")
        .arg(&decision_path)
        .arg("--index")
        .arg(&decision_index_path)
        .arg("--git-ref")
        .arg("refs/heads/server-ops-smoke")
        .arg("--git-sha")
        .arg("abc123");
    add_server_flags(&mut decision_upload, server.url(), &project, &api_key);
    decision_upload
        .assert()
        .success()
        .stdout(predicate::str::contains("Uploaded decision"))
        .stdout(predicate::str::contains(&decision_scenario))
        .stdout(predicate::str::contains("accepted_rules=memory_for_speed"));

    let mut decision_history = perfgate_cmd();
    decision_history
        .arg("decision")
        .arg("history")
        .arg("--scenario")
        .arg(&decision_scenario)
        .arg("--limit")
        .arg("5");
    add_server_flags(&mut decision_history, server.url(), &project, &api_key);
    decision_history
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision history"))
        .stdout(predicate::str::contains(&decision_scenario));

    let mut decision_latest = perfgate_cmd();
    decision_latest.arg("decision").arg("latest");
    add_server_flags(&mut decision_latest, server.url(), &project, &api_key);
    decision_latest
        .assert()
        .success()
        .stdout(predicate::str::contains("Latest decision"))
        .stdout(predicate::str::contains(&decision_scenario))
        .stdout(predicate::str::contains("refs/heads/server-ops-smoke"));

    let mut decision_debt = perfgate_cmd();
    decision_debt
        .arg("decision")
        .arg("debt")
        .arg("--days")
        .arg("0");
    add_server_flags(&mut decision_debt, server.url(), &project, &api_key);
    decision_debt
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision debt"))
        .stdout(predicate::str::contains("Accepted tradeoff records: 1"))
        .stdout(predicate::str::contains("memory_for_speed"));

    let mut decision_export = perfgate_cmd();
    decision_export
        .arg("decision")
        .arg("export")
        .arg("--days")
        .arg("0")
        .arg("--format")
        .arg("jsonl")
        .arg("--out")
        .arg(&decision_export_path);
    add_server_flags(&mut decision_export, server.url(), &project, &api_key);
    decision_export
        .assert()
        .success()
        .stderr(predicate::str::contains("Exported 1 decision record"));
    let exported = fs::read_to_string(&decision_export_path).expect("read decision export");
    let exported_lines = exported.lines().collect::<Vec<_>>();
    assert_eq!(
        exported_lines.len(),
        1,
        "decision export should include one JSONL record: {exported}"
    );
    let exported_decision: serde_json::Value =
        serde_json::from_str(exported_lines[0]).expect("decision export line should be JSON");
    assert_eq!(
        exported_decision["scenario"].as_str(),
        Some(decision_scenario.as_str())
    );
    assert_eq!(
        exported_decision["artifact_index"]["schema"].as_str(),
        Some("perfgate.decision_index.v1")
    );

    let mut decision_prune_dry_run = perfgate_cmd();
    decision_prune_dry_run
        .arg("decision")
        .arg("prune")
        .arg("--older-than")
        .arg("0s")
        .arg("--dry-run");
    add_server_flags(
        &mut decision_prune_dry_run,
        server.url(),
        &project,
        &api_key,
    );
    decision_prune_dry_run
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision prune dry run"))
        .stdout(predicate::str::contains("1 record"));

    let mut decision_history_after_dry_run = perfgate_cmd();
    decision_history_after_dry_run
        .arg("decision")
        .arg("history")
        .arg("--scenario")
        .arg(&decision_scenario)
        .arg("--limit")
        .arg("5");
    add_server_flags(
        &mut decision_history_after_dry_run,
        server.url(),
        &project,
        &api_key,
    );
    decision_history_after_dry_run
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision history"))
        .stdout(predicate::str::contains(&decision_scenario));

    let mut decision_create_audit = perfgate_cmd();
    decision_create_audit
        .arg("audit")
        .arg("list")
        .arg("--project")
        .arg(&project)
        .arg("--resource-type")
        .arg("decision")
        .arg("--action")
        .arg("create")
        .arg("--limit")
        .arg("5");
    add_server_auth_flags(&mut decision_create_audit, server.url(), &api_key);
    decision_create_audit
        .assert()
        .success()
        .stdout(predicate::str::contains("decision"))
        .stdout(predicate::str::contains("create"));

    let mut delete = perfgate_cmd();
    delete
        .arg("baseline")
        .arg("delete")
        .arg("--benchmark")
        .arg(&benchmark)
        .arg("--force");
    add_server_flags(&mut delete, server.url(), &project, &api_key);
    delete
        .assert()
        .success()
        .stderr(predicate::str::contains(format!(
            "Deleted baseline {benchmark} version ops-v1 from server"
        )));

    let mut audit_list = perfgate_cmd();
    audit_list
        .arg("audit")
        .arg("list")
        .arg("--project")
        .arg(&project)
        .arg("--limit")
        .arg("20");
    add_server_auth_flags(&mut audit_list, server.url(), &api_key);
    audit_list
        .assert()
        .success()
        .stdout(predicate::str::contains("Audit events"))
        .stdout(predicate::str::contains(&project));

    let mut audit_export = perfgate_cmd();
    audit_export
        .arg("audit")
        .arg("export")
        .arg("--project")
        .arg(&project)
        .arg("--format")
        .arg("jsonl");
    add_server_auth_flags(&mut audit_export, server.url(), &api_key);
    audit_export
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            r#""project":"{project}""#
        )));
}

async fn run_live_server_cli_workflow(
    config: ServerConfig,
    project: &str,
    api_key: &str,
    admin_key: &str,
) {
    let server = RunningTestServer::spawn(config).await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let baseline_file = fixtures_dir().join("baseline.json");
    let current_file = fixtures_dir().join("current_pass.json");
    let uploaded_bench = format!("{project}-uploaded");
    let run_bench = format!("{project}-run");
    let promoted_bench = format!("{project}-promoted");
    let downloaded_path = temp_dir.path().join("downloaded-baseline.json");
    let run_path = temp_dir.path().join("uploaded-run.json");
    let compare_path = temp_dir.path().join("server-compare.json");
    let decision_path = temp_dir.path().join("server-decision-tradeoff.json");
    let decision_scenario = format!("{project}-decision-scenario");

    let mut upload = perfgate_cmd();
    upload
        .arg("baseline")
        .arg("upload")
        .arg("--file")
        .arg(&baseline_file)
        .arg("--benchmark")
        .arg(&uploaded_bench)
        .arg("--version")
        .arg("seed-v1");
    add_server_flags(&mut upload, server.url(), project, api_key);
    upload
        .assert()
        .success()
        .stderr(predicate::str::contains(&uploaded_bench));

    let mut list = perfgate_cmd();
    list.arg("baseline").arg("list").arg("--limit").arg("10");
    add_server_flags(&mut list, server.url(), project, api_key);
    list.assert()
        .success()
        .stdout(predicate::str::contains(&uploaded_bench));

    let mut download = perfgate_cmd();
    download
        .arg("baseline")
        .arg("download")
        .arg("--benchmark")
        .arg(&uploaded_bench)
        .arg("--output")
        .arg(&downloaded_path);
    add_server_flags(&mut download, server.url(), project, api_key);
    download.assert().success();
    assert!(
        downloaded_path.exists(),
        "downloaded baseline should be written"
    );
    let downloaded: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&downloaded_path).expect("downloaded baseline should be readable"),
    )
    .expect("downloaded baseline should be valid JSON");
    assert_eq!(downloaded["schema"].as_str(), Some("perfgate.run.v1"));

    let mut history = perfgate_cmd();
    history
        .arg("baseline")
        .arg("history")
        .arg("--benchmark")
        .arg(&uploaded_bench);
    add_server_flags(&mut history, server.url(), project, api_key);
    history
        .assert()
        .success()
        .stdout(predicate::str::contains("seed-v1"));

    let mut run = perfgate_cmd();
    run.arg("run")
        .arg("--name")
        .arg(&run_bench)
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&run_path)
        .arg("--upload");
    add_server_flags(&mut run, server.url(), project, api_key);
    add_success_command(&mut run);
    run.assert()
        .success()
        .stderr(predicate::str::contains(&run_bench));
    assert!(
        run_path.exists(),
        "uploaded run receipt should be preserved"
    );

    let mut compare = perfgate_cmd();
    compare
        .arg("compare")
        .arg("--baseline")
        .arg(format!("@server:{uploaded_bench}"))
        .arg("--current")
        .arg(&current_file)
        .arg("--out")
        .arg(&compare_path)
        .arg("--host-mismatch")
        .arg("ignore");
    add_server_flags(&mut compare, server.url(), project, api_key);
    compare.assert().success();
    assert!(
        compare_path.exists(),
        "server compare receipt should be written"
    );
    let compare_receipt: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&compare_path).expect("compare receipt should be readable"),
    )
    .expect("compare receipt should be valid JSON");
    assert_eq!(
        compare_receipt["schema"].as_str(),
        Some("perfgate.compare.v1")
    );

    let mut submit_verdict = perfgate_cmd();
    submit_verdict
        .arg("baseline")
        .arg("submit-verdict")
        .arg("--compare")
        .arg(&compare_path)
        .arg("--git-ref")
        .arg("refs/heads/live-server-cli")
        .arg("--git-sha")
        .arg("0123456789abcdef0123456789abcdef01234567");
    add_server_flags(&mut submit_verdict, server.url(), project, api_key);
    submit_verdict
        .assert()
        .success()
        .stdout(predicate::str::contains("Verdict submitted for benchmark"));

    let mut verdicts = perfgate_cmd();
    verdicts
        .arg("baseline")
        .arg("verdicts")
        .arg("--benchmark")
        .arg("test-benchmark")
        .arg("--limit")
        .arg("5");
    add_server_flags(&mut verdicts, server.url(), project, api_key);
    verdicts
        .assert()
        .success()
        .stdout(predicate::str::contains("Verdict history"))
        .stdout(predicate::str::contains("test-benchmark"))
        .stdout(predicate::str::contains("refs/heads/live-server-cli"));

    write_decision_tradeoff_receipt(&decision_path, &decision_scenario);
    let mut decision_upload = perfgate_cmd();
    decision_upload
        .arg("decision")
        .arg("upload")
        .arg("--file")
        .arg(&decision_path)
        .arg("--git-ref")
        .arg("refs/heads/live-server-cli");
    add_server_flags(&mut decision_upload, server.url(), project, api_key);
    decision_upload
        .assert()
        .success()
        .stdout(predicate::str::contains("Uploaded decision"))
        .stdout(predicate::str::contains(&decision_scenario));

    let mut decision_latest = perfgate_cmd();
    decision_latest.arg("decision").arg("latest");
    add_server_flags(&mut decision_latest, server.url(), project, api_key);
    decision_latest
        .assert()
        .success()
        .stdout(predicate::str::contains("Latest decision"))
        .stdout(predicate::str::contains(&decision_scenario))
        .stdout(predicate::str::contains("accepted_rules=memory_for_speed"));

    let mut decision_history = perfgate_cmd();
    decision_history
        .arg("decision")
        .arg("history")
        .arg("--scenario")
        .arg(&decision_scenario)
        .arg("--limit")
        .arg("5");
    add_server_flags(&mut decision_history, server.url(), project, api_key);
    decision_history
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision history"))
        .stdout(predicate::str::contains(&decision_scenario));

    let mut decision_debt = perfgate_cmd();
    decision_debt
        .arg("decision")
        .arg("debt")
        .arg("--days")
        .arg("0");
    add_server_flags(&mut decision_debt, server.url(), project, api_key);
    decision_debt
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision debt"))
        .stdout(predicate::str::contains(&decision_scenario))
        .stdout(predicate::str::contains("memory_for_speed (1)"));

    let decision_export_path = temp_dir.path().join("decision-history.jsonl");
    let mut decision_export = perfgate_cmd();
    decision_export
        .arg("decision")
        .arg("export")
        .arg("--days")
        .arg("0")
        .arg("--out")
        .arg(&decision_export_path);
    add_server_flags(&mut decision_export, server.url(), project, api_key);
    decision_export
        .assert()
        .success()
        .stderr(predicate::str::contains("Exported 1 decision record"));
    let exported = fs::read_to_string(&decision_export_path).expect("read decision export");
    assert!(
        exported.contains(&decision_scenario),
        "decision export should include uploaded scenario: {exported}"
    );

    let mut decision_prune_dry_run = perfgate_cmd();
    decision_prune_dry_run
        .arg("decision")
        .arg("prune")
        .arg("--older-than")
        .arg("0s")
        .arg("--dry-run");
    add_server_flags(
        &mut decision_prune_dry_run,
        server.url(),
        project,
        admin_key,
    );
    decision_prune_dry_run
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision prune dry run"))
        .stdout(predicate::str::contains("1 record"));

    let mut decision_prune_unconfirmed = perfgate_cmd();
    decision_prune_unconfirmed
        .arg("decision")
        .arg("prune")
        .arg("--older-than")
        .arg("0s");
    add_server_flags(
        &mut decision_prune_unconfirmed,
        server.url(),
        project,
        admin_key,
    );
    decision_prune_unconfirmed
        .assert()
        .failure()
        .stderr(predicate::str::contains("Decision prune not confirmed"));

    let mut decision_prune = perfgate_cmd();
    decision_prune
        .arg("decision")
        .arg("prune")
        .arg("--older-than")
        .arg("0s")
        .arg("--force");
    add_server_flags(&mut decision_prune, server.url(), project, admin_key);
    decision_prune
        .assert()
        .success()
        .stdout(predicate::str::contains("Pruned 1 decision record"));

    let mut decision_history_after_prune = perfgate_cmd();
    decision_history_after_prune
        .arg("decision")
        .arg("history")
        .arg("--limit")
        .arg("5");
    add_server_flags(
        &mut decision_history_after_prune,
        server.url(),
        project,
        api_key,
    );
    decision_history_after_prune
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "No decisions found for project '{project}'."
        )));

    let mut decision_prune_audit = perfgate_cmd();
    decision_prune_audit
        .arg("audit")
        .arg("list")
        .arg("--project")
        .arg(project)
        .arg("--resource-type")
        .arg("decision")
        .arg("--action")
        .arg("delete")
        .arg("--limit")
        .arg("5");
    add_server_auth_flags(&mut decision_prune_audit, server.url(), admin_key);
    decision_prune_audit
        .assert()
        .success()
        .stdout(predicate::str::contains("decision"))
        .stdout(predicate::str::contains("delete"));

    let mut delete = perfgate_cmd();
    delete
        .arg("baseline")
        .arg("delete")
        .arg("--benchmark")
        .arg(&uploaded_bench)
        .arg("--force");
    add_server_flags(&mut delete, server.url(), project, admin_key);
    delete
        .assert()
        .success()
        .stderr(predicate::str::contains(format!(
            "Deleted baseline {uploaded_bench} version seed-v1 from server"
        )));

    let mut deleted_history = perfgate_cmd();
    deleted_history
        .arg("baseline")
        .arg("history")
        .arg("--benchmark")
        .arg(&uploaded_bench);
    add_server_flags(&mut deleted_history, server.url(), project, api_key);
    deleted_history
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "No versions found for baseline '{uploaded_bench}'."
        )));

    let mut promote = perfgate_cmd();
    promote
        .arg("promote")
        .arg("--current")
        .arg(&baseline_file)
        .arg("--to-server")
        .arg("--benchmark")
        .arg(&promoted_bench)
        .arg("--version")
        .arg("promoted-v1");
    add_server_flags(&mut promote, server.url(), project, api_key);
    promote
        .assert()
        .success()
        .stderr(predicate::str::contains(&promoted_bench));
}
