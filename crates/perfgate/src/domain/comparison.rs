use std::collections::BTreeMap;

use perfgate_types::{
    Budget, Delta, Metric, MetricStatistic, MetricStatus, RunReceipt, Stats, TradeoffDowngrade,
    TradeoffRule, VERDICT_REASON_TRADEOFF_MISSING_REQUIRED_METRIC,
    VERDICT_REASON_TRADEOFF_RULE_NOT_SATISFIED, Verdict, VerdictCounts, VerdictStatus,
};

use super::{
    DomainError, compute_significance, evaluate_budget, metric_cv, metric_series_from_run,
    metric_value, metric_value_from_run, reason_token,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub deltas: BTreeMap<Metric, Delta>,
    pub verdict: Verdict,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SignificancePolicy {
    pub alpha: f64,
    pub min_samples: usize,
    pub require_significance: bool,
}

impl SignificancePolicy {
    pub fn new(
        alpha: f64,
        min_samples: usize,
        require_significance: bool,
    ) -> Result<Self, DomainError> {
        if !(0.0..=1.0).contains(&alpha) {
            return Err(DomainError::InvalidAlpha(alpha));
        }
        Ok(Self {
            alpha,
            min_samples,
            require_significance,
        })
    }
}

fn aggregate_verdict_from_counts(counts: VerdictCounts, reasons: Vec<String>) -> Verdict {
    let status = if counts.fail > 0 {
        VerdictStatus::Fail
    } else if counts.warn > 0 {
        VerdictStatus::Warn
    } else if counts.pass > 0 {
        VerdictStatus::Pass
    } else {
        VerdictStatus::Skip
    };

    Verdict {
        status,
        counts,
        reasons,
    }
}

/// Compare stats under the provided budgets.
///
/// Metrics without both baseline+current values are skipped (and therefore do not affect verdict).
///
/// # Examples
///
/// ```
/// use perfgate::domain::compare_stats;
/// use perfgate_types::*;
/// use std::collections::BTreeMap;
///
/// let baseline = Stats {
///     wall_ms: U64Summary::new(100, 90, 110 ),
///     cpu_ms: None, page_faults: None, ctx_switches: None,
///     max_rss_kb: None,
///     io_read_bytes: None, io_write_bytes: None, network_packets: None,
///     energy_uj: None,
///     binary_bytes: None, throughput_per_s: None,
/// };
/// let current = Stats {
///     wall_ms: U64Summary::new(105, 95, 115 ),
///     cpu_ms: None, page_faults: None, ctx_switches: None,
///     max_rss_kb: None,
///     io_read_bytes: None, io_write_bytes: None, network_packets: None,
///     energy_uj: None,
///     binary_bytes: None, throughput_per_s: None,
/// };
///
/// let mut budgets = BTreeMap::new();
/// budgets.insert(Metric::WallMs, Budget {
///     noise_threshold: None,
///     noise_policy: perfgate_types::NoisePolicy::Ignore,
///     threshold: 0.20, warn_threshold: 0.10, direction: Direction::Lower,
/// });
///
/// let cmp = compare_stats(&baseline, &current, &budgets).unwrap();
/// assert_eq!(cmp.verdict.status, VerdictStatus::Pass);
/// ```
#[must_use = "pure computation; call site should use the returned Comparison"]
pub fn compare_stats(
    baseline: &Stats,
    current: &Stats,
    budgets: &BTreeMap<Metric, Budget>,
) -> Result<Comparison, DomainError> {
    compare_stats_with_tradeoffs(baseline, current, budgets, &[])
}

/// Compare stats under the provided budgets and optional tradeoff rules.
#[must_use = "pure computation; call site should use the returned Comparison"]
pub fn compare_stats_with_tradeoffs(
    baseline: &Stats,
    current: &Stats,
    budgets: &BTreeMap<Metric, Budget>,
    tradeoffs: &[TradeoffRule],
) -> Result<Comparison, DomainError> {
    let mut deltas: BTreeMap<Metric, Delta> = BTreeMap::new();
    let mut reasons: Vec<String> = Vec::new();

    let mut counts = VerdictCounts {
        pass: 0,
        warn: 0,
        fail: 0,
        skip: 0,
    };

    for (metric, budget) in budgets {
        let b = metric_value(baseline, *metric);
        let c = metric_value(current, *metric);
        let current_cv = metric_cv(current, *metric);

        let (Some(bv), Some(cv)) = (b, c) else {
            continue;
        };

        if bv <= 0.0 {
            deltas.insert(
                *metric,
                Delta {
                    baseline: bv,
                    current: cv,
                    ratio: 1.0,
                    pct: 0.0,
                    regression: 0.0,
                    status: MetricStatus::Skip,
                    significance: None,
                    cv: current_cv,
                    noise_threshold: budget.noise_threshold,
                    statistic: MetricStatistic::Median,
                },
            );
            counts.skip += 1;
            continue;
        }

        let result = evaluate_budget(bv, cv, budget, current_cv)
            .expect("evaluate_budget is infallible for bv > 0");

        match result.status {
            MetricStatus::Pass => counts.pass += 1,
            MetricStatus::Warn => {
                counts.warn += 1;
                reasons.push(reason_token(*metric, MetricStatus::Warn));
            }
            MetricStatus::Fail => {
                counts.fail += 1;
                reasons.push(reason_token(*metric, MetricStatus::Fail));
            }
            MetricStatus::Skip => {
                counts.skip += 1;
                reasons.push(reason_token(*metric, MetricStatus::Skip));
            }
        }

        deltas.insert(
            *metric,
            Delta {
                baseline: result.baseline,
                current: result.current,
                ratio: result.ratio,
                pct: result.pct,
                regression: result.regression,
                cv: result.cv,
                noise_threshold: result.noise_threshold,
                statistic: MetricStatistic::Median,
                significance: None,
                status: result.status,
            },
        );
    }

    apply_tradeoffs(&mut deltas, &mut counts, &mut reasons, tradeoffs);
    let verdict = aggregate_verdict_from_counts(counts, reasons);

    Ok(Comparison { deltas, verdict })
}

/// Compare full run receipts under the provided budgets.
///
/// This variant supports:
/// - Per-metric statistic selection (`median` or `p95`)
/// - Optional significance analysis with Welch's t-test
pub fn compare_runs(
    baseline: &RunReceipt,
    current: &RunReceipt,
    budgets: &BTreeMap<Metric, Budget>,
    metric_statistics: &BTreeMap<Metric, MetricStatistic>,
    significance_policy: Option<SignificancePolicy>,
) -> Result<Comparison, DomainError> {
    compare_runs_with_tradeoffs(
        baseline,
        current,
        budgets,
        metric_statistics,
        significance_policy,
        &[],
    )
}

/// Compare full run receipts under budgets with optional tradeoff rules.
pub fn compare_runs_with_tradeoffs(
    baseline: &RunReceipt,
    current: &RunReceipt,
    budgets: &BTreeMap<Metric, Budget>,
    metric_statistics: &BTreeMap<Metric, MetricStatistic>,
    significance_policy: Option<SignificancePolicy>,
    tradeoffs: &[TradeoffRule],
) -> Result<Comparison, DomainError> {
    let mut deltas: BTreeMap<Metric, Delta> = BTreeMap::new();
    let mut reasons: Vec<String> = Vec::new();

    let mut counts = VerdictCounts {
        pass: 0,
        warn: 0,
        fail: 0,
        skip: 0,
    };

    for (metric, budget) in budgets {
        let statistic = metric_statistics
            .get(metric)
            .copied()
            .unwrap_or(MetricStatistic::Median);

        let b = metric_value_from_run(baseline, *metric, statistic);
        let c = metric_value_from_run(current, *metric, statistic);
        let current_cv = metric_cv(&current.stats, *metric);

        let (Some(bv), Some(cv)) = (b, c) else {
            continue;
        };

        if bv <= 0.0 {
            deltas.insert(
                *metric,
                Delta {
                    baseline: bv,
                    current: cv,
                    ratio: 1.0,
                    pct: 0.0,
                    regression: 0.0,
                    status: MetricStatus::Skip,
                    significance: None,
                    cv: current_cv,
                    noise_threshold: budget.noise_threshold,
                    statistic,
                },
            );
            counts.skip += 1;
            continue;
        }

        let result = evaluate_budget(bv, cv, budget, current_cv)
            .expect("evaluate_budget is infallible for bv > 0");

        let mut status = result.status;

        let significance = significance_policy.and_then(|policy| {
            let baseline_series = metric_series_from_run(baseline, *metric);
            let current_series = metric_series_from_run(current, *metric);
            compute_significance(
                &baseline_series,
                &current_series,
                policy.alpha,
                policy.min_samples,
            )
        });

        if let Some(policy) = significance_policy
            && policy.require_significance
            && matches!(status, MetricStatus::Warn | MetricStatus::Fail)
        {
            let is_significant = significance
                .as_ref()
                .map(|sig| sig.significant)
                .unwrap_or(false);
            if !is_significant {
                status = MetricStatus::Pass;
            }
        }

        match status {
            MetricStatus::Pass => counts.pass += 1,
            MetricStatus::Warn => {
                counts.warn += 1;
                reasons.push(reason_token(*metric, MetricStatus::Warn));
            }
            MetricStatus::Fail => {
                counts.fail += 1;
                reasons.push(reason_token(*metric, MetricStatus::Fail));
            }
            MetricStatus::Skip => {
                counts.skip += 1;
                reasons.push(reason_token(*metric, MetricStatus::Skip));
            }
        }

        deltas.insert(
            *metric,
            Delta {
                baseline: result.baseline,
                current: result.current,
                ratio: result.ratio,
                pct: result.pct,
                regression: result.regression,
                cv: result.cv,
                noise_threshold: result.noise_threshold,
                statistic,
                significance,
                status,
            },
        );
    }

    apply_tradeoffs(&mut deltas, &mut counts, &mut reasons, tradeoffs);
    let verdict = aggregate_verdict_from_counts(counts, reasons);

    Ok(Comparison { deltas, verdict })
}

fn push_unique_reason(reasons: &mut Vec<String>, token: String) {
    if !reasons.contains(&token) {
        reasons.push(token);
    }
}

fn remove_reason(reasons: &mut Vec<String>, token: &str) {
    reasons.retain(|reason| reason != token);
}

fn improvement_ratio(delta: &Delta, metric: Metric) -> Option<f64> {
    match metric.default_direction() {
        perfgate_types::Direction::Higher => Some(delta.ratio),
        perfgate_types::Direction::Lower => Some(if delta.current <= 0.0 {
            f64::INFINITY
        } else {
            delta.baseline / delta.current
        }),
    }
}

fn apply_tradeoffs(
    deltas: &mut BTreeMap<Metric, Delta>,
    counts: &mut VerdictCounts,
    reasons: &mut Vec<String>,
    tradeoffs: &[TradeoffRule],
) {
    let failed_metrics: Vec<Metric> = deltas
        .iter()
        .filter_map(|(metric, delta)| (delta.status == MetricStatus::Fail).then_some(*metric))
        .collect();

    for failed_metric in failed_metrics {
        let mut applied = false;
        let mut saw_missing_required_metric = false;
        let mut saw_unsatisfied_rule = false;

        for rule in tradeoffs
            .iter()
            .filter(|rule| rule.if_failed == failed_metric)
        {
            let mut missing_required_metric = false;
            let mut satisfied = !rule.require.is_empty();

            for requirement in &rule.require {
                if requirement.probe.is_some() {
                    missing_required_metric = true;
                    satisfied = false;
                    break;
                }

                let Some(required_delta) = deltas.get(&requirement.metric) else {
                    missing_required_metric = true;
                    satisfied = false;
                    break;
                };

                let Some(ratio) = improvement_ratio(required_delta, requirement.metric) else {
                    satisfied = false;
                    break;
                };

                if ratio < requirement.min_improvement_ratio {
                    satisfied = false;
                }
            }

            if !rule.allow.is_empty() {
                missing_required_metric = true;
                satisfied = false;
            }

            if !satisfied {
                saw_missing_required_metric |= missing_required_metric;
                saw_unsatisfied_rule = true;
                continue;
            }

            if let Some(delta) = deltas.get_mut(&failed_metric) {
                let new_status = match rule.downgrade_to {
                    TradeoffDowngrade::Warn => MetricStatus::Warn,
                    TradeoffDowngrade::Pass => MetricStatus::Pass,
                };
                if delta.status == MetricStatus::Fail && new_status != MetricStatus::Fail {
                    counts.fail = counts.fail.saturating_sub(1);
                    match new_status {
                        MetricStatus::Warn => counts.warn += 1,
                        MetricStatus::Pass => counts.pass += 1,
                        MetricStatus::Fail | MetricStatus::Skip => {}
                    }
                    delta.status = new_status;
                    remove_reason(reasons, &reason_token(failed_metric, MetricStatus::Fail));
                    if new_status == MetricStatus::Warn {
                        push_unique_reason(
                            reasons,
                            reason_token(failed_metric, MetricStatus::Warn),
                        );
                    }
                }
            }

            push_unique_reason(reasons, format!("tradeoff_{}_applied", rule.name));
            applied = true;
            break;
        }

        if !applied {
            if saw_missing_required_metric {
                push_unique_reason(
                    reasons,
                    VERDICT_REASON_TRADEOFF_MISSING_REQUIRED_METRIC.to_string(),
                );
            }
            if saw_unsatisfied_rule {
                push_unique_reason(
                    reasons,
                    VERDICT_REASON_TRADEOFF_RULE_NOT_SATISFIED.to_string(),
                );
            }
        }
    }
}
