//! Integration tests with a real in-memory server.

use perfgate_client::types::{
    AuditAction, AuditResourceType, CreateKeyRequest, ListAuditEventsResponse, SubmitVerdictRequest,
};
use perfgate_client::{BaselineClient, ClientConfig};
use perfgate_server::auth::Role;
use perfgate_server::server::{ServerConfig, StorageBackend};
use perfgate_server::testing::spawn_test_server;
use perfgate_types::{VerdictCounts, VerdictStatus};

mod common;
use common::{ADMIN_KEY, CONTRIBUTOR_KEY, create_test_upload_request};

fn assert_score_close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("flakiness score should be present");
    assert!(
        (actual - expected).abs() < 0.000_001,
        "expected flakiness score {expected}, got {actual}"
    );
}

#[tokio::test]
async fn test_server_end_to_end_workflow() {
    let config = ServerConfig::new()
        .storage_backend(StorageBackend::Memory)
        .scoped_api_key(CONTRIBUTOR_KEY, Role::Contributor, "test-proj", None)
        .api_key(ADMIN_KEY, Role::Admin);

    let server = spawn_test_server(config).await;

    let client =
        BaselineClient::new(ClientConfig::new(&server.url).with_api_key(CONTRIBUTOR_KEY)).unwrap();

    // 1. Health check (root)
    let root_client = reqwest::Client::new();
    let health_res = root_client
        .get(format!("{}/health", server.root_url))
        .send()
        .await
        .unwrap();
    assert!(health_res.status().is_success());

    // 2. Health check (versioned alias)
    let health = client.health_check().await.expect("health check failed");
    assert_eq!(health.status, "healthy");

    // 3. Upload baseline
    let upload_req = create_test_upload_request("test-bench");
    let expected_version = upload_req
        .version
        .clone()
        .expect("test request should have a version");

    let upload_res = client
        .upload_baseline("test-proj", &upload_req)
        .await
        .expect("upload failed");
    assert_eq!(upload_res.benchmark, "test-bench");
    assert_eq!(upload_res.version, expected_version);

    // 4. Get latest
    let latest = client
        .get_latest_baseline("test-proj", "test-bench")
        .await
        .expect("get latest failed");
    assert_eq!(latest.version, expected_version);
    assert_eq!(latest.project, "test-proj");

    // 5. List baselines
    let query = perfgate_client::types::ListBaselinesQuery::new();
    let list = client
        .list_baselines("test-proj", &query)
        .await
        .expect("list failed");
    assert_eq!(list.baselines.len(), 1);
    assert_eq!(list.baselines[0].benchmark, "test-bench");

    // 6. Delete (requires Admin)
    let admin_client =
        BaselineClient::new(ClientConfig::new(&server.url).with_api_key(ADMIN_KEY)).unwrap();
    admin_client
        .delete_baseline("test-proj", "test-bench", &expected_version)
        .await
        .expect("delete failed");

    // 7. Verify deleted
    let list_after = client
        .list_baselines("test-proj", &query)
        .await
        .expect("list failed");
    assert_eq!(list_after.baselines.len(), 0);
}

#[tokio::test]
async fn test_key_management_writes_audit_events() {
    let config = ServerConfig::new()
        .storage_backend(StorageBackend::Memory)
        .api_key(ADMIN_KEY, Role::Admin);

    let server = spawn_test_server(config).await;
    let admin_client =
        BaselineClient::new(ClientConfig::new(&server.url).with_api_key(ADMIN_KEY)).unwrap();

    let created = admin_client
        .create_key(&CreateKeyRequest {
            description: "promotion key".to_string(),
            role: Role::Promoter,
            project: "test-proj".to_string(),
            pattern: Some("^bench-.*$".to_string()),
            expires_at: None,
        })
        .await
        .expect("create key failed");
    assert_eq!(created.role, Role::Promoter);
    assert_eq!(created.project, "test-proj");

    let listed = admin_client.list_keys().await.expect("list keys failed");
    assert!(listed.keys.iter().any(|key| key.id == created.id));

    admin_client
        .revoke_key(&created.id)
        .await
        .expect("revoke key failed");

    let audit: ListAuditEventsResponse = reqwest::Client::new()
        .get(format!("{}/audit", server.url))
        .bearer_auth(ADMIN_KEY)
        .send()
        .await
        .expect("audit request failed")
        .json()
        .await
        .expect("audit response should decode");

    let create_event = audit
        .events
        .iter()
        .find(|event| event.resource_id == created.id && event.action == AuditAction::Create)
        .expect("create key audit event should exist");
    assert_eq!(create_event.resource_type, AuditResourceType::Key);
    assert_eq!(create_event.project, "test-proj");
    assert_eq!(create_event.metadata["source"].as_str(), Some("api_key"));
    assert_eq!(create_event.metadata["role"].as_str(), Some("promoter"));

    let revoke_event = audit
        .events
        .iter()
        .find(|event| event.resource_id == created.id && event.action == AuditAction::Delete)
        .expect("revoke key audit event should exist");
    assert_eq!(revoke_event.resource_type, AuditResourceType::Key);
    assert_eq!(revoke_event.project, "test-proj");
    assert_eq!(revoke_event.metadata["source"].as_str(), Some("api_key"));
}

#[tokio::test]
async fn test_verdict_submission_scores_flakiness_from_recent_cv_history() {
    let config = ServerConfig::new()
        .storage_backend(StorageBackend::Memory)
        .scoped_api_key(CONTRIBUTOR_KEY, Role::Contributor, "test-proj", None);

    let server = spawn_test_server(config).await;
    let client =
        BaselineClient::new(ClientConfig::new(&server.url).with_api_key(CONTRIBUTOR_KEY)).unwrap();

    let first = client
        .submit_verdict(
            "test-proj",
            &SubmitVerdictRequest {
                benchmark: "flaky-bench".to_string(),
                run_id: "run-1".to_string(),
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
                git_ref: Some("main".to_string()),
                git_sha: Some("abc123".to_string()),
                wall_ms_cv: Some(0.12),
            },
        )
        .await
        .expect("first verdict should submit");
    assert_score_close(first.flakiness_score, 0.06);

    let second = client
        .submit_verdict(
            "test-proj",
            &SubmitVerdictRequest {
                benchmark: "flaky-bench".to_string(),
                run_id: "run-2".to_string(),
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 1,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec!["wall_ms.noisy".to_string()],
                git_ref: Some("main".to_string()),
                git_sha: Some("def456".to_string()),
                wall_ms_cv: Some(0.60),
            },
        )
        .await
        .expect("second verdict should submit");
    assert_score_close(second.flakiness_score, 0.53);
}
