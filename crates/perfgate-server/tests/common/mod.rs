//! Common test utilities for perfgate-server integration tests.
//!
//! This module provides helpers for creating test fixtures.

// API keys with proper format (pg_test_ prefix + 32+ alphanumeric chars)
pub const ADMIN_KEY: &str = "pg_test_admin0key00000000000000000000000001";
#[allow(dead_code)]
pub const PROMOTER_KEY: &str = "pg_test_promoter0key00000000000000000000002";
pub const CONTRIBUTOR_KEY: &str = "pg_test_contributor0key00000000000000000000003";
#[allow(dead_code)]
pub const VIEWER_KEY: &str = "pg_test_viewer0key0000000000000000000000004";

/// Creates a test upload request with default values.
pub fn create_test_upload_request(
    benchmark: &str,
) -> perfgate_client::types::UploadBaselineRequest {
    perfgate_client::types::UploadBaselineRequest {
        benchmark: benchmark.to_string(),
        version: Some("20240115-100000".to_string()),
        git_ref: Some("main".to_string()),
        git_sha: Some("abc123def456".to_string()),
        receipt: create_test_receipt(benchmark),
        metadata: std::collections::BTreeMap::new(),
        tags: vec!["test".to_string()],
        normalize: false,
    }
}

/// Creates a test receipt for use in tests.
pub fn create_test_receipt(benchmark: &str) -> perfgate_types::RunReceipt {
    use perfgate_types::{
        BenchMeta, HostInfo, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
    };

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
        samples: vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                max_rss_kb: Some(1024),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 102,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                max_rss_kb: Some(1028),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 98,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                max_rss_kb: Some(1020),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ],
        stats: Stats {
            wall_ms: U64Summary::new(100, 98, 102),
            max_rss_kb: Some(U64Summary::new(1024, 1020, 1028)),
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            binary_bytes: None,
            throughput_per_s: None,
        },
    }
}
