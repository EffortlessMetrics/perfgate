//! Promote use case for copying run receipts to become baselines.
//!
//! This module provides functionality for promoting a current run receipt
//! to become the new baseline for subsequent comparisons. This is typically
//! used on trusted branches (e.g., main) after successful benchmark runs.

use perfgate_types::{HostInfo, RunMeta, RunReceipt};

/// Request for promoting a run receipt to become a baseline.
#[derive(Debug, Clone)]
pub struct PromoteRequest {
    /// The run receipt to promote.
    pub receipt: RunReceipt,

    /// If true, strip run-specific fields (run_id, timestamps) to make
    /// the baseline more stable across runs.
    pub normalize: bool,
}

/// Result of a promote operation.
#[derive(Debug, Clone)]
pub struct PromoteResult {
    /// The (possibly normalized) receipt to be written as baseline.
    pub receipt: RunReceipt,
}

/// Use case for promoting run receipts to baselines.
pub struct PromoteUseCase;

impl PromoteUseCase {
    /// Execute the promote operation.
    ///
    /// If `normalize` is true, the receipt will have run-specific fields
    /// (run_id, started_at, ended_at) replaced with placeholder values
    /// to make the baseline more stable for comparison purposes.
    pub fn execute(req: PromoteRequest) -> PromoteResult {
        let receipt = if req.normalize {
            Self::normalize_receipt(req.receipt)
        } else {
            req.receipt
        };

        PromoteResult { receipt }
    }

    /// Normalize a receipt by stripping run-specific fields.
    fn normalize_receipt(mut receipt: RunReceipt) -> RunReceipt {
        receipt.run = RunMeta {
            id: "baseline".to_string(),
            started_at: "1970-01-01T00:00:00Z".to_string(),
            ended_at: "1970-01-01T00:00:00Z".to_string(),
            host: HostInfo {
                os: receipt.run.host.os,
                arch: receipt.run.host.arch,
                cpu_count: receipt.run.host.cpu_count,
                memory_bytes: receipt.run.host.memory_bytes,
                hostname_hash: receipt.run.host.hostname_hash,
            },
        };
        receipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{BenchMeta, HostInfo, RunMeta, Sample, Stats, ToolInfo, U64Summary};

    fn create_test_receipt() -> RunReceipt {
        RunReceipt {
            schema: "perfgate.run.v1".to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "unique-run-id-12345".to_string(),
                started_at: "2024-01-15T10:00:00Z".to_string(),
                ended_at: "2024-01-15T10:00:05Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(4),
                    memory_bytes: Some(8_000_000_000),
                    hostname_hash: Some("testhash123".to_string()),
                },
            },
            bench: BenchMeta {
                name: "test-benchmark".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                max_rss_kb: Some(1024),
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary {
                    median: 100,
                    min: 98,
                    max: 102,
                },
                max_rss_kb: Some(U64Summary {
                    median: 1024,
                    min: 1020,
                    max: 1028,
                }),
                throughput_per_s: None,
            },
        }
    }

    #[test]
    fn test_promote_without_normalize() {
        let receipt = create_test_receipt();
        let original_run_id = receipt.run.id.clone();
        let original_started_at = receipt.run.started_at.clone();

        let result = PromoteUseCase::execute(PromoteRequest {
            receipt,
            normalize: false,
        });

        // Without normalize, the receipt should be unchanged
        assert_eq!(result.receipt.run.id, original_run_id);
        assert_eq!(result.receipt.run.started_at, original_started_at);
    }

    #[test]
    fn test_promote_with_normalize() {
        let receipt = create_test_receipt();

        let result = PromoteUseCase::execute(PromoteRequest {
            receipt,
            normalize: true,
        });

        // With normalize, run-specific fields should be replaced
        assert_eq!(result.receipt.run.id, "baseline");
        assert_eq!(result.receipt.run.started_at, "1970-01-01T00:00:00Z");
        assert_eq!(result.receipt.run.ended_at, "1970-01-01T00:00:00Z");

        // Host info should be preserved
        assert_eq!(result.receipt.run.host.os, "linux");
        assert_eq!(result.receipt.run.host.arch, "x86_64");

        // Other fields should be unchanged
        assert_eq!(result.receipt.bench.name, "test-benchmark");
        assert_eq!(result.receipt.stats.wall_ms.median, 100);
    }

    #[test]
    fn test_normalize_preserves_bench_data() {
        let receipt = create_test_receipt();

        let result = PromoteUseCase::execute(PromoteRequest {
            receipt: receipt.clone(),
            normalize: true,
        });

        // Verify bench metadata is preserved
        assert_eq!(result.receipt.bench.name, receipt.bench.name);
        assert_eq!(result.receipt.bench.command, receipt.bench.command);
        assert_eq!(result.receipt.bench.repeat, receipt.bench.repeat);
        assert_eq!(result.receipt.bench.warmup, receipt.bench.warmup);

        // Verify samples are preserved
        assert_eq!(result.receipt.samples.len(), receipt.samples.len());
        assert_eq!(
            result.receipt.samples[0].wall_ms,
            receipt.samples[0].wall_ms
        );

        // Verify stats are preserved
        assert_eq!(
            result.receipt.stats.wall_ms.median,
            receipt.stats.wall_ms.median
        );
    }

    #[test]
    fn test_normalize_preserves_schema_and_tool() {
        let receipt = create_test_receipt();

        let result = PromoteUseCase::execute(PromoteRequest {
            receipt: receipt.clone(),
            normalize: true,
        });

        assert_eq!(result.receipt.schema, receipt.schema);
        assert_eq!(result.receipt.tool.name, receipt.tool.name);
        assert_eq!(result.receipt.tool.version, receipt.tool.version);
    }
}
