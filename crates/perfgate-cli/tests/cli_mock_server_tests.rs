//! Integration tests for CLI commands using a mock server.

use perfgate_types::{BASELINE_SCHEMA_V1, RUN_SCHEMA_V1};
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;
use common::perfgate_cmd;

fn run_receipt(run_id: &str, benchmark: &str, wall_ms: u64) -> serde_json::Value {
    serde_json::json!({
        "schema": RUN_SCHEMA_V1,
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": run_id,
            "started_at": "2026-01-15T10:00:00Z",
            "ended_at": "2026-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": benchmark,
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": wall_ms, "exit_code": 0, "warmup": false, "timed_out": false}],
        "stats": {
            "wall_ms": {"median": wall_ms, "min": wall_ms, "max": wall_ms}
        }
    })
}

fn baseline_record(
    project: &str,
    benchmark: &str,
    run_id: &str,
    wall_ms: u64,
) -> serde_json::Value {
    serde_json::json!({
        "schema": BASELINE_SCHEMA_V1,
        "id": "bl_contract",
        "project": project,
        "benchmark": benchmark,
        "version": "v1.0.0",
        "receipt": run_receipt(run_id, benchmark, wall_ms),
        "metadata": {},
        "tags": [],
        "promoted_at": null,
        "source": "upload",
        "content_hash": "abc",
        "deleted": false,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z"
    })
}

fn verdict_record(project: &str, benchmark: &str, run_id: &str) -> serde_json::Value {
    serde_json::json!({
        "schema": "perfgate.verdict.v1",
        "id": "verdict_contract",
        "project": project,
        "benchmark": benchmark,
        "run_id": run_id,
        "status": "pass",
        "counts": {"pass": 1, "warn": 0, "fail": 0, "skip": 0},
        "reasons": [],
        "created_at": "2026-01-01T00:00:00Z"
    })
}

fn write_run_receipt(path: &std::path::Path, run_id: &str, benchmark: &str, wall_ms: u64) {
    fs::write(
        path,
        serde_json::to_string(&run_receipt(run_id, benchmark, wall_ms)).unwrap(),
    )
    .unwrap();
}

fn write_fallback_baseline(
    root: &std::path::Path,
    project: &str,
    benchmark: &str,
    run_id: &str,
    wall_ms: u64,
) {
    let project_dir = root.join("baselines").join(project);
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(
        project_dir.join(format!("{benchmark}-v1.json")),
        serde_json::to_string(&baseline_record(project, benchmark, run_id, wall_ms)).unwrap(),
    )
    .unwrap();
}

fn add_success_command(cmd: &mut assert_cmd::Command) {
    cmd.arg("--");
    if cfg!(windows) {
        cmd.args(["cmd", "/C", "echo", "hello"]);
    } else {
        cmd.args(["echo", "hello"]);
    }
}

fn mock_plaintext_key() -> String {
    ["pg_live_", "fixtureonlynotsecret0000000000000000"].concat()
}

fn key_entry(id: &str, project: &str, revoked: bool) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "key_prefix": "pg_live_fixt...***",
        "description": format!("{} key", project),
        "role": "promoter",
        "project": project,
        "pattern": null,
        "created_at": "2026-01-01T00:00:00Z",
        "expires_at": null,
        "revoked_at": if revoked {
            serde_json::Value::String("2026-01-02T00:00:00Z".to_string())
        } else {
            serde_json::Value::Null
        }
    })
}

#[tokio::test]
async fn test_run_upload_with_mock_server() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("output.json");

    // Mock the upload endpoint
    Mock::given(method("POST"))
        .and(path("/api/v1/projects/test-project/baselines"))
        .and(header("Authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "bl_new",
            "benchmark": "test-bench",
            "version": "v1.0.0",
            "etag": "some-etag",
            "created_at": "2026-01-01T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("test-bench")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--api-key")
        .arg("test-key")
        .arg("--project")
        .arg("test-project")
        .arg("--");

    if cfg!(windows) {
        cmd.arg("cmd").arg("/c").arg("echo").arg("hello");
    } else {
        cmd.arg("echo").arg("hello");
    }

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Uploaded baseline"));

    assert!(output_path.exists());
}

#[tokio::test]
async fn test_admin_keys_create_with_mock_server() {
    let mock_server = MockServer::start().await;
    let plaintext = mock_plaintext_key();

    Mock::given(method("POST"))
        .and(path("/api/v1/keys"))
        .and(header("Authorization", "Bearer admin-key"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "key-new",
            "key": plaintext,
            "description": "promotion key",
            "role": "promoter",
            "project": "my-project",
            "pattern": null,
            "created_at": "2026-01-01T00:00:00Z",
            "expires_at": null
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--api-key")
        .arg("admin-key")
        .arg("admin")
        .arg("keys")
        .arg("create")
        .arg("--project")
        .arg("my-project")
        .arg("--role")
        .arg("promoter")
        .arg("--description")
        .arg("promotion key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("key-new"))
        .stdout(predicate::str::contains("my-project"))
        .stdout(predicate::str::contains("pg_live_"))
        .stderr(predicate::str::contains("Created API key key-new"));
}

#[tokio::test]
async fn test_admin_keys_list_filters_project_and_revoked_keys() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "keys": [
                key_entry("key-active", "my-project", false),
                key_entry("key-other", "other-project", false),
                key_entry("key-revoked", "my-project", true)
            ]
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--api-key")
        .arg("admin-key")
        .arg("admin")
        .arg("keys")
        .arg("list")
        .arg("--project")
        .arg("my-project");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("key-active"))
        .stdout(predicate::str::contains("key-other").not())
        .stdout(predicate::str::contains("key-revoked").not());
}

#[tokio::test]
async fn test_admin_keys_revoke_with_mock_server() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path("/api/v1/keys/key-active"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "key-active",
            "revoked_at": "2026-01-02T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--api-key")
        .arg("admin-key")
        .arg("admin")
        .arg("keys")
        .arg("revoke")
        .arg("key-active");

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Revoked API key key-active"));
}

#[tokio::test]
async fn test_admin_keys_rotate_with_mock_server() {
    let mock_server = MockServer::start().await;
    let plaintext = mock_plaintext_key();

    Mock::given(method("GET"))
        .and(path("/api/v1/keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "keys": [key_entry("key-old", "my-project", false)]
        })))
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/v1/keys"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "key-new",
            "key": plaintext,
            "description": "my-project key",
            "role": "promoter",
            "project": "my-project",
            "pattern": null,
            "created_at": "2026-01-03T00:00:00Z",
            "expires_at": null
        })))
        .mount(&mock_server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/v1/keys/key-old"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "key-old",
            "revoked_at": "2026-01-03T00:01:00Z"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--api-key")
        .arg("admin-key")
        .arg("admin")
        .arg("keys")
        .arg("rotate")
        .arg("key-old");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("key-old"))
        .stdout(predicate::str::contains("key-new"))
        .stdout(predicate::str::contains("pg_live_"))
        .stderr(predicate::str::contains(
            "Rotated API key key-old -> key-new",
        ));
}

#[tokio::test]
async fn test_compare_explicit_local_baseline_wins_over_configured_server() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let baseline_path = temp_dir.path().join("baseline.json");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");

    write_run_receipt(&baseline_path, "local-base", "contract-bench", 90);
    write_run_receipt(&current_path, "current-run", "contract-bench", 80);

    Mock::given(method("POST"))
        .and(path("/api/v1/projects/test-project/verdicts"))
        .respond_with(ResponseTemplate::new(201).set_body_json(verdict_record(
            "test-project",
            "contract-bench",
            "current-run",
        )))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("compare")
        .arg("--baseline")
        .arg(&baseline_path)
        .arg("--current")
        .arg(&current_path)
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--project")
        .arg("test-project")
        .arg("--out")
        .arg(&compare_path);

    cmd.assert().success();

    let compare_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&compare_path).unwrap()).unwrap();
    assert_eq!(
        compare_json["deltas"]["wall_ms"]["baseline"].as_f64(),
        Some(90.0)
    );

    let requests = mock_server.received_requests().await.unwrap();
    assert!(
        requests.iter().all(|request| {
            request.method.as_str() != "GET" || !request.url.path().contains("/baselines/")
        }),
        "explicit local baseline path must not fetch a server baseline"
    );
}

#[tokio::test]
async fn test_compare_bare_baseline_uses_server_when_configured_and_no_local_file_exists() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");

    write_run_receipt(&current_path, "current-run", "contract-bench", 110);

    Mock::given(method("GET"))
        .and(path(
            "/api/v1/projects/test-project/baselines/contract-bench/latest",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(baseline_record(
            "test-project",
            "contract-bench",
            "server-base",
            100,
        )))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("compare")
        .arg("--baseline")
        .arg("contract-bench")
        .arg("--current")
        .arg(&current_path)
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--project")
        .arg("test-project")
        .arg("--out")
        .arg(&compare_path);

    cmd.assert().success();

    let compare_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&compare_path).unwrap()).unwrap();
    assert_eq!(
        compare_json["deltas"]["wall_ms"]["baseline"].as_f64(),
        Some(100.0)
    );

    let requests = mock_server.received_requests().await.unwrap();
    let baseline_fetches = requests
        .iter()
        .filter(|request| {
            request.method.as_str() == "GET" && request.url.path().contains("/baselines/")
        })
        .count();
    assert_eq!(
        baseline_fetches, 1,
        "implicit server fallback should fetch exactly one baseline"
    );
}

#[test]
fn test_compare_bare_baseline_without_server_uses_existing_local_error() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");

    write_run_receipt(&current_path, "current-run", "contract-bench", 110);

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("compare")
        .arg("--baseline")
        .arg("missing-contract-bench")
        .arg("--current")
        .arg(&current_path)
        .arg("--out")
        .arg(&compare_path);

    cmd.assert().failure().stderr(
        predicate::str::contains("read")
            .and(predicate::str::contains("missing-contract-bench"))
            .and(predicate::str::contains("baseline server is not configured").not()),
    );
}

#[test]
fn test_compare_explicit_server_baseline_without_config_hard_errors() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");

    write_run_receipt(&current_path, "current-run", "contract-bench", 110);

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("compare")
        .arg("--baseline")
        .arg("@server:contract-bench")
        .arg("--current")
        .arg(&current_path)
        .arg("--out")
        .arg(&compare_path);

    cmd.assert().failure().stderr(predicate::str::contains(
        "baseline server is not configured",
    ));
    assert!(
        !compare_path.exists(),
        "explicit server compare must not write a compare receipt after configuration failure"
    );
}

#[test]
fn test_compare_explicit_server_baseline_does_not_use_local_fallback() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");
    let compare_path = temp_dir.path().join("compare.json");
    let config_path = temp_dir.path().join("perfgate.toml");

    write_run_receipt(&current_path, "current-run", "explicit-contract", 110);
    write_fallback_baseline(
        temp_dir.path(),
        "test-project",
        "explicit-contract",
        "fallback-run",
        90,
    );
    fs::write(
        &config_path,
        "[baseline_server]\nfallback_to_local = true\n",
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("compare")
        .arg("--baseline")
        .arg("@server:explicit-contract")
        .arg("--current")
        .arg(&current_path)
        .arg("--baseline-server")
        .arg("http://127.0.0.1:9/api/v1")
        .arg("--project")
        .arg("test-project")
        .arg("--out")
        .arg(&compare_path);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Failed to fetch baseline"));
    assert!(
        !compare_path.exists(),
        "explicit @server baseline must not silently compare against local fallback storage"
    );
}

#[tokio::test]
async fn test_run_upload_failure_preserves_local_receipt_and_exits_nonzero() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("run.json");

    Mock::given(method("POST"))
        .and(path("/api/v1/projects/test-project/baselines"))
        .respond_with(ResponseTemplate::new(500).set_body_string("server unavailable"))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("run")
        .arg("--name")
        .arg("contract-upload")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--project")
        .arg("test-project");
    add_success_command(&mut cmd);

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to upload baseline to server",
    ));
    assert!(output_path.exists(), "run receipt should be preserved");
}

#[test]
fn test_run_upload_connection_failure_does_not_write_local_fallback() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let output_path = temp_dir.path().join("run.json");
    let config_path = temp_dir.path().join("perfgate.toml");
    fs::write(
        &config_path,
        "[baseline_server]\nfallback_to_local = true\n",
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("run")
        .arg("--name")
        .arg("explicit-upload")
        .arg("--repeat")
        .arg("1")
        .arg("--out")
        .arg(&output_path)
        .arg("--upload")
        .arg("--baseline-server")
        .arg("http://127.0.0.1:9/api/v1")
        .arg("--project")
        .arg("test-project");
    add_success_command(&mut cmd);

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to upload baseline to server",
    ));
    assert!(output_path.exists(), "run receipt should be preserved");
    assert!(
        !temp_dir
            .path()
            .join("baselines")
            .join("test-project")
            .exists(),
        "explicit upload must not write a local fallback baseline"
    );
}

#[tokio::test]
async fn test_promote_to_server_failure_hard_errors() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");

    write_run_receipt(&current_path, "current-run", "contract-promote", 100);

    Mock::given(method("POST"))
        .and(path("/api/v1/projects/test-project/baselines"))
        .respond_with(ResponseTemplate::new(503).set_body_string("server unavailable"))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("promote")
        .arg("--current")
        .arg(&current_path)
        .arg("--to-server")
        .arg("--benchmark")
        .arg("contract-promote")
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--project")
        .arg("test-project");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to promote baseline to server",
    ));
}

#[test]
fn test_promote_to_server_connection_failure_does_not_write_local_fallback() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");
    let config_path = temp_dir.path().join("perfgate.toml");

    write_run_receipt(&current_path, "current-run", "explicit-promote", 100);
    fs::write(
        &config_path,
        "[baseline_server]\nfallback_to_local = true\n",
    )
    .unwrap();

    let mut cmd = perfgate_cmd();
    cmd.current_dir(temp_dir.path())
        .arg("promote")
        .arg("--current")
        .arg(&current_path)
        .arg("--to-server")
        .arg("--benchmark")
        .arg("explicit-promote")
        .arg("--baseline-server")
        .arg("http://127.0.0.1:9/api/v1")
        .arg("--project")
        .arg("test-project");

    cmd.assert().failure().stderr(predicate::str::contains(
        "Failed to promote baseline to server",
    ));
    assert!(
        !temp_dir
            .path()
            .join("baselines")
            .join("test-project")
            .exists(),
        "explicit promote must not write a local fallback baseline"
    );
}

#[tokio::test]
async fn test_baseline_list_with_mock_server() {
    let mock_server = MockServer::start().await;

    // Mock the list endpoint
    Mock::given(method("GET"))
        .and(path("/api/v1/projects/test-project/baselines"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "baselines": [

                {
                    "id": "bl_1",
                    "project": "test-project",
                    "benchmark": "test-bench",
                    "version": "v1.0.0",
                    "created_at": "2026-01-01T00:00:00Z",
                    "tags": [],
                    "promoted_at": null
                }
            ],
            "pagination": {
                "total": 1,
                "limit": 50,
                "offset": 0,
                "has_more": false
            }
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("baseline")
        .arg("list")
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--project")
        .arg("test-project");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("v1.0.0"))
        .stdout(predicate::str::contains("test-bench"));
}

#[tokio::test]
async fn test_compare_with_server_baseline() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let current_path = temp_dir.path().join("current.json");

    // Create a current receipt
    let current_content = serde_json::json!({
        "schema": RUN_SCHEMA_V1,
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "current-run",
            "started_at": "2026-01-15T10:00:00Z",
            "ended_at": "2026-01-15T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 110, "exit_code": 0, "warmup": false, "timed_out": false}],
        "stats": {
            "wall_ms": {"median": 110, "min": 110, "max": 110}
        }
    });
    fs::write(
        &current_path,
        serde_json::to_string(&current_content).unwrap(),
    )
    .unwrap();

    // Mock the get latest endpoint
    Mock::given(method("GET"))
        .and(path(
            "/api/v1/projects/test-project/baselines/test-bench/latest",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "schema": BASELINE_SCHEMA_V1,
            "id": "bl_1",
            "project": "test-project",
            "benchmark": "test-bench",
            "version": "v1.0.0",
            "receipt": {
                "schema": RUN_SCHEMA_V1,
                "tool": {"name": "perfgate", "version": "0.3.0"},
                "run": {
                    "id": "base-run",
                    "started_at": "2026-01-01T10:00:00Z",
                    "ended_at": "2026-01-01T10:00:01Z",
                    "host": {"os": "linux", "arch": "x86_64"}
                },
                "bench": {
                    "name": "test-bench",
                    "command": ["echo", "test"],
                    "repeat": 1,
                    "warmup": 0
                },
                "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false}],
                "stats": {
                    "wall_ms": {"median": 100, "min": 100, "max": 100}
                }
            },
            "metadata": {},
            "tags": [],
            "promoted_at": null,
            "source": "upload",
            "content_hash": "abc",
            "deleted": false,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    let compare_path = temp_dir.path().join("compare.json");
    cmd.arg("compare")
        .arg("--baseline")
        .arg("@server:test-bench")
        .arg("--current")
        .arg(&current_path)
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--project")
        .arg("test-project")
        .arg("--out")
        .arg(&compare_path);

    cmd.assert().success();

    assert!(compare_path.exists());
    let compare_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&compare_path).unwrap()).unwrap();

    let wall_ms_delta = &compare_json["deltas"]["wall_ms"];
    assert_eq!(wall_ms_delta["baseline"].as_f64(), Some(100.0));
    assert_eq!(wall_ms_delta["current"].as_f64(), Some(110.0));
    assert_eq!(wall_ms_delta["pct"].as_f64(), Some(0.1));
}

#[tokio::test]
async fn test_compare_with_explicit_baseline_project() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().unwrap();
    let current_path = temp_dir.path().join("current.json");

    let current_content = serde_json::json!({
        "schema": RUN_SCHEMA_V1,
        "tool": {"name": "perfgate", "version": "0.3.0"},
        "run": {
            "id": "current-run",
            "started_at": "2026-01-01T10:00:00Z",
            "ended_at": "2026-01-01T10:00:01Z",
            "host": {"os": "linux", "arch": "x86_64"}
        },
        "bench": {
            "name": "test-bench",
            "command": ["echo", "test"],
            "repeat": 1,
            "warmup": 0
        },
        "samples": [{"wall_ms": 110, "exit_code": 0, "warmup": false, "timed_out": false}],
        "stats": {
            "wall_ms": {"median": 110, "min": 110, "max": 110}
        }
    });
    fs::write(
        &current_path,
        serde_json::to_string(&current_content).unwrap(),
    )
    .unwrap();

    Mock::given(method("GET"))
        .and(path(
            "/api/v1/projects/source-project/baselines/test-bench/latest",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "schema": BASELINE_SCHEMA_V1,
            "id": "bl_2",
            "project": "source-project",
            "benchmark": "test-bench",
            "version": "v1.0.0",
            "receipt": {
                "schema": RUN_SCHEMA_V1,
                "tool": {"name": "perfgate", "version": "0.3.0"},
                "run": {
                    "id": "source-run",
                    "started_at": "2026-01-01T10:00:00Z",
                    "ended_at": "2026-01-01T10:00:01Z",
                    "host": {"os": "linux", "arch": "x86_64"}
                },
                "bench": {
                    "name": "test-bench",
                    "command": ["echo", "test"],
                    "repeat": 1,
                    "warmup": 0
                },
                "samples": [{"wall_ms": 100, "exit_code": 0, "warmup": false, "timed_out": false}],
                "stats": {
                    "wall_ms": {"median": 100, "min": 100, "max": 100}
                }
            },
            "metadata": {},
            "tags": [],
            "promoted_at": null,
            "source": "upload",
            "content_hash": "def",
            "deleted": false,
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    let compare_path = temp_dir.path().join("compare.json");
    cmd.arg("compare")
        .arg("--baseline")
        .arg("@server:test-bench")
        .arg("--baseline-project")
        .arg("source-project")
        .arg("--current")
        .arg(&current_path)
        .arg("--baseline-server")
        .arg(format!("{}/api/v1", mock_server.uri()))
        .arg("--project")
        .arg("current-project")
        .arg("--out")
        .arg(&compare_path);

    cmd.assert().success();

    assert!(compare_path.exists());
    let compare_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&compare_path).unwrap()).unwrap();

    let wall_ms_delta = &compare_json["deltas"]["wall_ms"];
    assert_eq!(wall_ms_delta["baseline"].as_f64(), Some(100.0));
    assert_eq!(wall_ms_delta["current"].as_f64(), Some(110.0));
    assert_eq!(wall_ms_delta["pct"].as_f64(), Some(0.1));
}
