use anyhow::Context;
use perfgate_types::{
    AGGREGATE_SCHEMA_V1, AggregateMember, AggregateReceipt, AggregateStats, AggregationConfig,
    AggregationPolicy, BenchMeta, HostInfo, RunMeta, RunReceipt, Verdict, VerdictCounts,
    VerdictStatus,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

pub struct AggregateRequest {
    pub files: Vec<PathBuf>,
    pub config: AggregationConfig,
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

        let members = build_members(&receipts);
        let warnings = build_warnings(&members);
        let stats = aggregate_stats(&members, &req.config.weights);
        let verdict = aggregate_verdict(&members, &req.config, &stats)?;

        let receipt = AggregateReceipt {
            schema: AGGREGATE_SCHEMA_V1.to_string(),
            tool: receipts[0].tool.clone(),
            run: aggregate_run_meta(&receipts),
            bench: aggregate_bench_meta(&receipts[0].bench, members.len() as u32),
            config: req.config,
            stats,
            verdict,
            members,
            warnings,
        };

        Ok(AggregateOutcome { receipt })
    }
}

fn build_members(receipts: &[RunReceipt]) -> Vec<AggregateMember> {
    receipts
        .iter()
        .map(|receipt| {
            let status = if receipt
                .samples
                .iter()
                .any(|sample| sample.exit_code != 0 || sample.timed_out)
            {
                VerdictStatus::Fail
            } else {
                VerdictStatus::Pass
            };

            let runner_label = receipt
                .run
                .runner
                .as_ref()
                .map(|runner| runner.label.clone())
                .unwrap_or_else(|| format!("{}-{}", receipt.run.host.os, receipt.run.host.arch));

            let host_fingerprint = receipt
                .run
                .host_fingerprint
                .clone()
                .unwrap_or_else(|| fallback_host_fingerprint(&receipt.run.host));

            AggregateMember {
                run_id: receipt.run.id.clone(),
                bench_name: receipt.bench.name.clone(),
                runner_label,
                host_fingerprint,
                status,
            }
        })
        .collect()
}

fn fallback_host_fingerprint(host: &HostInfo) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        host.os,
        host.arch,
        host.cpu_count.map_or("na".to_string(), |v| v.to_string()),
        host.memory_bytes
            .map_or("na".to_string(), |v| v.to_string()),
        host.hostname_hash
            .clone()
            .unwrap_or_else(|| "na".to_string())
    )
}

fn build_warnings(members: &[AggregateMember]) -> Vec<String> {
    let mut warnings = Vec::new();
    let host_fingerprints: BTreeSet<_> =
        members.iter().map(|m| m.host_fingerprint.clone()).collect();
    if host_fingerprints.len() > 1 {
        warnings.push(format!(
            "host mismatch detected across aggregated members ({} unique host fingerprints)",
            host_fingerprints.len()
        ));
    }
    warnings
}

fn aggregate_stats(members: &[AggregateMember], weights: &BTreeMap<String, f64>) -> AggregateStats {
    let mut pass = 0u32;
    let mut fail = 0u32;
    let mut pass_weight = 0.0f64;
    let mut fail_weight = 0.0f64;

    for member in members {
        let weight = weights.get(&member.runner_label).copied().unwrap_or(1.0);
        match member.status {
            VerdictStatus::Pass => {
                pass += 1;
                pass_weight += weight;
            }
            VerdictStatus::Fail => {
                fail += 1;
                fail_weight += weight;
            }
            VerdictStatus::Warn | VerdictStatus::Skip => {}
        }
    }

    AggregateStats {
        total: members.len() as u32,
        pass,
        fail,
        pass_weight,
        fail_weight,
    }
}

fn aggregate_verdict(
    members: &[AggregateMember],
    config: &AggregationConfig,
    stats: &AggregateStats,
) -> anyhow::Result<Verdict> {
    let status = match config.policy {
        AggregationPolicy::All => {
            if stats.fail == 0 {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregationPolicy::Majority => {
            if stats.pass > stats.fail {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregationPolicy::Weighted => {
            if stats.pass_weight > stats.fail_weight {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregationPolicy::Quorum => {
            let quorum = config
                .quorum
                .unwrap_or_else(|| ((members.len() / 2) + 1) as u32);
            if stats.pass >= quorum {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregationPolicy::FailIfNOfM => {
            let n = config
                .fail_if_n
                .context("fail_if_n_of_m policy requires --fail-if-n")?;
            if stats.fail >= n {
                VerdictStatus::Fail
            } else {
                VerdictStatus::Pass
            }
        }
    };

    let reasons = vec![format!(
        "policy={} pass={} fail={} pass_weight={:.3} fail_weight={:.3}",
        config.policy.as_str(),
        stats.pass,
        stats.fail,
        stats.pass_weight,
        stats.fail_weight
    )];

    Ok(Verdict {
        status,
        counts: VerdictCounts {
            pass: stats.pass,
            warn: 0,
            fail: stats.fail,
            skip: 0,
        },
        reasons,
    })
}

fn aggregate_run_meta(receipts: &[RunReceipt]) -> RunMeta {
    RunMeta {
        id: uuid::Uuid::new_v4().to_string(),
        started_at: receipts[0].run.started_at.clone(),
        ended_at: receipts
            .last()
            .map(|r| r.run.ended_at.clone())
            .unwrap_or_else(|| receipts[0].run.ended_at.clone()),
        host: HostInfo {
            os: "fleet".to_string(),
            arch: "mixed".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        },
        runner: None,
        host_fingerprint: None,
        lane: None,
    }
}

fn aggregate_bench_meta(first: &BenchMeta, repeat: u32) -> BenchMeta {
    let mut bench = first.clone();
    bench.repeat = repeat;
    bench
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{RunReceipt, Sample, Stats, ToolInfo, U64Summary};

    fn mk_receipt(run_id: &str, label: &str, exit_code: i32) -> RunReceipt {
        RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "test".to_string(),
            },
            run: RunMeta {
                id: run_id.to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: "2026-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(8),
                    memory_bytes: Some(16_000_000_000),
                    hostname_hash: Some(format!("hash-{label}")),
                },
                runner: Some(perfgate_types::RunnerMeta {
                    label: label.to_string(),
                    class: None,
                    weight: None,
                }),
                host_fingerprint: Some(format!("fp-{label}")),
                lane: None,
            },
            bench: BenchMeta {
                name: "bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![Sample {
                wall_ms: 100,
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
                wall_ms: U64Summary::new(100, 90, 110),
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
    fn majority_passes_when_more_pass_than_fail() {
        let members = build_members(&[
            mk_receipt("a", "ubuntu-x86_64", 0),
            mk_receipt("b", "windows-x86_64", 0),
            mk_receipt("c", "macos-arm64", 1),
        ]);
        let config = AggregationConfig {
            policy: AggregationPolicy::Majority,
            quorum: None,
            fail_if_n: None,
            weights: BTreeMap::new(),
        };
        let stats = aggregate_stats(&members, &config.weights);
        let verdict = aggregate_verdict(&members, &config, &stats).unwrap();
        assert_eq!(verdict.status, VerdictStatus::Pass);
    }

    #[test]
    fn weighted_fails_when_fail_weight_dominates() {
        let members = build_members(&[
            mk_receipt("a", "ubuntu-x86_64", 0),
            mk_receipt("b", "windows-x86_64", 1),
        ]);
        let mut weights = BTreeMap::new();
        weights.insert("ubuntu-x86_64".to_string(), 0.2);
        weights.insert("windows-x86_64".to_string(), 0.8);
        let config = AggregationConfig {
            policy: AggregationPolicy::Weighted,
            quorum: None,
            fail_if_n: None,
            weights,
        };
        let stats = aggregate_stats(&members, &config.weights);
        let verdict = aggregate_verdict(&members, &config, &stats).unwrap();
        assert_eq!(verdict.status, VerdictStatus::Fail);
    }
}
