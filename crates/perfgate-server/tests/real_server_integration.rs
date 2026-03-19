//! Integration tests with a real in-memory server.

use perfgate_client::{BaselineClient, ClientConfig};
use perfgate_server::auth::Role;
use perfgate_server::server::{ServerConfig, StorageBackend};
use perfgate_server::testing::spawn_test_server;

mod common;
use common::{ADMIN_KEY, CONTRIBUTOR_KEY, create_test_upload_request};

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
