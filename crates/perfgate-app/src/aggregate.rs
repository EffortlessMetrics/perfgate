use anyhow::Context;
use perfgate_domain::compute_stats;
use perfgate_types::{HostInfo, RunMeta, RunReceipt};
use std::fs;
use std::path::PathBuf;

pub struct AggregateRequest {
    pub files: Vec<PathBuf>,
}

pub struct AggregateOutcome {
    pub receipt: RunReceipt,
}

pub struct AggregateUseCase;

impl AggregateUseCase {
    pub fn execute(&self, req: AggregateRequest) -> anyhow::Result<AggregateOutcome> {
        if req.files.is_empty() {
            anyhow::bail!("No files provided for aggregation");
        }

        let mut receipts = Vec::new();
        for file in &req.files {
            let content =
                fs::read_to_string(file).with_context(|| format!("failed to read {:?}", file))?;
            let receipt: RunReceipt = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse {:?}", file))?;
            receipts.push(receipt);
        }

        // Verify all receipts are for the same bench name
        let first_bench_name = &receipts[0].bench.name;
        for r in &receipts {
            if &r.bench.name != first_bench_name {
                anyhow::bail!(
                    "Cannot aggregate receipts for different benchmarks: {} vs {}",
                    first_bench_name,
                    r.bench.name
                );
            }
        }

        let mut combined_samples = Vec::new();
        for r in &receipts {
            combined_samples.extend(r.samples.clone());
        }

        // We assume work_units is consistent across the receipts.
        // If they differ, we take the first one.
        let work_units = receipts[0].bench.work_units;

        let stats = compute_stats(&combined_samples, work_units)?;

        // Update bench metadata.
        let mut bench = receipts[0].bench.clone();
        bench.repeat = combined_samples.len() as u32;

        let receipt = RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: receipts[0].tool.clone(),
            run: RunMeta {
                id: uuid::Uuid::new_v4().to_string(),
                started_at: receipts[0].run.started_at.clone(),
                ended_at: receipts.last().unwrap().run.ended_at.clone(),
                host: HostInfo {
                    os: "fleet".to_string(),
                    arch: "mixed".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench,
            samples: combined_samples,
            stats,
        };

        Ok(AggregateOutcome { receipt })
    }
}
