use perfgate_budget::tightened_threshold;
use perfgate_config::ThresholdRatchetEdit;
use perfgate_types::{
    CompareReceipt, CompareRef, MetricStatus, RATCHET_SCHEMA_V1, RatchetChange,
    RatchetEvidence, RatchetMode, RatchetPolicyConfig, RatchetReceipt, ToolInfo, VerdictStatus,
};

#[derive(Debug, Clone)]
pub struct RatchetRequest {
    pub compare: CompareReceipt,
    pub policy: RatchetPolicyConfig,
    pub compare_ref: CompareRef,
    pub tool: ToolInfo,
}

#[derive(Debug, Clone, Default)]
pub struct RatchetResult {
    pub changes: Vec<RatchetChange>,
    pub skipped: Vec<String>,
}

impl RatchetResult {
    pub fn threshold_edits(&self) -> Vec<ThresholdRatchetEdit> {
        self.changes
            .iter()
            .filter(|c| c.mode == RatchetMode::Threshold)
            .map(|c| ThresholdRatchetEdit {
                bench: c.bench.clone(),
                metric: c.metric,
                new_threshold: c.new_value,
            })
            .collect()
    }

    pub fn receipt(self, compare_ref: CompareRef, tool: ToolInfo) -> RatchetReceipt {
        RatchetReceipt {
            schema: RATCHET_SCHEMA_V1.to_string(),
            tool,
            compare_ref,
            changes: self.changes,
        }
    }
}

pub struct RatchetUseCase;

impl RatchetUseCase {
    pub fn execute(req: RatchetRequest) -> RatchetResult {
        let mut out = RatchetResult::default();
        if !req.policy.enabled {
            out.skipped.push("ratchet policy is disabled".to_string());
            return out;
        }
        if req.compare.verdict.status != VerdictStatus::Pass {
            out.skipped.push("compare verdict is not pass".to_string());
            return out;
        }
        if req
            .compare
            .verdict
            .reasons
            .iter()
            .any(|r| r.contains("host_mismatch"))
        {
            out.skipped.push("host mismatch reason present".to_string());
            return out;
        }

        for metric in &req.policy.allow_metrics {
            let Some(delta) = req.compare.deltas.get(metric) else {
                continue;
            };
            let Some(budget) = req.compare.budgets.get(metric) else {
                continue;
            };

            if delta.status != MetricStatus::Pass {
                out.skipped.push(format!(
                    "{} skipped: metric status is not pass",
                    metric.as_str()
                ));
                continue;
            }

            let noisy = delta
                .cv
                .zip(delta.noise_threshold)
                .is_some_and(|(cv, threshold)| cv > threshold);
            if noisy {
                out.skipped.push(format!(
                    "{} skipped: noisy sample (cv>{})",
                    metric.as_str(),
                    delta.noise_threshold.unwrap_or_default()
                ));
                continue;
            }

            let significance_met = delta.significance.as_ref().map(|s| s.significant);
            if req.policy.require_significance && significance_met != Some(true) {
                out.skipped.push(format!(
                    "{} skipped: significance requirement not met",
                    metric.as_str()
                ));
                continue;
            }

            let improvement = match budget.direction {
                perfgate_types::Direction::Lower => {
                    (delta.baseline - delta.current) / delta.baseline
                }
                perfgate_types::Direction::Higher => {
                    (delta.current - delta.baseline) / delta.baseline
                }
            };

            if improvement < req.policy.min_improvement {
                out.skipped.push(format!(
                    "{} skipped: improvement {:.4} below minimum {:.4}",
                    metric.as_str(),
                    improvement,
                    req.policy.min_improvement
                ));
                continue;
            }

            match req.policy.mode {
                RatchetMode::Threshold => {
                    let Some(candidate) = tightened_threshold(
                        delta.baseline,
                        delta.current,
                        budget.threshold,
                        budget.direction,
                        req.policy.max_tightening,
                    ) else {
                        continue;
                    };
                    if candidate >= budget.threshold {
                        continue;
                    }
                    out.changes.push(RatchetChange {
                        bench: req.compare.bench.name.clone(),
                        metric: *metric,
                        mode: RatchetMode::Threshold,
                        old_value: budget.threshold,
                        new_value: candidate,
                        reason: format!(
                            "improved by {:.2}% (capped by max_tightening {:.2}%)",
                            improvement * 100.0,
                            req.policy.max_tightening * 100.0
                        ),
                        evidence: RatchetEvidence {
                            verdict_status: req.compare.verdict.status,
                            significance_required: req.policy.require_significance,
                            significance_met,
                            noisy,
                        },
                    });
                }
                RatchetMode::BaselineValue => {
                    if delta.current >= delta.baseline {
                        continue;
                    }
                    out.changes.push(RatchetChange {
                        bench: req.compare.bench.name.clone(),
                        metric: *metric,
                        mode: RatchetMode::BaselineValue,
                        old_value: delta.baseline,
                        new_value: delta.current,
                        reason: format!(
                            "baseline updated after {:.2}% improvement",
                            improvement * 100.0
                        ),
                        evidence: RatchetEvidence {
                            verdict_status: req.compare.verdict.status,
                            significance_required: req.policy.require_significance,
                            significance_met,
                            noisy,
                        },
                    });
                }
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use perfgate_types::{
        BenchMeta, Budget, CompareReceipt, CompareRef, Delta, Direction, Metric, MetricStatistic,
        Significance, SignificanceTest, ToolInfo, Verdict, VerdictCounts,
    };

    use super::*;

    fn fixture_compare() -> CompareReceipt {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.20,
                warn_threshold: 0.10,
                noise_threshold: Some(0.05),
                noise_policy: perfgate_types::NoisePolicy::Ignore,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 90.0,
                ratio: 0.9,
                pct: -0.1,
                regression: 0.0,
                cv: Some(0.01),
                noise_threshold: Some(0.05),
                statistic: MetricStatistic::Median,
                significance: Some(Significance {
                    test: SignificanceTest::WelchT,
                    p_value: Some(0.01),
                    alpha: 0.05,
                    significant: true,
                    baseline_samples: 10,
                    current_samples: 10,
                    ci_lower: None,
                    ci_upper: None,
                }),
                status: MetricStatus::Pass,
            },
        );

        CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.0.0".to_string(),
            },
            bench: BenchMeta {
                name: "bench-a".to_string(),
                cwd: None,
                command: vec!["echo".to_string()],
                repeat: 10,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baseline.json".to_string()),
                run_id: None,
            },
            current_ref: CompareRef {
                path: Some("current.json".to_string()),
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
    fn threshold_ratchet_is_capped() {
        let compare = fixture_compare();
        let result = RatchetUseCase::execute(RatchetRequest {
            compare,
            policy: RatchetPolicyConfig {
                enabled: true,
                mode: RatchetMode::Threshold,
                min_improvement: 0.05,
                max_tightening: 0.05,
                require_significance: true,
                allow_metrics: vec![Metric::WallMs],
            },
            compare_ref: CompareRef {
                path: None,
                run_id: None,
            },
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "x".into(),
            },
        });

        assert_eq!(result.changes.len(), 1);
        assert!((result.changes[0].new_value - 0.19).abs() < 1e-9);
    }
}
