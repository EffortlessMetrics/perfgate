use anyhow::Context;
use perfgate_domain::compute_stats;
use perfgate_types::{
    AGGREGATE_SCHEMA_V1, AggregateInput, AggregateReceipt, AggregateRunnerMeta, AggregateVerdict,
    AggregationPolicy, FailIfNOfM, HostInfo, MetricStatus, RunMeta, RunReceipt,
};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub struct AggregateRequest {
    pub files: Vec<PathBuf>,
    pub policy: AggregationPolicy,
    pub quorum: Option<f64>,
    pub fail_if: Option<FailIfNOfM>,
    pub weights: BTreeMap<String, f64>,
    pub runner_class: Option<String>,
    pub lane: Option<String>,
}

pub struct AggregateOutcome {
    pub aggregate: AggregateReceipt,
    pub receipt: RunReceipt,
}

pub struct AggregateUseCase;

impl AggregateUseCase {
    pub fn execute(&self, req: AggregateRequest) -> anyhow::Result<AggregateOutcome> {
        if req.files.is_empty() {
            anyhow::bail!("No files provided for aggregation");
        }

        let mut receipts = Vec::new();
        let mut sources = Vec::new();
        let mut seen_run_ids = HashSet::new();
        for file in &req.files {
            let content =
                fs::read_to_string(file).with_context(|| format!("failed to read {:?}", file))?;
            let receipt: RunReceipt = serde_json::from_str(&content)
                .with_context(|| format!("failed to parse {:?}", file))?;
            if !seen_run_ids.insert(receipt.run.id.clone()) {
                anyhow::bail!(
                    "duplicate run id detected during aggregation: {}",
                    receipt.run.id
                );
            }
            sources.push(file.display().to_string());
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

        let mut inputs = Vec::with_capacity(receipts.len());
        for (idx, r) in receipts.iter().enumerate() {
            let label = format!("{}-{}", r.run.host.os, r.run.host.arch);
            let status = input_status(r);
            inputs.push(AggregateInput {
                source: sources[idx].clone(),
                run_id: r.run.id.clone(),
                bench_name: r.bench.name.clone(),
                host: r.run.host.clone(),
                runner: AggregateRunnerMeta {
                    label: label.clone(),
                    class: req.runner_class.clone(),
                    lane: req.lane.clone(),
                    weight: req.weights.get(&label).copied(),
                },
                status,
                reasons: input_reasons(r),
            });
        }

        let warnings = host_mismatch_warnings(&receipts);
        let verdict = evaluate_policy(&inputs, &req);

        let aggregate = AggregateReceipt {
            schema: AGGREGATE_SCHEMA_V1.to_string(),
            tool: receipts[0].tool.clone(),
            run: receipt.run.clone(),
            benchmark: first_bench_name.clone(),
            policy: req.policy,
            quorum: req.quorum,
            fail_if: req.fail_if,
            weights: req.weights,
            inputs,
            verdict,
            warnings,
        };

        Ok(AggregateOutcome { aggregate, receipt })
    }
}

fn input_status(receipt: &RunReceipt) -> MetricStatus {
    if receipt
        .samples
        .iter()
        .any(|s| s.exit_code != 0 || s.timed_out)
    {
        MetricStatus::Fail
    } else {
        MetricStatus::Pass
    }
}

fn input_reasons(receipt: &RunReceipt) -> Vec<String> {
    let failed = receipt.samples.iter().filter(|s| s.exit_code != 0).count();
    let timed_out = receipt.samples.iter().filter(|s| s.timed_out).count();
    let mut reasons = Vec::new();
    if failed > 0 {
        reasons.push(format!("{failed} sample(s) had non-zero exit codes"));
    }
    if timed_out > 0 {
        reasons.push(format!("{timed_out} sample(s) timed out"));
    }
    reasons
}

fn host_mismatch_warnings(receipts: &[RunReceipt]) -> Vec<String> {
    let Some(first) = receipts.first() else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    for r in receipts.iter().skip(1) {
        warnings.extend(compare_hosts(&first.run.host, &r.run.host));
    }
    warnings.sort();
    warnings.dedup();
    warnings
}

fn compare_hosts(a: &HostInfo, b: &HostInfo) -> Vec<String> {
    let mut reasons = Vec::new();
    if a.os != b.os {
        reasons.push(format!("host os mismatch: {} vs {}", a.os, b.os));
    }
    if a.arch != b.arch {
        reasons.push(format!("host arch mismatch: {} vs {}", a.arch, b.arch));
    }
    if let (Some(ca), Some(cb)) = (a.cpu_count, b.cpu_count)
        && (ca > cb.saturating_mul(2) || cb > ca.saturating_mul(2))
    {
        reasons.push(format!(
            "host cpu_count differs significantly: {} vs {}",
            ca, cb
        ));
    }
    if let (Some(ma), Some(mb)) = (a.memory_bytes, b.memory_bytes)
        && (ma > mb.saturating_mul(2) || mb > ma.saturating_mul(2))
    {
        reasons.push(format!(
            "host memory_bytes differs significantly: {} vs {}",
            ma, mb
        ));
    }
    if let (Some(ha), Some(hb)) = (&a.hostname_hash, &b.hostname_hash)
        && ha != hb
    {
        reasons.push("host hostname_hash mismatch".to_string());
    }
    reasons
}

fn evaluate_policy(inputs: &[AggregateInput], req: &AggregateRequest) -> AggregateVerdict {
    let passed = inputs
        .iter()
        .filter(|i| i.status == MetricStatus::Pass)
        .count() as u32;
    let failed = inputs.len() as u32 - passed;
    let total = inputs.len() as u32;

    let weighted_total: f64 = inputs
        .iter()
        .map(|i| req.weights.get(&i.runner.label).copied().unwrap_or(1.0))
        .sum();
    let weighted_pass: f64 = inputs
        .iter()
        .filter(|i| i.status == MetricStatus::Pass)
        .map(|i| req.weights.get(&i.runner.label).copied().unwrap_or(1.0))
        .sum();

    let mut reasons = Vec::new();
    let status = match req.policy {
        AggregationPolicy::All => {
            if failed == 0 {
                MetricStatus::Pass
            } else {
                reasons.push(format!(
                    "{failed} runner(s) failed under all-must-pass policy"
                ));
                MetricStatus::Fail
            }
        }
        AggregationPolicy::Majority => {
            if passed > failed {
                MetricStatus::Pass
            } else {
                reasons.push(format!(
                    "majority policy failed: pass={} fail={}",
                    passed, failed
                ));
                MetricStatus::Fail
            }
        }
        AggregationPolicy::Weighted => {
            let required = req.quorum.unwrap_or(0.5).clamp(0.0, 1.0);
            let ratio = if weighted_total == 0.0 {
                0.0
            } else {
                weighted_pass / weighted_total
            };
            if ratio >= required {
                MetricStatus::Pass
            } else {
                reasons.push(format!(
                    "weighted policy failed: score={ratio:.3}, required={required:.3}"
                ));
                MetricStatus::Fail
            }
        }
        AggregationPolicy::Quorum => {
            let required = req.quorum.unwrap_or(0.5).clamp(0.0, 1.0);
            let ratio = if total == 0 {
                0.0
            } else {
                passed as f64 / total as f64
            };
            if ratio >= required {
                MetricStatus::Pass
            } else {
                reasons.push(format!(
                    "quorum policy failed: score={ratio:.3}, required={required:.3}"
                ));
                MetricStatus::Fail
            }
        }
        AggregationPolicy::FailIfNOfM => {
            let fail_if = req.fail_if.clone().unwrap_or(FailIfNOfM { n: 1, m: None });
            let m = fail_if.m.unwrap_or(total);
            if total < m {
                reasons.push(format!(
                    "insufficient receipts: expected {m}, received {total}"
                ));
                MetricStatus::Fail
            } else if failed >= fail_if.n {
                reasons.push(format!(
                    "fail-if-n-of-m policy triggered: failed={failed} threshold={}",
                    fail_if.n
                ));
                MetricStatus::Fail
            } else {
                MetricStatus::Pass
            }
        }
    };

    AggregateVerdict {
        status,
        passed,
        failed,
        total,
        weighted_pass: matches!(req.policy, AggregationPolicy::Weighted).then_some(weighted_pass),
        weighted_total: matches!(req.policy, AggregationPolicy::Weighted).then_some(weighted_total),
        required: matches!(
            req.policy,
            AggregationPolicy::Weighted | AggregationPolicy::Quorum
        )
        .then_some(req.quorum.unwrap_or(0.5)),
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{BenchMeta, Sample, Stats, ToolInfo, U64Summary};

    fn mk_receipt(id: &str, os: &str, arch: &str, exit_code: i32) -> RunReceipt {
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
                    os: os.to_string(),
                    arch: arch.to_string(),
                    cpu_count: Some(8),
                    memory_bytes: Some(16 * 1024 * 1024 * 1024),
                    hostname_hash: None,
                },
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
                wall_ms: 10,
                exit_code,
                warmup: false,
                timed_out: false,
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
                wall_ms: U64Summary::new(10, 10, 10),
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
    fn majority_policy_passes_when_most_inputs_pass() {
        let inputs = vec![
            AggregateInput {
                source: "a".to_string(),
                run_id: "1".to_string(),
                bench_name: "bench".to_string(),
                host: mk_receipt("1", "linux", "x86_64", 0).run.host,
                runner: AggregateRunnerMeta {
                    label: "ubuntu-x86_64".to_string(),
                    class: None,
                    lane: None,
                    weight: None,
                },
                status: MetricStatus::Pass,
                reasons: vec![],
            },
            AggregateInput {
                source: "b".to_string(),
                run_id: "2".to_string(),
                bench_name: "bench".to_string(),
                host: mk_receipt("2", "linux", "x86_64", 1).run.host,
                runner: AggregateRunnerMeta {
                    label: "ubuntu-x86_64".to_string(),
                    class: None,
                    lane: None,
                    weight: None,
                },
                status: MetricStatus::Fail,
                reasons: vec!["non-zero".to_string()],
            },
            AggregateInput {
                source: "c".to_string(),
                run_id: "3".to_string(),
                bench_name: "bench".to_string(),
                host: mk_receipt("3", "linux", "x86_64", 0).run.host,
                runner: AggregateRunnerMeta {
                    label: "ubuntu-x86_64".to_string(),
                    class: None,
                    lane: None,
                    weight: None,
                },
                status: MetricStatus::Pass,
                reasons: vec![],
            },
        ];

        let verdict = evaluate_policy(
            &inputs,
            &AggregateRequest {
                files: vec![],
                policy: AggregationPolicy::Majority,
                quorum: None,
                fail_if: None,
                weights: BTreeMap::new(),
                runner_class: None,
                lane: None,
            },
        );
        assert_eq!(verdict.status, MetricStatus::Pass);
    }

    #[test]
    fn weighted_policy_uses_configured_weights() {
        let mut weights = BTreeMap::new();
        weights.insert("ubuntu-x86_64".to_string(), 0.8);
        weights.insert("macos-aarch64".to_string(), 0.2);
        let inputs = vec![
            AggregateInput {
                source: "a".to_string(),
                run_id: "1".to_string(),
                bench_name: "bench".to_string(),
                host: mk_receipt("1", "linux", "x86_64", 0).run.host,
                runner: AggregateRunnerMeta {
                    label: "ubuntu-x86_64".to_string(),
                    class: None,
                    lane: None,
                    weight: Some(0.8),
                },
                status: MetricStatus::Pass,
                reasons: vec![],
            },
            AggregateInput {
                source: "b".to_string(),
                run_id: "2".to_string(),
                bench_name: "bench".to_string(),
                host: mk_receipt("2", "macos", "aarch64", 1).run.host,
                runner: AggregateRunnerMeta {
                    label: "macos-aarch64".to_string(),
                    class: None,
                    lane: None,
                    weight: Some(0.2),
                },
                status: MetricStatus::Fail,
                reasons: vec!["non-zero".to_string()],
            },
        ];
        let verdict = evaluate_policy(
            &inputs,
            &AggregateRequest {
                files: vec![],
                policy: AggregationPolicy::Weighted,
                quorum: Some(0.7),
                fail_if: None,
                weights,
                runner_class: None,
                lane: None,
            },
        );
        assert_eq!(verdict.status, MetricStatus::Pass);
        assert_eq!(verdict.weighted_pass, Some(0.8));
    }
}
