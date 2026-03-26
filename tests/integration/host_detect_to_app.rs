//! Integration tests: host-detect crate → app layer.
//!
//! These tests verify that perfgate-host-detect integrates correctly
//! with the perfgate-app layer, and that host mismatches flow through
//! to CLI output.

use perfgate_app::{CompareRequest, CompareUseCase};
use perfgate_host_detect::detect_host_mismatch;
use perfgate_types::{
    BenchMeta, Budget, CompareRef, Direction, HostInfo, HostMismatchPolicy, Metric, RUN_SCHEMA_V1,
    RunMeta, RunReceipt, Stats, ToolInfo, U64Summary,
};
use std::collections::BTreeMap;

fn make_run_receipt(host: HostInfo) -> RunReceipt {
    RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        run: RunMeta {
            id: "test-id".to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            ended_at: "2024-01-01T00:00:01Z".to_string(),
            host,
        },
        bench: BenchMeta {
            name: "test-bench".to_string(),
            cwd: None,
            command: vec!["echo".to_string()],
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
        },
    }
}

fn make_budgets() -> BTreeMap<Metric, Budget> {
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));
    budgets
}

#[test]
fn host_detect_finds_os_mismatch() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };
    let current = HostInfo {
        os: "windows".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_some());

    let info = mismatch.unwrap();
    assert!(info.reasons.iter().any(|r| r.contains("OS mismatch")));
}

#[test]
fn host_detect_finds_arch_mismatch() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };
    let current = HostInfo {
        os: "linux".to_string(),
        arch: "aarch64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_some());

    let info = mismatch.unwrap();
    assert!(info.reasons.iter().any(|r| r.contains("architecture")));
}

#[test]
fn host_detect_finds_cpu_count_difference() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(4),
        memory_bytes: None,
        hostname_hash: None,
    };
    let current = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(16),
        memory_bytes: None,
        hostname_hash: None,
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_some());
}

#[test]
fn host_detect_finds_memory_difference() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: Some(8 * 1024 * 1024 * 1024),
        hostname_hash: None,
    };
    let current = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: Some(32 * 1024 * 1024 * 1024),
        hostname_hash: None,
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_some());
}

#[test]
fn host_detect_finds_hostname_difference() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: Some("abc123".to_string()),
    };
    let current = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: Some("def456".to_string()),
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_some());
}

#[test]
fn host_detect_returns_none_for_matching_hosts() {
    let host = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(8),
        memory_bytes: Some(16 * 1024 * 1024 * 1024),
        hostname_hash: Some("same".to_string()),
    };

    let mismatch = detect_host_mismatch(&host, &host);
    assert!(mismatch.is_none());
}

#[test]
fn app_compare_with_host_mismatch_warn_policy() {
    let baseline_host = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };
    let current_host = HostInfo {
        os: "windows".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    let result = CompareUseCase::execute(CompareRequest {
        baseline: make_run_receipt(baseline_host),
        current: make_run_receipt(current_host),
        budgets: make_budgets(),
        metric_statistics: BTreeMap::new(),
        significance: None,
        baseline_ref: CompareRef {
            path: None,
            run_id: None,
        },
        current_ref: CompareRef {
            path: None,
            run_id: None,
        },
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Warn,
    })
    .unwrap();

    assert!(result.host_mismatch.is_some());
}

#[test]
fn app_compare_with_host_mismatch_error_policy() {
    let baseline_host = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };
    let current_host = HostInfo {
        os: "windows".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    let result = CompareUseCase::execute(CompareRequest {
        baseline: make_run_receipt(baseline_host),
        current: make_run_receipt(current_host),
        budgets: make_budgets(),
        metric_statistics: BTreeMap::new(),
        significance: None,
        baseline_ref: CompareRef {
            path: None,
            run_id: None,
        },
        current_ref: CompareRef {
            path: None,
            run_id: None,
        },
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Error,
    });

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("host mismatch"));
}

#[test]
fn app_compare_with_host_mismatch_ignore_policy() {
    let baseline_host = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };
    let current_host = HostInfo {
        os: "windows".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    };

    let result = CompareUseCase::execute(CompareRequest {
        baseline: make_run_receipt(baseline_host),
        current: make_run_receipt(current_host),
        budgets: make_budgets(),
        metric_statistics: BTreeMap::new(),
        significance: None,
        baseline_ref: CompareRef {
            path: None,
            run_id: None,
        },
        current_ref: CompareRef {
            path: None,
            run_id: None,
        },
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Ignore,
    })
    .unwrap();

    assert!(result.host_mismatch.is_none());
}

#[test]
fn app_compare_matching_hosts_no_mismatch() {
    let host = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(8),
        memory_bytes: Some(16 * 1024 * 1024 * 1024),
        hostname_hash: None,
    };

    let result = CompareUseCase::execute(CompareRequest {
        baseline: make_run_receipt(host.clone()),
        current: make_run_receipt(host),
        budgets: make_budgets(),
        metric_statistics: BTreeMap::new(),
        significance: None,
        baseline_ref: CompareRef {
            path: None,
            run_id: None,
        },
        current_ref: CompareRef {
            path: None,
            run_id: None,
        },
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        host_mismatch_policy: HostMismatchPolicy::Warn,
    })
    .unwrap();

    assert!(result.host_mismatch.is_none());
}

#[test]
fn host_detect_minor_cpu_difference_ignored() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(8),
        memory_bytes: None,
        hostname_hash: None,
    };
    let current = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(12),
        memory_bytes: None,
        hostname_hash: None,
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_none());
}

#[test]
fn host_detect_exact_2x_threshold_not_mismatch() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(4),
        memory_bytes: Some(8 * 1024 * 1024 * 1024),
        hostname_hash: None,
    };
    let current = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(8),
        memory_bytes: Some(16 * 1024 * 1024 * 1024),
        hostname_hash: None,
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_none());
}

#[test]
fn host_detect_just_over_2x_is_mismatch() {
    let baseline = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(4),
        memory_bytes: None,
        hostname_hash: None,
    };
    let current = HostInfo {
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        cpu_count: Some(9),
        memory_bytes: None,
        hostname_hash: None,
    };

    let mismatch = detect_host_mismatch(&baseline, &current);
    assert!(mismatch.is_some());
}
