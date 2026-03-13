//! Integration tests for the perfgate-client against a real server.
//!
//! These tests spin up a test server and verify the client can interact with it.

use perfgate_client::types::{ListBaselinesQuery, PromoteBaselineRequest, UploadBaselineRequest};
use perfgate_client::{BaselineClient, ClientConfig, ClientError, FallbackClient, FallbackStorage};
use perfgate_types::{
    BenchMeta, HostInfo, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
};
use tempfile::TempDir;
use wiremock::MockServer;
use wiremock::matchers::header;
use wiremock::{Mock, ResponseTemplate};

/// Helper to create a test receipt.
fn create_test_receipt(benchmark: &str) -> RunReceipt {
    RunReceipt {
        schema: "perfgate.run.v1".to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        run: RunMeta {
            id: uuid::Uuid::new_v4().to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            ended_at: "2024-01-15T10:00:01Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                hostname_hash: Some("test-host".to_string()),
                cpu_count: Some(8),
                memory_bytes: None,
            },
        },
        bench: BenchMeta {
            name: benchmark.to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            repeat: 3,
            warmup: 0,
            timeout_ms: None,
            cwd: None,
            work_units: None,
        },
        samples: vec![Sample {
            wall_ms: 100,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            max_rss_kb: Some(1024),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            binary_bytes: None,
            stdout: None,
            stderr: None,
        }],
        stats: Stats {
            wall_ms: U64Summary {
                median: 100,
                min: 100,
                max: 100,
            },
            max_rss_kb: Some(U64Summary {
                median: 1024,
                min: 1024,
                max: 1024,
            }),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            binary_bytes: None,
            throughput_per_s: None,
        },
    }
}

/// Helper to create a test upload request.
fn create_test_upload_request(benchmark: &str) -> UploadBaselineRequest {
    UploadBaselineRequest {
        benchmark: benchmark.to_string(),
        version: Some(format!("v1-{}", chrono::Utc::now().format("%Y%m%d%H%M%S"))),
        git_ref: Some("main".to_string()),
        git_sha: Some("abc123".to_string()),
        receipt: create_test_receipt(benchmark),
        metadata: std::collections::BTreeMap::new(),
        tags: vec!["test".to_string()],
        normalize: false,
    }
}

// Note: These tests use wiremock for HTTP mocking. For full integration tests
// that spin up a real perfgate-server, see the perfgate-server crate tests.

/// Test that the client can be created with a valid configuration.
#[test]
fn test_client_creation() {
    let config = ClientConfig::new("http://localhost:8080").with_api_key("test-key");

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

/// Test health check with a mock server.
#[tokio::test]
async fn test_health_check_mock() {
    let mock_server = MockServer::start().await;

    // Mock the health endpoint
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "status": "healthy",
            "version": "0.3.0",
            "storage": {
                "backend": "memory",
                "status": "healthy"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri());
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client.health_check().await;
    assert!(result.is_ok());

    let health = result.unwrap();
    assert_eq!(health.status, "healthy");
}

/// Test upload baseline with mock server.
#[tokio::test]
async fn test_upload_baseline_mock() {
    let mock_server = MockServer::start().await;

    // Mock the upload endpoint
    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/projects/test-project/baselines"))
        .and(header("Authorization", "Bearer test-key"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "perfgate_abc123",
            "benchmark": "test-bench",
            "version": "v1",
            "created_at": "2024-01-15T10:00:00Z",
            "etag": "\"sha256:hash123\""
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri()).with_api_key("test-key");
    let client = BaselineClient::new(config).expect("Failed to create client");

    let request = create_test_upload_request("test-bench");
    let result = client.upload_baseline("test-project", &request).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.benchmark, "test-bench");
}

/// Test get latest baseline with mock server.
#[tokio::test]
async fn test_get_latest_baseline_mock() {
    let mock_server = MockServer::start().await;

    let receipt = create_test_receipt("my-benchmark");

    // Mock the get latest endpoint
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/projects/my-project/baselines/my-benchmark/latest",
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

    let config = ClientConfig::new(mock_server.uri());
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client
        .get_latest_baseline("my-project", "my-benchmark")
        .await;

    assert!(result.is_ok());
    let baseline = result.unwrap();
    assert_eq!(baseline.benchmark, "my-benchmark");
    assert_eq!(baseline.project, "my-project");
}

/// Test list baselines with mock server.
#[tokio::test]
async fn test_list_baselines_mock() {
    let mock_server = MockServer::start().await;

    // Mock the list endpoint
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/projects/list-project/baselines"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "baselines": [
                {
                    "id": "perfgate_1",
                    "benchmark": "bench-1",
                    "version": "v1",
                    "created_at": "2024-01-15T10:00:00Z"
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

    let config = ClientConfig::new(mock_server.uri());
    let client = BaselineClient::new(config).expect("Failed to create client");

    let query = ListBaselinesQuery::new();
    let result = client.list_baselines("list-project", &query).await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.baselines.len(), 1);
    assert_eq!(response.pagination.total, 1);
}

/// Test delete baseline with mock server.
#[tokio::test]
async fn test_delete_baseline_mock() {
    let mock_server = MockServer::start().await;

    // Mock the delete endpoint
    Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(
            "/projects/del-project/baselines/del-bench/versions/v1",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "deleted": true,
            "id": "perfgate_del",
            "benchmark": "del-bench",
            "version": "v1"
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri()).with_api_key("admin-key");
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client
        .delete_baseline("del-project", "del-bench", "v1")
        .await;

    assert!(result.is_ok());
}

/// Test promote baseline with mock server.
#[tokio::test]
async fn test_promote_baseline_mock() {
    let mock_server = MockServer::start().await;

    // Mock the promote endpoint
    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path(
            "/projects/prom-project/baselines/prom-bench/promote",
        ))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "perfgate_promoted",
            "benchmark": "prom-bench",
            "version": "production",
            "promoted_from": "v1",
            "created_at": "2024-01-15T10:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri()).with_api_key("promoter-key");
    let client = BaselineClient::new(config).expect("Failed to create client");

    let request = PromoteBaselineRequest {
        from_version: "v1".to_string(),
        to_version: "production".to_string(),
        git_ref: Some("main".to_string()),
        git_sha: Some("def456".to_string()),
        normalize: false,
    };

    let result = client
        .promote_baseline("prom-project", "prom-bench", &request)
        .await;

    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.version, "production");
}

/// Test404 error handling.
#[tokio::test]
async fn test_not_found_error() {
    let mock_server = MockServer::start().await;

    // Mock a404 response
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/projects/my-project/baselines/nonexistent/latest",
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "code": "NOT_FOUND",
                "message": "Baseline not found"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri());
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

    // Mock a401 response
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/projects/my-project/baselines/bench/latest",
        ))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "code": "UNAUTHORIZED",
                "message": "Invalid API key"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri()).with_api_key("invalid-key");
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

    // Mock a403 response
    Mock::given(wiremock::matchers::method("DELETE"))
        .and(wiremock::matchers::path(
            "/projects/my-project/baselines/bench/versions/v1",
        ))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "error": {
                "code": "FORBIDDEN",
                "message": "Insufficient permissions"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri()).with_api_key("viewer-key");
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

    // Mock a409 response
    Mock::given(wiremock::matchers::method("POST"))
        .and(wiremock::matchers::path("/projects/my-project/baselines"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({
            "error": {
                "code": "ALREADY_EXISTS",
                "message": "Baseline already exists"
            }
        })))
        .mount(&mock_server)
        .await;

    let config = ClientConfig::new(mock_server.uri()).with_api_key("contributor-key");
    let client = BaselineClient::new(config).expect("Failed to create client");

    let request = create_test_upload_request("test-bench");
    let result = client.upload_baseline("my-project", &request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ClientError::AlreadyExistsError(_)));
}

/// Test connection error handling.
#[tokio::test]
async fn test_connection_error() {
    // Use a non-routable IP to trigger a connection error
    let config = ClientConfig::new("http://10.255.255.1:9999").with_api_key("test-key");
    let client = BaselineClient::new(config).expect("Failed to create client");

    let result = client.health_check().await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_connection_error());
}

/// Test fallback client saves to local storage on connection error.
#[tokio::test]
async fn test_fallback_client_saves_locally() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Use a non-routable address to trigger connection error
    let config = ClientConfig::new("http://10.255.255.1:9999")
        .with_api_key("test-key")
        .with_fallback(FallbackStorage::local(temp_dir.path()));

    let client = BaselineClient::new(config).expect("Failed to create client");
    let fallback_client =
        FallbackClient::new(client, Some(FallbackStorage::local(temp_dir.path())));

    let request = create_test_upload_request("fallback-bench");
    let result = fallback_client
        .upload_baseline("fallback-project", &request)
        .await;

    // Should succeed by falling back to local storage
    assert!(result.is_ok());
}

/// Test fallback client retrieves from local storage on connection error.
#[tokio::test]
async fn test_fallback_client_retrieves_locally() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // First, save a baseline to local storage
    let config = ClientConfig::new("http://10.255.255.1:9999")
        .with_api_key("test-key")
        .with_fallback(FallbackStorage::local(temp_dir.path()));

    let client = BaselineClient::new(config.clone()).expect("Failed to create client");
    let fallback_client =
        FallbackClient::new(client, Some(FallbackStorage::local(temp_dir.path())));

    // Upload to fallback storage
    let upload_request = create_test_upload_request("retrieve-bench");
    let upload_result = fallback_client
        .upload_baseline("retrieve-project", &upload_request)
        .await;
    assert!(upload_result.is_ok());

    // Create a new client to retrieve
    let client2 = BaselineClient::new(config).expect("Failed to create client");
    let fallback_client2 =
        FallbackClient::new(client2, Some(FallbackStorage::local(temp_dir.path())));

    // Retrieve from fallback storage
    let result = fallback_client2
        .get_latest_baseline("retrieve-project", "retrieve-bench")
        .await;

    assert!(result.is_ok());
    let baseline = result.unwrap();
    assert_eq!(baseline.benchmark, "retrieve-bench");
}

/// Test that fallback client passes through non-connection errors.
#[tokio::test]
async fn test_fallback_client_passes_through_errors() {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Mock a404 response (not a connection error)
    Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path(
            "/projects/my-project/baselines/bench/latest",
        ))
        .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
            "error": {
                "code": "NOT_FOUND",
                "message": "Baseline not found"
            }
        })))
        .mount(&mock_server)
        .await;

    let config =
        ClientConfig::new(mock_server.uri()).with_fallback(FallbackStorage::local(temp_dir.path()));
    let client = BaselineClient::new(config).expect("Failed to create client");
    let fallback_client =
        FallbackClient::new(client, Some(FallbackStorage::local(temp_dir.path())));

    let result = fallback_client
        .get_latest_baseline("my-project", "bench")
        .await;

    // Should return the404 error, not fall back
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, ClientError::NotFoundError(_)));
}

/// Test list baselines query builder.
#[test]
fn test_list_baselines_query_builder() {
    let query = ListBaselinesQuery::new()
        .with_benchmark("my-bench")
        .with_limit(10)
        .with_offset(20)
        .with_git_ref("main")
        .with_tags("tag1,tag2");

    assert_eq!(query.benchmark, Some("my-bench".to_string()));
    assert_eq!(query.limit, 10);
    assert_eq!(query.offset, 20);
    assert_eq!(query.git_ref, Some("main".to_string()));
    assert_eq!(query.tags, Some("tag1,tag2".to_string()));
}

/// Test upload request construction.
#[test]
fn test_upload_request_construction() {
    let receipt = create_test_receipt("builder-bench");

    let mut metadata = std::collections::BTreeMap::new();
    metadata.insert("env".to_string(), "test".to_string());

    let request = UploadBaselineRequest {
        benchmark: "builder-bench".to_string(),
        version: Some("v1".to_string()),
        git_ref: Some("feature-branch".to_string()),
        git_sha: Some("abc123".to_string()),
        receipt: receipt.clone(),
        metadata,
        tags: vec!["ci".to_string()],
        normalize: false,
    };

    assert_eq!(request.benchmark, "builder-bench");
    assert_eq!(request.version, Some("v1".to_string()));
    assert_eq!(request.git_ref, Some("feature-branch".to_string()));
    assert_eq!(request.git_sha, Some("abc123".to_string()));
    assert!(request.tags.contains(&"ci".to_string()));
    assert_eq!(request.metadata.get("env"), Some(&"test".to_string()));
}

/// Test promote request construction.
#[test]
fn test_promote_request_construction() {
    let request = PromoteBaselineRequest {
        from_version: "v1".to_string(),
        to_version: "production".to_string(),
        git_ref: Some("main".to_string()),
        git_sha: Some("def456".to_string()),
        normalize: false,
    };

    assert_eq!(request.from_version, "v1");
    assert_eq!(request.to_version, "production");
    assert_eq!(request.git_ref, Some("main".to_string()));
    assert_eq!(request.git_sha, Some("def456".to_string()));
}
