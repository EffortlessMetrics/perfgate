//! Integration tests for the baseline client and server.

use perfgate_client::client::BaselineClient;
use perfgate_client::config::ClientConfig;
use perfgate_client::types::*;
use perfgate_types::{BenchMeta, HostInfo, RunMeta, RunReceipt, Stats, ToolInfo, U64Summary};
use std::collections::BTreeMap;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_config(url: &str) -> ClientConfig {
    ClientConfig::new(url)
}

fn create_test_receipt() -> RunReceipt {
    RunReceipt {
        schema: "perfgate.run.v1".to_string(),
        tool: ToolInfo {
            name: "test".into(),
            version: "0".into(),
        },
        run: RunMeta {
            id: "r1".into(),
            started_at: "2024-01-01T00:00:00Z".into(),
            ended_at: "2024-01-01T00:00:01Z".into(),
            host: HostInfo {
                os: "linux".into(),
                arch: "x86_64".into(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: "my-bench".into(),
            cwd: None,
            command: vec![],
            repeat: 1,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples: vec![],
        stats: Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
            custom_metrics: std::collections::BTreeMap::new(),
        },
    }
}

#[tokio::test]
async fn test_upload_baseline() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/projects/my-project/baselines"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "bl_123",
            "benchmark": "my-bench",
            "version": "v1.2.3",
            "created_at": "2024-01-01T00:00:00Z",
            "etag": "hash123"
        })))
        .mount(&mock_server)
        .await;

    let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
    let receipt = create_test_receipt();
    let request = UploadBaselineRequest {
        benchmark: "my-bench".to_string(),
        version: Some("v1.2.3".to_string()),
        git_ref: None,
        git_sha: None,
        receipt,
        metadata: BTreeMap::new(),
        tags: vec![],
        normalize: true,
    };

    let result = client.upload_baseline("my-project", &request).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_promote_baseline() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/projects/my-project/baselines/my-bench/promote"))
        .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
            "id": "bl_new",
            "benchmark": "my-bench",
            "version": "v2.0.0",
            "promoted_from": "v1.0.0",
            "promoted_at": "2024-01-01T00:00:00Z",
            "created_at": "2024-01-01T00:00:00Z"
        })))
        .mount(&mock_server)
        .await;

    let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
    let request = PromoteBaselineRequest {
        from_version: "v1.0.0".to_string(),
        to_version: "v2.0.0".to_string(),
        git_ref: None,
        git_sha: None,
        tags: vec![],
        normalize: true,
    };

    let result = client
        .promote_baseline("my-project", "my-bench", &request)
        .await;
    assert!(result.is_ok());
}
