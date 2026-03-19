//! Integration tests for the perfgate-server.
//!
//! These tests verify the server lifecycle, health endpoints, and baseline operations.
//!
//! # Running Tests
//!
//! Most tests use wiremock for HTTP mocking and can run without a real server.
//! Tests marked with `#[ignore]` require a running perfgate-server.

mod common;

use common::{ADMIN_KEY, CONTRIBUTOR_KEY, PROMOTER_KEY, VIEWER_KEY, create_test_upload_request};
use perfgate_client::types::ListBaselinesQuery;
use perfgate_client::{BaselineClient, ClientConfig, ClientError};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// =============================================================================
// Unit tests for client configuration (no server required)
// =============================================================================

/// Test that the client can be created with a valid configuration.
#[test]
fn test_client_creation() {
    let config = ClientConfig::new("http://localhost:8080")
        .with_api_key("pg_test_key_00000000000000000000000000001");

    let result = BaselineClient::new(config);
    assert!(result.is_ok());
}

/// Test that client creation fails with an invalid URL.
#[test]
fn test_client_creation_invalid_url() {
    let config = ClientConfig::new("not a valid url");

    let result = BaselineClient::new(config);
    assert!(result.is_err());
}

// =============================================================================
// Mock-based tests for HTTP operations
// =============================================================================

/// Test health check with a mock server.
#[tokio::test]
async fn test_health_check_mock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "healthy",
            "version": "0.0.0",
            "storage": {
                "backend": "memory",
                "status": "healthy"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(format!("{}/api/v1", mock_server.uri()));
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client.health_check().await;
    assert!(result.is_ok());

    let health = result.unwrap();
    assert_eq!(health.status, "healthy");
}

/// Test uploading a baseline with mock server.
#[tokio::test]
async fn test_upload_baseline_mock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/projects/test-project/baselines"))
        .and(header(
            "Authorization",
            format!("Bearer {}", CONTRIBUTOR_KEY),
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "perfgate_abc123",
            "benchmark": "test-bench",
            "version": "v1",
            "created_at": "2024-01-15T10:00:00Z",
            "etag": "\"sha256:hash123\""
        })))
        .mount(&mock_server)
        .await;

    let config =
        ClientConfig::new(format!("{}/api/v1", mock_server.uri())).with_api_key(CONTRIBUTOR_KEY);
    let client = BaselineClient::new(config).expect("Failed to create client");

    let request = create_test_upload_request("test-bench");
    let result = client.upload_baseline("test-project", &request).await;

    if let Err(ref e) = result {
        println!("ERROR upload: {:?}", e);
    }
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.benchmark, "test-bench");
}

/// Test getting the latest baseline with mock server.
#[tokio::test]
async fn test_get_latest_baseline_mock() {
    let mock_server = MockServer::start().await;

    let receipt = common::create_test_receipt("my-benchmark");

    Mock::given(method("GET"))
        .and(path(
            "/api/v1/projects/my-project/baselines/my-benchmark/latest",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "schema": "perfgate.baseline.v1",
            "id": "perfgate_xyz789",
            "project": "my-project",
            "benchmark": "my-benchmark",
            "version": "v1",
            "git_ref": "main",
            "git_sha": "abc123",
            "receipt": receipt,
            "metadata": {},
            "tags": [],
            "created_at": "2024-01-15T10:00:00Z",
            "updated_at": "2024-01-15T10:00:00Z",
            "content_hash": "hash123",
            "source": "upload",
            "deleted": false
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(format!("{}/api/v1", mock_server.uri()));
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client
        .get_latest_baseline("my-project", "my-benchmark")
        .await;

    if let Err(ref e) = result {
        println!("ERROR get_latest: {:?}", e);
    }
    assert!(result.is_ok());
    let baseline = result.unwrap();
    assert_eq!(baseline.benchmark, "my-benchmark");
    assert_eq!(baseline.project, "my-project");
}

/// Test listing baselines with mock server.
#[tokio::test]
async fn test_list_baselines_mock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/projects/list-project/baselines"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "baselines": [
                {
                    "id": "perfgate_1",
                    "benchmark": "bench-1",
                    "version": "v1",
                    "created_at": "2024-01-15T10:00:00Z",
                    "tags": []
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

    let config = ClientConfig::new(format!("{}/api/v1", mock_server.uri()));
    let client = BaselineClient::new(config).expect("Failed to create client");

    let query = ListBaselinesQuery::new();
    let result = client.list_baselines("list-project", &query).await;

    if let Err(ref e) = result {
        println!("ERROR list: {:?}", e);
    }
    assert!(result.is_ok());
    let response = result.unwrap();
    assert!(!response.baselines.is_empty());
}

/// Test deleting a baseline with mock server.
#[tokio::test]
async fn test_delete_baseline_mock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/api/v1/projects/del-project/baselines/del-bench/versions/v1",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted": true,
            "id": "perfgate_del",
            "benchmark": "del-bench",
            "version": "v1",
            "deleted_at": "2024-01-15T10:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(format!("{}/api/v1", mock_server.uri())).with_api_key(ADMIN_KEY);
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client
        .delete_baseline("del-project", "del-bench", "v1")
        .await;

    if let Err(ref e) = result {
        println!("ERROR delete: {:?}", e);
    }
    assert!(result.is_ok());
}

/// Test promoting a baseline with mock server.
#[tokio::test]
async fn test_promote_baseline_mock() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path(
            "/api/v1/projects/prom-project/baselines/prom-bench/promote",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "perfgate_promoted",
            "benchmark": "prom-bench",
            "version": "production",
            "promoted_from": "v1",
            "promoted_at": "2024-01-15T10:00:00Z",
            "created_at": "2024-01-15T10:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let config =
        ClientConfig::new(format!("{}/api/v1", mock_server.uri())).with_api_key(PROMOTER_KEY);
    let client = BaselineClient::new(config).expect("Failed to create client");

    let request = perfgate_client::types::PromoteBaselineRequest {
        from_version: "v1".to_string(),
        to_version: "production".to_string(),
        git_ref: Some("main".to_string()),
        git_sha: Some("def456".to_string()),
        tags: vec![],
        normalize: false,
    };

    let result = client
        .promote_baseline("prom-project", "prom-bench", &request)
        .await;

    if let Err(ref e) = result {
        println!("ERROR promote: {:?}", e);
    }
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.version, "production");
}

/// Test404 error handling.
#[tokio::test]
async fn test_not_found_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path(
            "/api/v1/projects/my-project/baselines/nonexistent/latest",
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "code": "NOT_FOUND",
                "message": "Baseline not found"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(format!("{}/api/v1", mock_server.uri()));
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client
        .get_latest_baseline("my-project", "nonexistent")
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ClientError::NotFoundError(_)));
}

/// Test401 authentication error handling.
#[tokio::test]
async fn test_auth_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/v1/projects/my-project/baselines/bench/latest"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "code": "UNAUTHORIZED",
                "message": "Invalid API key"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(format!("{}/api/v1", mock_server.uri()))
        .with_api_key("pg_test_invalid_0000000000000000000000");
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client.get_latest_baseline("my-project", "bench").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ClientError::AuthError(_)));
}

/// Test403 forbidden error handling.
#[tokio::test]
async fn test_forbidden_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("DELETE"))
        .and(path(
            "/api/v1/projects/my-project/baselines/bench/versions/v1",
        ))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": {
                "code": "FORBIDDEN",
                "message": "Insufficient permissions"
            }
        })))
        .mount(&mock_server)
        .await;

    let config =
        ClientConfig::new(format!("{}/api/v1", mock_server.uri())).with_api_key(VIEWER_KEY);
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client.delete_baseline("my-project", "bench", "v1").await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    // 403 FORBIDDEN is mapped to AuthError
    assert!(matches!(err, ClientError::AuthError(_)));
}

/// Test409 conflict error handling.
#[tokio::test]
async fn test_conflict_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/api/v1/projects/my-project/baselines"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": {
                "code": "ALREADY_EXISTS",
                "message": "Baseline already exists"
            }
        })))
        .mount(&mock_server)
        .await;

    let config =
        ClientConfig::new(format!("{}/api/v1", mock_server.uri())).with_api_key(CONTRIBUTOR_KEY);
    let client = BaselineClient::new(config).expect("Failed to create client");

    let request = create_test_upload_request("test-bench");
    let result = client.upload_baseline("my-project", &request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ClientError::AlreadyExistsError(_)));
}

// =============================================================================
// Integration tests requiring a running server (marked with #[ignore])
// =============================================================================

/// Full integration test with a running server.
/// Run with: cargo test -p perfgate-server --test server_integration -- --ignored
#[tokio::test]
#[ignore = "Requires a running perfgate-server on localhost:8080"]
async fn test_full_upload_workflow_with_server() {
    let server_url = "http://localhost:8080";
    let client = reqwest::Client::new();

    // First, check server health
    let health_response = client
        .get(format!("{}/health", server_url))
        .send()
        .await
        .expect("Failed to connect to server");

    assert!(
        health_response.status().is_success(),
        "Server must be healthy"
    );

    // Upload a baseline
    let request = create_test_upload_request("integration-test-bench");
    let upload_response = client
        .post(format!(
            "{}/projects/integration-tests/baselines",
            server_url
        ))
        .header("Authorization", format!("Bearer {}", CONTRIBUTOR_KEY))
        .json(&request)
        .send()
        .await
        .expect("Failed to upload");

    assert!(
        upload_response.status().is_success(),
        "Upload should succeed"
    );
}

/// Test baseline list with a running server.
#[tokio::test]
#[ignore = "Requires a running perfgate-server on localhost:8080"]
async fn test_baseline_list_with_server() {
    let server_url = "http://localhost:8080";
    let client = reqwest::Client::new();

    let response = client
        .get(format!(
            "{}/projects/integration-tests/baselines",
            server_url
        ))
        .header("Authorization", format!("Bearer {}", VIEWER_KEY))
        .query(&[("limit", "10")])
        .send()
        .await
        .expect("Failed to list baselines");

    assert!(response.status().is_success(), "List should succeed");

    let body: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert!(
        body["baselines"].is_array(),
        "Response should contain baselines array"
    );
}
