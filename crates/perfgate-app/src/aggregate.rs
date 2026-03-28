use anyhow::Context;
use perfgate_domain::detect_host_mismatch;
use perfgate_types::{
    AGGREGATE_SCHEMA_V1, AggregatePolicyConfig, AggregateReceipt, AggregateRunEvidence,
    AggregateStatus, AggregateVerdict, AggregationPolicy, RunMeta, RunReceipt,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

pub struct AggregateRequest {
    pub files: Vec<PathBuf>,
    pub policy: AggregationPolicy,
    pub quorum: Option<u32>,
    pub fail_threshold: Option<u32>,
    pub weights: BTreeMap<String, f64>,
}

pub struct AggregateOutcome {
    pub receipt: AggregateReceipt,
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

        let mut seen_ids = BTreeSet::new();
        for r in &receipts {
            if !seen_ids.insert(r.run.id.clone()) {
                anyhow::bail!("duplicate run id in aggregation inputs: {}", r.run.id);
            }
        }

        let mut host_mismatch_warnings = Vec::new();
        let baseline_host = &receipts[0].run.host;
        for r in receipts.iter().skip(1) {
            if let Some(mismatch) = detect_host_mismatch(baseline_host, &r.run.host) {
                for reason in mismatch.reasons {
                    host_mismatch_warnings.push(format!(
                        "run {} host mismatch vs {}: {}",
                        r.run.id, receipts[0].run.id, reason
                    ));
                }
            }
        }

        let runs: Vec<AggregateRunEvidence> = receipts.iter().map(run_evidence).collect();
        let verdict = compute_verdict(&runs, &req)?;

        let receipt = AggregateReceipt {
            schema: AGGREGATE_SCHEMA_V1.to_string(),
            tool: receipts[0].tool.clone(),
            run: RunMeta {
                id: uuid::Uuid::new_v4().to_string(),
                started_at: receipts[0].run.started_at.clone(),
                ended_at: receipts.last().unwrap().run.ended_at.clone(),
                host: receipts[0].run.host.clone(),
                runner: None,
            },
            bench_name: first_bench_name.clone(),
            policy: AggregatePolicyConfig {
                policy: req.policy,
                quorum: req.quorum,
                fail_threshold: req.fail_threshold,
                weights: req.weights,
            },
            verdict,
            host_mismatch_warnings,
            runs,
        };

        Ok(AggregateOutcome { receipt })
    }
}

fn run_evidence(receipt: &RunReceipt) -> AggregateRunEvidence {
    let mut reasons = Vec::new();
    for (idx, sample) in receipt.samples.iter().enumerate() {
        if sample.timed_out {
            reasons.push(format!("sample {} timed out", idx));
        }
        if sample.exit_code != 0 {
            reasons.push(format!("sample {} exit_code={}", idx, sample.exit_code));
        }
    }
    AggregateRunEvidence {
        run_id: receipt.run.id.clone(),
        bench_name: receipt.bench.name.clone(),
        host: receipt.run.host.clone(),
        runner: receipt.run.runner.clone(),
        pass: reasons.is_empty(),
        reasons,
    }
}

fn compute_verdict(
    runs: &[AggregateRunEvidence],
    req: &AggregateRequest,
) -> anyhow::Result<AggregateVerdict> {
    let pass_count = runs.iter().filter(|r| r.pass).count() as u32;
    let total_count = runs.len() as u32;
    let fail_count = total_count.saturating_sub(pass_count);

    let verdict = match req.policy {
        AggregationPolicy::All => AggregateVerdict {
            status: if pass_count == total_count {
                AggregateStatus::Pass
            } else {
                AggregateStatus::Fail
            },
            reason: format!("all policy: {}/{} runs passed", pass_count, total_count),
            pass_count,
            fail_count,
            total_count,
            pass_weight: None,
            fail_weight: None,
            total_weight: None,
        },
        AggregationPolicy::Majority => AggregateVerdict {
            status: if pass_count > total_count / 2 {
                AggregateStatus::Pass
            } else {
                AggregateStatus::Fail
            },
            reason: format!(
                "majority policy: {}/{} runs passed",
                pass_count, total_count
            ),
            pass_count,
            fail_count,
            total_count,
            pass_weight: None,
            fail_weight: None,
            total_weight: None,
        },
        AggregationPolicy::Weighted => {
            let mut pass_weight = 0.0f64;
            let mut fail_weight = 0.0f64;
            for run in runs {
                let label = run
                    .runner
                    .as_ref()
                    .and_then(|r| r.label.as_deref().map(ToString::to_string))
                    .unwrap_or_else(|| format!("{}-{}", run.host.os, run.host.arch));
                let weight = req
                    .weights
                    .get(&label)
                    .copied()
                    .or_else(|| run.runner.as_ref().and_then(|r| r.weight))
                    .unwrap_or(1.0);
                if run.pass {
                    pass_weight += weight;
                } else {
                    fail_weight += weight;
                }
            }
            let total_weight = pass_weight + fail_weight;
            if total_weight <= 0.0 {
                anyhow::bail!("weighted policy requires a positive total weight");
            }
            AggregateVerdict {
                status: if pass_weight > total_weight / 2.0 {
                    AggregateStatus::Pass
                } else {
                    AggregateStatus::Fail
                },
                reason: format!(
                    "weighted policy: pass_weight={:.3}, total_weight={:.3}",
                    pass_weight, total_weight
                ),
                pass_count,
                fail_count,
                total_count,
                pass_weight: Some(pass_weight),
                fail_weight: Some(fail_weight),
                total_weight: Some(total_weight),
            }
        }
        AggregationPolicy::Quorum => {
            let quorum = req
                .quorum
                .ok_or_else(|| anyhow::anyhow!("quorum policy requires --quorum"))?;
            AggregateVerdict {
                status: if pass_count >= quorum {
                    AggregateStatus::Pass
                } else {
                    AggregateStatus::Fail
                },
                reason: format!(
                    "quorum policy: pass_count={} required_quorum={}",
                    pass_count, quorum
                ),
                pass_count,
                fail_count,
                total_count,
                pass_weight: None,
                fail_weight: None,
                total_weight: None,
            }
        }
        AggregationPolicy::FailIfNOfM => {
            let threshold = req.fail_threshold.ok_or_else(|| {
                anyhow::anyhow!("fail_if_n_of_m policy requires --fail-threshold")
            })?;
            AggregateVerdict {
                status: if fail_count >= threshold {
                    AggregateStatus::Fail
                } else {
                    AggregateStatus::Pass
                },
                reason: format!(
                    "fail_if_n_of_m policy: fail_count={} fail_threshold={}",
                    fail_count, threshold
                ),
                pass_count,
                fail_count,
                total_count,
                pass_weight: None,
                fail_weight: None,
                total_weight: None,
            }
        }
    };
    Ok(verdict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{BenchMeta, HostInfo, Sample, Stats, ToolInfo, U64Summary};

    fn run(id: &str, label: Option<&str>, pass: bool) -> RunReceipt {
        RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "test".to_string(),
            },
            run: RunMeta {
                id: id.to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: "2026-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(8),
                    memory_bytes: Some(16 * 1024 * 1024 * 1024),
                    hostname_hash: Some(format!("host-{id}")),
                },
                runner: Some(perfgate_types::RunnerMeta {
                    label: label.map(ToString::to_string),
                    class: None,
                    weight: None,
                    lane: Some("default".to_string()),
                }),
            },
            bench: BenchMeta {
                name: "bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "x".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![Sample {
                wall_ms: 100,
                exit_code: if pass { 0 } else { 1 },
                warmup: false,
                timed_out: !pass,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }],
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

    #[test]
    fn majority_policy_passes_when_strict_majority_pass() {
        let runs = vec![
            run_evidence(&run("r1", Some("ubuntu-x86_64"), true)),
            run_evidence(&run("r2", Some("windows-x86_64"), true)),
            run_evidence(&run("r3", Some("macos-arm64"), false)),
        ];
        let req = AggregateRequest {
            files: vec![],
            policy: AggregationPolicy::Majority,
            quorum: None,
            fail_threshold: None,
            weights: BTreeMap::new(),
        };
        let verdict = compute_verdict(&runs, &req).expect("verdict");
        assert_eq!(verdict.status, AggregateStatus::Pass);
    }

    #[test]
    fn weighted_policy_uses_configured_weights() {
        let runs = vec![
            run_evidence(&run("r1", Some("ubuntu-x86_64"), true)),
            run_evidence(&run("r2", Some("windows-x86_64"), false)),
            run_evidence(&run("r3", Some("macos-arm64"), false)),
        ];
        let mut weights = BTreeMap::new();
        weights.insert("ubuntu-x86_64".to_string(), 0.7);
        weights.insert("windows-x86_64".to_string(), 0.2);
        weights.insert("macos-arm64".to_string(), 0.1);
        let req = AggregateRequest {
            files: vec![],
            policy: AggregationPolicy::Weighted,
            quorum: None,
            fail_threshold: None,
            weights,
        };
        let verdict = compute_verdict(&runs, &req).expect("verdict");
        assert_eq!(verdict.status, AggregateStatus::Pass);
    }
}
