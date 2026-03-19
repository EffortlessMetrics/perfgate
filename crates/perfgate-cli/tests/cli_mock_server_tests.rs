//! Integration tests for CLI commands using a mock server.

use perfgate_types::{BASELINE_SCHEMA_V1, RUN_SCHEMA_V1};
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;
use common::perfgate_cmd;

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
