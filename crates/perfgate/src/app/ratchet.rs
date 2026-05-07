use perfgate_types::{
    CompareReceipt, Metric, MetricStatus, RatchetChange, RatchetConfig, RatchetMode,
    RatchetReceipt, ToolInfo, VerdictStatus,
};

#[derive(Debug, Clone, PartialEq)]
pub struct RatchetPlan {
    pub receipt: RatchetReceipt,
}

pub struct RatchetUseCase;

impl RatchetUseCase {
    pub fn preview(
        compare: &CompareReceipt,
        policy: &RatchetConfig,
        compare_path: Option<String>,
        host_mismatch_detected: bool,
        tool: ToolInfo,
    ) -> RatchetPlan {
        let mut changes = Vec::new();

        if !policy.enabled
            || compare.verdict.status != VerdictStatus::Pass
            || host_mismatch_detected
        {
            return RatchetPlan {
                receipt: RatchetReceipt {
                    schema: perfgate_types::RATCHET_SCHEMA_V1.to_string(),
                    tool,
                    bench_name: compare.bench.name.clone(),
                    compare_path,
                    changes,
                },
            };
        }

        for (metric, delta) in &compare.deltas {
            if !policy.allow_metrics.contains(metric) {
                continue;
            }
            if delta.status != MetricStatus::Pass {
                continue;
            }
            if let (Some(cv), Some(limit)) = (delta.cv, delta.noise_threshold)
                && cv > limit
            {
                continue;
            }
            if policy.require_significance {
                match &delta.significance {
                    Some(sig) if sig.significant => {}
                    _ => continue,
                }
            }

            let Some(current_budget) = compare.budgets.get(metric) else {
                continue;
            };

            let improvement = match current_budget.direction {
                perfgate_types::Direction::Lower => (-delta.pct).max(0.0),
                perfgate_types::Direction::Higher => delta.pct.max(0.0),
            };
            if improvement < policy.min_improvement {
                continue;
            }

            match policy.mode {
                RatchetMode::Threshold => {
                    // conservative tightening: bounded by max_tightening and observed improvement.
                    let tighten_by = improvement.min(policy.max_tightening);
                    let old_threshold = current_budget.threshold;
                    let new_threshold = old_threshold * (1.0 - tighten_by);
                    if new_threshold + f64::EPSILON < old_threshold {
                        changes.push(RatchetChange {
                            metric: *metric,
                            field: "threshold".to_string(),
                            old_value: old_threshold,
                            new_value: new_threshold,
                            reason: format!(
                                "improved {:.2}% (tightened by {:.2}% cap)",
                                improvement * 100.0,
                                tighten_by * 100.0
                            ),
                        });
                    }
                }
            }
        }

        RatchetPlan {
            receipt: RatchetReceipt {
                schema: perfgate_types::RATCHET_SCHEMA_V1.to_string(),
                tool,
                bench_name: compare.bench.name.clone(),
                compare_path,
                changes,
            },
        }
    }
}

pub fn is_host_mismatch_reason(reasons: &[String]) -> bool {
    reasons.iter().any(|r| r.contains("host_mismatch"))
}

pub fn preview_lines(changes: &[RatchetChange]) -> Vec<String> {
    if changes.is_empty() {
        return vec!["No ratchet changes eligible.".to_string()];
    }

    let mut lines = Vec::with_capacity(changes.len() + 1);
    lines.push("Planned ratchet changes:".to_string());
    for c in changes {
        lines.push(format!(
            "- {}.{}: {:.4} -> {:.4} ({})",
            metric_key(c.metric),
            c.field,
            c.old_value,
            c.new_value,
            c.reason
        ));
    }
    lines
}

fn metric_key(metric: Metric) -> &'static str {
    metric.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, Direction, MetricStatistic, Significance,
        SignificanceTest, ToolInfo, Verdict, VerdictCounts,
    };
    use std::collections::BTreeMap;

    fn mk_compare() -> CompareReceipt {
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::WallMs, Budget::new(0.20, 0.18, Direction::Lower));

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 90.0,
                ratio: 0.9,
                pct: -0.10,
                regression: 0.0,
                cv: Some(0.01),
                noise_threshold: Some(0.05),
                statistic: MetricStatistic::Median,
                significance: Some(Significance {
                    test: SignificanceTest::WelchT,
                    p_value: Some(0.01),
                    alpha: 0.05,
                    significant: true,
                    baseline_samples: 12,
                    current_samples: 12,
                    ci_lower: None,
                    ci_upper: None,
                }),
                status: MetricStatus::Pass,
            },
        );

        CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "test".into(),
            },
            bench: BenchMeta {
                name: "bench".into(),
                cwd: None,
                command: vec!["echo".into()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
        }
    }

    #[test]
    fn ratchet_threshold_tightens_when_eligible() {
        let policy = RatchetConfig {
            enabled: true,
            ..RatchetConfig::default()
        };
        let compare = mk_compare();
        let plan = RatchetUseCase::preview(
            &compare,
            &policy,
            None,
            false,
            ToolInfo {
                name: "perfgate".into(),
                version: "test".into(),
            },
        );

        assert_eq!(plan.receipt.changes.len(), 1);
        let c = &plan.receipt.changes[0];
        assert_eq!(c.metric, Metric::WallMs);
        assert!(c.new_value < c.old_value);
    }

    #[test]
    fn ratchet_skips_when_verdict_not_pass() {
        let policy = RatchetConfig {
            enabled: true,
            ..RatchetConfig::default()
        };
        let mut compare = mk_compare();
        compare.verdict.status = VerdictStatus::Warn;

        let plan = RatchetUseCase::preview(
            &compare,
            &policy,
            None,
            false,
            ToolInfo {
                name: "perfgate".into(),
                version: "test".into(),
            },
        );

        assert!(plan.receipt.changes.is_empty());
    }

    #[test]
    fn ratchet_tightens_higher_is_better_metrics() {
        let mut compare = mk_compare();
        compare
            .budgets
            .insert(Metric::WallMs, Budget::new(0.20, 0.18, Direction::Higher));
        compare
            .deltas
            .get_mut(&Metric::WallMs)
            .expect("wall delta")
            .pct = 0.10;

        let policy = RatchetConfig {
            enabled: true,
            ..RatchetConfig::default()
        };
        let plan = RatchetUseCase::preview(
            &compare,
            &policy,
            None,
            false,
            ToolInfo {
                name: "perfgate".into(),
                version: "test".into(),
            },
        );

        assert_eq!(plan.receipt.changes.len(), 1);
        assert!(plan.receipt.changes[0].new_value < plan.receipt.changes[0].old_value);
    }
}
