use perfgate_budget::evaluate_ratchet_threshold;
use perfgate_types::{
    CompareReceipt, ConfigFile, MetricStatus, RATCHET_SCHEMA_V1, RatchetChange, RatchetConfig,
    RatchetEvidence, RatchetReceipt, VerdictStatus,
};

#[derive(Debug, Clone)]
pub struct RatchetRequest {
    pub compare: CompareReceipt,
    pub config: ConfigFile,
}

#[derive(Debug, Clone)]
pub struct RatchetOutcome {
    pub bench_name: String,
    pub changes: Vec<RatchetChange>,
    pub skipped: Vec<String>,
}

pub struct RatchetUseCase;

impl RatchetUseCase {
    pub fn preview(req: RatchetRequest) -> RatchetOutcome {
        let mut changes = Vec::new();
        let mut skipped = Vec::new();
        let compare = req.compare;
        let policy: RatchetConfig = req.config.ratchet;

        if compare.verdict.status != VerdictStatus::Pass {
            skipped.push("verdict is not pass".to_string());
            return RatchetOutcome {
                bench_name: compare.bench.name,
                changes,
                skipped,
            };
        }

        let host_mismatch = compare
            .verdict
            .reasons
            .iter()
            .any(|r| r == perfgate_types::VERDICT_REASON_HOST_MISMATCH);
        if host_mismatch {
            skipped.push("host mismatch present".to_string());
            return RatchetOutcome {
                bench_name: compare.bench.name,
                changes,
                skipped,
            };
        }

        let Some(bench_cfg) = req
            .config
            .benches
            .iter()
            .find(|b| b.name == compare.bench.name)
        else {
            skipped.push(format!(
                "bench '{}' not found in config",
                compare.bench.name
            ));
            return RatchetOutcome {
                bench_name: compare.bench.name,
                changes,
                skipped,
            };
        };

        let Some(budget_overrides) = &bench_cfg.budgets else {
            skipped.push("bench has no explicit budgets to ratchet".to_string());
            return RatchetOutcome {
                bench_name: compare.bench.name,
                changes,
                skipped,
            };
        };

        for metric in &policy.allow_metrics {
            let Some(delta) = compare.deltas.get(metric) else {
                skipped.push(format!("{}: missing delta", metric.as_str()));
                continue;
            };
            if delta.status != MetricStatus::Pass {
                skipped.push(format!("{}: status is not pass", metric.as_str()));
                continue;
            }
            let Some(override_budget) = budget_overrides.get(metric) else {
                skipped.push(format!("{}: no config budget override", metric.as_str()));
                continue;
            };
            let Some(current_threshold) = override_budget.threshold else {
                skipped.push(format!("{}: threshold missing in config", metric.as_str()));
                continue;
            };
            let Some(compare_budget) = compare.budgets.get(metric) else {
                skipped.push(format!("{}: compare budget missing", metric.as_str()));
                continue;
            };

            let noisy =
                matches!(delta.cv.zip(delta.noise_threshold), Some((cv, limit)) if cv > limit);
            if noisy {
                skipped.push(format!("{}: noisy sample detected", metric.as_str()));
                continue;
            }

            let significance_met = delta
                .significance
                .as_ref()
                .map(|s| s.significant)
                .unwrap_or(false);
            if policy.require_significance && !significance_met {
                skipped.push(format!("{}: significance not met", metric.as_str()));
                continue;
            }

            let decision = evaluate_ratchet_threshold(
                delta.baseline,
                delta.current,
                current_threshold,
                compare_budget.direction,
                &policy,
            );
            if !decision.eligible {
                skipped.push(format!("{}: {}", metric.as_str(), decision.reason));
                continue;
            }

            let new_threshold = decision.proposed_threshold.unwrap_or(current_threshold);
            if new_threshold >= current_threshold {
                continue;
            }

            changes.push(RatchetChange {
                metric: *metric,
                old_threshold: current_threshold,
                new_threshold,
                improvement: decision.improvement,
                reason: decision.reason,
                confidence: RatchetEvidence {
                    verdict_pass: true,
                    host_mismatch,
                    noisy,
                    significance_met: !policy.require_significance || significance_met,
                },
            });
        }

        RatchetOutcome {
            bench_name: compare.bench.name,
            changes,
            skipped,
        }
    }

    pub fn build_receipt(
        outcome: &RatchetOutcome,
        policy: RatchetConfig,
        applied: bool,
    ) -> RatchetReceipt {
        RatchetReceipt {
            schema: RATCHET_SCHEMA_V1.to_string(),
            bench_name: outcome.bench_name.clone(),
            mode: policy.mode,
            applied,
            changes: outcome.changes.clone(),
            skipped: outcome.skipped.clone(),
        }
    }
}

pub fn render_preview(outcome: &RatchetOutcome) -> String {
    let mut out = String::new();
    if outcome.changes.is_empty() {
        out.push_str("No ratchet changes proposed.\n");
    } else {
        out.push_str("Proposed ratchet changes:\n");
        for change in &outcome.changes {
            out.push_str(&format!(
                "- {}: {:.6} -> {:.6} (improvement {:.2}%)\n",
                change.metric.as_str(),
                change.old_threshold,
                change.new_threshold,
                change.improvement * 100.0
            ));
        }
    }
    if !outcome.skipped.is_empty() {
        out.push_str("Skipped:\n");
        for reason in &outcome.skipped {
            out.push_str(&format!("- {reason}\n"));
        }
    }
    out
}
