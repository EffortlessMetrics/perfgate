//! Integration tests for `perfgate comment`.

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;
use wiremock::matchers::{bearer_token, body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;
use common::perfgate_cmd;

#[test]
fn test_comment_dry_run_body_file_injects_marker() {
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let body_path = temp_dir.path().join("comment.md");
    fs::write(&body_path, "## perf summary\n\nbody").expect("write comment body");

    let mut cmd = perfgate_cmd();
    cmd.arg("comment")
        .arg("--body-file")
        .arg(&body_path)
        .arg("--dry-run");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "<!-- perfgate:comment kind=summary -->",
        ))
        .stdout(predicate::str::contains("## perf summary"));
}

#[tokio::test]
async fn test_comment_body_file_updates_existing_legacy_comment() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let body_path = temp_dir.path().join("comment.md");
    fs::write(&body_path, "## perf summary\n\nbody").expect("write comment body");

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/issues/7/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            {
                "id": 42,
                "body": "<!-- perfgate -->\nold content",
                "html_url": "https://github.com/owner/repo/pull/7#issuecomment-42",
                "user": { "login": "perfgate-bot" }
            }
        ])))
        .mount(&mock_server)
        .await;

    Mock::given(method("PATCH"))
        .and(path("/repos/owner/repo/issues/comments/42"))
        .and(bearer_token("test-token"))
        .and(body_string_contains(
            "<!-- perfgate:comment kind=summary -->",
        ))
        .and(body_string_contains("## perf summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 42,
            "body": "<!-- perfgate:comment kind=summary -->\n## perf summary\n\nbody",
            "html_url": "https://github.com/owner/repo/pull/7#issuecomment-42",
            "user": { "login": "perfgate-bot" }
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("comment")
        .arg("--body-file")
        .arg(&body_path)
        .arg("--repo")
        .arg("owner/repo")
        .arg("--pr")
        .arg("7")
        .arg("--github-token")
        .arg("test-token")
        .arg("--github-api-url")
        .arg(mock_server.uri());

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Updated perfgate comment"));
}

#[tokio::test]
async fn test_comment_body_file_uses_github_env_defaults_on_create() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("failed to create temp dir");
    let body_path = temp_dir.path().join("comment.md");
    fs::write(&body_path, "## perf summary\n\nbody").expect("write comment body");

    Mock::given(method("GET"))
        .and(path("/repos/owner/repo/issues/9/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&mock_server)
        .await;

    Mock::given(method("POST"))
        .and(path("/repos/owner/repo/issues/9/comments"))
        .and(bearer_token("test-token"))
        .and(body_string_contains(
            "<!-- perfgate:comment kind=summary -->",
        ))
        .and(body_string_contains("## perf summary"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": 84,
            "body": "<!-- perfgate:comment kind=summary -->\n## perf summary\n\nbody",
            "html_url": "https://github.com/owner/repo/pull/9#issuecomment-84",
            "user": { "login": "perfgate-bot" }
        })))
        .mount(&mock_server)
        .await;

    let mut cmd = perfgate_cmd();
    cmd.arg("comment")
        .arg("--body-file")
        .arg(&body_path)
        .arg("--github-api-url")
        .arg(mock_server.uri())
        .env("GITHUB_TOKEN", "test-token")
        .env("GITHUB_REPOSITORY", "owner/repo")
        .env("GITHUB_REF", "refs/pull/9/merge");

    cmd.assert()
        .success()
        .stderr(predicate::str::contains("Created perfgate comment"));
}
