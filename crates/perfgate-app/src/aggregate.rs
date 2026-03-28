use anyhow::Context;
use perfgate_types::{
    AGGREGATE_SCHEMA_V1, AggregateMember, AggregatePolicy, AggregateReceipt, AggregateSummary,
    RunReceipt, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

pub struct AggregateRequest {
    pub files: Vec<PathBuf>,
    pub policy: AggregatePolicy,
    pub quorum: Option<f64>,
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
            receipts.push((file, receipt));
        }

        let first_bench_name = &receipts[0].1.bench.name;
        for (_, r) in &receipts {
            if &r.bench.name != first_bench_name {
                anyhow::bail!(
                    "Cannot aggregate receipts for different benchmarks: {} vs {}",
                    first_bench_name,
                    r.bench.name
                );
            }
        }

        let mut warnings = Vec::new();
        let mut seen_ids = BTreeSet::new();
        let mut members = Vec::with_capacity(receipts.len());

        for (path, receipt) in receipts {
            if !seen_ids.insert(receipt.run.id.clone()) {
                warnings.push(format!(
                    "duplicate run id detected: {} ({})",
                    receipt.run.id,
                    path.display()
                ));
            }

            let has_failures = receipt
                .samples
                .iter()
                .filter(|s| !s.warmup)
                .any(|s| s.timed_out || s.exit_code != 0);

            let status = if has_failures {
                VerdictStatus::Fail
            } else {
                VerdictStatus::Pass
            };

            let runner_label = format!("{}-{}", receipt.run.host.os, receipt.run.host.arch);
            let weight = req.weights.get(&runner_label).copied().unwrap_or(1.0);
            members.push(AggregateMember {
                path: path.display().to_string(),
                run_id: receipt.run.id,
                bench_name: receipt.bench.name,
                runner: runner_label,
                os: receipt.run.host.os,
                arch: receipt.run.host.arch,
                host_fingerprint: receipt.run.host.hostname_hash,
                status,
                weight,
                reasons: if has_failures {
                    vec!["nonzero_or_timeout_sample".to_string()]
                } else {
                    Vec::new()
                },
            });
        }

        let summary = summarize(&members);

        // Preserve host mismatch signal at fleet-level: mixed host tuples are surfaced as warnings.
        let host_variants: BTreeSet<(String, String)> = members
            .iter()
            .map(|m| (m.os.clone(), m.arch.clone()))
            .collect();
        if host_variants.len() > 1 {
            let labels: Vec<String> = host_variants
                .iter()
                .map(|(os, arch)| format!("{os}/{arch}"))
                .collect();
            warnings.push(format!(
                "host mismatch across fleet inputs: {}",
                labels.join(", ")
            ));
        }

        let status = evaluate_policy(req.policy, &summary, req.quorum, req.fail_threshold)?;
        let counts = VerdictCounts {
            pass: summary.pass,
            warn: summary.warn,
            fail: summary.fail,
            skip: summary.skip,
        };
        let mut reasons = vec![format!("policy.{}", req.policy.as_str())];
        if status == VerdictStatus::Fail {
            reasons.push("fleet_gate_failed".to_string());
        }

        let receipt = AggregateReceipt {
            schema: AGGREGATE_SCHEMA_V1.to_string(),
            tool: perfgate_types::ToolInfo {
                name: "perfgate".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            policy: req.policy,
            verdict: Verdict {
                status,
                counts,
                reasons,
            },
            summary,
            warnings,
            members,
        };

        Ok(AggregateOutcome { receipt })
    }
}

fn summarize(members: &[AggregateMember]) -> AggregateSummary {
    let mut summary = AggregateSummary {
        total: members.len() as u32,
        pass: 0,
        fail: 0,
        warn: 0,
        skip: 0,
        pass_weight: 0.0,
        fail_weight: 0.0,
    };

    for m in members {
        match m.status {
            VerdictStatus::Pass => {
                summary.pass += 1;
                summary.pass_weight += m.weight;
            }
            VerdictStatus::Fail => {
                summary.fail += 1;
                summary.fail_weight += m.weight;
            }
            VerdictStatus::Warn => summary.warn += 1,
            VerdictStatus::Skip => summary.skip += 1,
        }
    }

    summary
}

fn evaluate_policy(
    policy: AggregatePolicy,
    summary: &AggregateSummary,
    quorum: Option<f64>,
    fail_threshold: Option<u32>,
) -> anyhow::Result<VerdictStatus> {
    if summary.total == 0 {
        return Ok(VerdictStatus::Skip);
    }

    let pass = summary.pass;
    let fail = summary.fail;
    let total = summary.total;

    let status = match policy {
        AggregatePolicy::All => {
            if fail == 0 {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregatePolicy::Majority => {
            if pass > fail {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregatePolicy::Weighted => {
            if summary.pass_weight > summary.fail_weight {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregatePolicy::Quorum => {
            let required = quorum.unwrap_or(0.67);
            if !(0.0..=1.0).contains(&required) {
                anyhow::bail!("quorum must be in [0.0, 1.0]");
            }
            let ratio = (pass as f64) / (total as f64);
            if ratio >= required {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Fail
            }
        }
        AggregatePolicy::FailIfNOfM => {
            let threshold = fail_threshold.unwrap_or(1);
            if fail >= threshold {
                VerdictStatus::Fail
            } else {
                VerdictStatus::Pass
            }
        }
    };

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(status: VerdictStatus, weight: f64) -> AggregateMember {
        AggregateMember {
            path: "x.json".into(),
            run_id: "run-1".into(),
            bench_name: "bench".into(),
            runner: "linux-x86_64".into(),
            os: "linux".into(),
            arch: "x86_64".into(),
            host_fingerprint: None,
            status,
            weight,
            reasons: vec![],
        }
    }

    #[test]
    fn majority_policy_passes_when_more_pass_than_fail() {
        let s = summarize(&[
            member(VerdictStatus::Pass, 1.0),
            member(VerdictStatus::Pass, 1.0),
            member(VerdictStatus::Fail, 1.0),
        ]);
        let out = evaluate_policy(AggregatePolicy::Majority, &s, None, None).unwrap();
        assert_eq!(out, VerdictStatus::Pass);
    }

    #[test]
    fn weighted_policy_uses_weights() {
        let s = summarize(&[
            member(VerdictStatus::Pass, 0.2),
            member(VerdictStatus::Fail, 0.8),
        ]);
        let out = evaluate_policy(AggregatePolicy::Weighted, &s, None, None).unwrap();
        assert_eq!(out, VerdictStatus::Fail);
    }

    #[test]
    fn fail_if_n_of_m_respects_threshold() {
        let s = summarize(&[
            member(VerdictStatus::Fail, 1.0),
            member(VerdictStatus::Pass, 1.0),
        ]);
        let out = evaluate_policy(AggregatePolicy::FailIfNOfM, &s, None, Some(2)).unwrap();
        assert_eq!(out, VerdictStatus::Pass);
    }
}
