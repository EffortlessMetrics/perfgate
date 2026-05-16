use crate::domain::budget::{aggregate_verdict, reason_token};
use crate::domain::improvement_ratio;
use perfgate_types::{
    DecisionPolicyConfig, Delta, HostInfo, Metric, MetricStatus, MissingNoisePolicy,
    PROBE_COMPARE_SCHEMA_V1, ProbeCompareObservation, ProbeCompareReceipt, RunMeta,
    ScenarioReceipt, TRADEOFF_SCHEMA_V1, ToolInfo, TradeoffAllowance, TradeoffAllowanceOutcome,
    TradeoffDecision, TradeoffDecisionStatus, TradeoffDowngrade, TradeoffProbeOutcome,
    TradeoffReceipt, TradeoffRequirement, TradeoffRequirementOutcome, TradeoffRule,
    TradeoffRuleOutcome, VERDICT_REASON_TRADEOFF_MISSING_REQUIRED_METRIC,
    VERDICT_REASON_TRADEOFF_REVIEW_REQUIRED, VERDICT_REASON_TRADEOFF_RULE_NOT_SATISFIED, Verdict,
};
use std::collections::{BTreeMap, BTreeSet};
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug)]
pub struct TradeoffEvaluateRequest {
    pub scenario: ScenarioReceipt,
    pub probe_compares: Vec<ProbeCompareReceipt>,
    pub rules: Vec<TradeoffRule>,
    pub decision_policy: DecisionPolicyConfig,
    pub tool: ToolInfo,
}

#[derive(Debug)]
pub struct TradeoffEvaluateOutcome {
    pub receipt: TradeoffReceipt,
}

pub struct TradeoffUseCase;

impl TradeoffUseCase {
    pub fn evaluate(req: TradeoffEvaluateRequest) -> anyhow::Result<TradeoffEvaluateOutcome> {
        if req.rules.is_empty() {
            anyhow::bail!("no tradeoff rules provided");
        }
        for probe_compare in &req.probe_compares {
            if probe_compare.schema != PROBE_COMPARE_SCHEMA_V1 {
                anyhow::bail!(
                    "probe compare receipt must use schema '{}', got '{}'",
                    PROBE_COMPARE_SCHEMA_V1,
                    probe_compare.schema
                );
            }
        }

        let mut weighted_deltas = req.scenario.weighted_deltas.clone();
        let (probe_index, probe_warnings) = index_probe_compares(&req.probe_compares);
        let mut rule_outcomes = Vec::new();
        let mut accepted_reasons = Vec::new();
        let mut rejected_reasons = Vec::new();
        let mut accepted_metrics = BTreeSet::new();
        let mut review_candidates = Vec::new();
        let mut review_reasons = Vec::new();

        for rule in &req.rules {
            let outcome = evaluate_rule(rule, &weighted_deltas, &probe_index, &req.decision_policy);
            if outcome.accepted {
                if let Some(delta) = weighted_deltas.get_mut(rule.if_failed.as_str()) {
                    delta.status = downgrade_status(rule.downgrade_to);
                }
                accepted_metrics.insert(rule.if_failed);
                accepted_reasons.push(format!("tradeoff_{}_applied", rule.name));
            } else if matches!(outcome.status, TradeoffDecisionStatus::NeedsReview) {
                review_candidates.push((
                    rule.if_failed,
                    format!(
                        "tradeoff '{}' requires review: {}",
                        rule.name,
                        outcome
                            .reason
                            .as_deref()
                            .unwrap_or("evidence is incomplete")
                    ),
                ));
            } else if matches!(outcome.status, TradeoffDecisionStatus::Rejected) {
                if outcome
                    .requirements
                    .iter()
                    .any(|requirement| requirement.observed_change.is_none())
                    || outcome
                        .allowances
                        .iter()
                        .any(|allowance| allowance.observed_regression.is_none())
                {
                    rejected_reasons
                        .push(VERDICT_REASON_TRADEOFF_MISSING_REQUIRED_METRIC.to_string());
                } else {
                    rejected_reasons.push(VERDICT_REASON_TRADEOFF_RULE_NOT_SATISFIED.to_string());
                }
            }
            rule_outcomes.push(outcome);
        }

        for (metric, reason) in review_candidates {
            if accepted_metrics.contains(&metric) {
                continue;
            }
            if let Some(delta) = weighted_deltas.get_mut(metric.as_str()) {
                delta.status = MetricStatus::Warn;
            }
            push_unique(&mut review_reasons, reason);
        }
        let probes = tradeoff_probe_outcomes(&probe_index);

        let mut verdict = verdict_from_weighted_deltas(&weighted_deltas);
        verdict
            .reasons
            .extend(non_pass_reason_tokens(&weighted_deltas));
        if !review_reasons.is_empty() {
            push_unique(
                &mut verdict.reasons,
                VERDICT_REASON_TRADEOFF_REVIEW_REQUIRED.to_string(),
            );
        }
        for reason in accepted_reasons.iter().chain(rejected_reasons.iter()) {
            push_unique(&mut verdict.reasons, reason.clone());
        }

        let accepted = rule_outcomes.iter().any(|outcome| outcome.accepted);
        let review_required = !review_reasons.is_empty();
        let decision = TradeoffDecision {
            accepted_tradeoff: accepted,
            review_required,
            review_reasons,
            status: metric_status_from_verdict(&verdict),
            reason: decision_reason(accepted, review_required, &rule_outcomes, &verdict),
        };

        let mut warnings = req.scenario.warnings;
        for warning in probe_warnings {
            push_unique(&mut warnings, warning);
        }

        let receipt = TradeoffReceipt {
            schema: TRADEOFF_SCHEMA_V1.to_string(),
            tool: req.tool,
            run: make_run_meta(),
            scenario: Some(req.scenario.scenario.name),
            baseline_ref: req.scenario.baseline_ref,
            current_ref: req.scenario.current_ref,
            configured_rules: req.rules,
            rules: rule_outcomes,
            probes,
            weighted_deltas,
            decision,
            verdict,
            warnings,
        };

        Ok(TradeoffEvaluateOutcome { receipt })
    }
}

fn evaluate_rule(
    rule: &TradeoffRule,
    weighted_deltas: &BTreeMap<String, Delta>,
    probe_index: &BTreeMap<String, ProbeCompareObservation>,
    decision_policy: &DecisionPolicyConfig,
) -> TradeoffRuleOutcome {
    let Some(target) = weighted_deltas.get(rule.if_failed.as_str()) else {
        return TradeoffRuleOutcome {
            name: rule.name.clone(),
            status: TradeoffDecisionStatus::NotEvaluated,
            accepted: false,
            downgrade_to: None,
            reason: Some(format!(
                "failed metric '{}' is not present",
                rule.if_failed.as_str()
            )),
            requirements: evaluate_requirements(&rule.require, weighted_deltas, probe_index),
            allowances: evaluate_allowances(&rule.allow, probe_index),
        };
    };

    if target.status != MetricStatus::Fail {
        return TradeoffRuleOutcome {
            name: rule.name.clone(),
            status: TradeoffDecisionStatus::NotEvaluated,
            accepted: false,
            downgrade_to: None,
            reason: Some(format!(
                "metric '{}' is not failing",
                rule.if_failed.as_str()
            )),
            requirements: evaluate_requirements(&rule.require, weighted_deltas, probe_index),
            allowances: evaluate_allowances(&rule.allow, probe_index),
        };
    }

    let requirements = evaluate_requirements(&rule.require, weighted_deltas, probe_index);
    let allowances = evaluate_allowances(&rule.allow, probe_index);
    let accepted_by_tradeoff_evidence = !requirements.is_empty()
        && requirements.iter().all(|requirement| requirement.satisfied)
        && allowances.iter().all(|allowance| allowance.satisfied);
    let noise_review_reason = accepted_by_tradeoff_evidence
        .then(|| noise_review_reason(rule, weighted_deltas, probe_index, decision_policy))
        .flatten();
    let accepted = accepted_by_tradeoff_evidence && noise_review_reason.is_none();
    let needs_review = noise_review_reason.is_some()
        || (!accepted && evidence_incomplete_but_satisfied(&requirements, &allowances));

    TradeoffRuleOutcome {
        name: rule.name.clone(),
        status: if accepted {
            TradeoffDecisionStatus::Accepted
        } else if needs_review {
            TradeoffDecisionStatus::NeedsReview
        } else {
            TradeoffDecisionStatus::Rejected
        },
        accepted,
        downgrade_to: accepted.then_some(rule.downgrade_to),
        reason: Some(if accepted {
            if allowances.is_empty() {
                "all required compensating improvements were satisfied".to_string()
            } else {
                "all required compensating improvements and local regression caps were satisfied"
                    .to_string()
            }
        } else if needs_review {
            noise_review_reason.unwrap_or_else(|| {
                "required tradeoff evidence is incomplete; review required".to_string()
            })
        } else {
            "one or more required compensating improvements or local regression caps were not satisfied".to_string()
        }),
        requirements,
        allowances,
    }
}

fn noise_review_reason(
    rule: &TradeoffRule,
    weighted_deltas: &BTreeMap<String, Delta>,
    probe_index: &BTreeMap<String, ProbeCompareObservation>,
    decision_policy: &DecisionPolicyConfig,
) -> Option<String> {
    if !decision_policy.require_low_noise_for_acceptance {
        return None;
    }
    let max_cv = decision_policy.max_cv?;
    let mut missing = Vec::new();
    let mut noisy = Vec::new();

    for requirement in &rule.require {
        let label = evidence_label(requirement.probe.as_deref(), requirement.metric.as_str());
        let delta = if let Some(probe) = requirement.probe.as_deref() {
            probe_index
                .get(probe)
                .and_then(|probe| probe.deltas.get(requirement.metric.as_str()))
        } else {
            weighted_deltas.get(requirement.metric.as_str())
        };
        collect_noise_evidence(&label, delta, max_cv, &mut missing, &mut noisy);
    }

    for allowance in &rule.allow {
        let label = evidence_label(Some(&allowance.probe), allowance.metric.as_str());
        let delta = probe_index
            .get(&allowance.probe)
            .and_then(|probe| probe.deltas.get(allowance.metric.as_str()));
        collect_noise_evidence(&label, delta, max_cv, &mut missing, &mut noisy);
    }

    if !noisy.is_empty() {
        return Some(format!(
            "tradeoff evidence exceeds max_cv {:.3}: {}",
            max_cv,
            noisy.join(", ")
        ));
    }

    if !missing.is_empty()
        && matches!(
            decision_policy.missing_noise,
            MissingNoisePolicy::NeedsReview
        )
    {
        return Some(format!(
            "tradeoff noise evidence missing for {}",
            missing.join(", ")
        ));
    }

    None
}

fn evidence_label(probe: Option<&str>, metric: &str) -> String {
    if let Some(probe) = probe {
        format!("probe '{probe}' metric '{metric}'")
    } else {
        format!("metric '{metric}'")
    }
}

fn collect_noise_evidence(
    label: &str,
    delta: Option<&Delta>,
    max_cv: f64,
    missing: &mut Vec<String>,
    noisy: &mut Vec<String>,
) {
    let Some(delta) = delta else {
        missing.push(label.to_string());
        return;
    };
    let Some(cv) = delta.cv else {
        missing.push(label.to_string());
        return;
    };
    if cv > max_cv + f64::EPSILON {
        noisy.push(format!("{label} cv {:.3}", cv));
    }
}

fn evidence_incomplete_but_satisfied(
    requirements: &[TradeoffRequirementOutcome],
    allowances: &[TradeoffAllowanceOutcome],
) -> bool {
    let missing_review_evidence = requirements
        .iter()
        .any(|requirement| requirement.probe.is_some() && requirement.observed_change.is_none())
        || allowances
            .iter()
            .any(|allowance| allowance.observed_regression.is_none());
    if !missing_review_evidence {
        return false;
    }

    let requirements_satisfied_or_missing_review_evidence =
        requirements.iter().all(|requirement| {
            requirement.satisfied
                || (requirement.probe.is_some() && requirement.observed_change.is_none())
        });
    let allowances_satisfied_or_missing = allowances
        .iter()
        .all(|allowance| allowance.satisfied || allowance.observed_regression.is_none());

    requirements_satisfied_or_missing_review_evidence && allowances_satisfied_or_missing
}

fn evaluate_requirements(
    requirements: &[TradeoffRequirement],
    weighted_deltas: &BTreeMap<String, Delta>,
    probe_index: &BTreeMap<String, ProbeCompareObservation>,
) -> Vec<TradeoffRequirementOutcome> {
    requirements
        .iter()
        .map(|requirement| {
            if requirement.probe.is_some() {
                return evaluate_probe_requirement(requirement, probe_index);
            }

            let metric_key = requirement.metric.as_str();
            let Some(delta) = weighted_deltas.get(metric_key) else {
                return TradeoffRequirementOutcome {
                    metric: metric_key.to_string(),
                    probe: requirement.probe.clone(),
                    required_change: required_change(requirement),
                    observed_change: None,
                    satisfied: false,
                    status: MetricStatus::Fail,
                    reason: Some("required metric missing".to_string()),
                };
            };

            let observed_ratio = improvement_ratio(requirement.metric, delta);
            let satisfied = observed_ratio
                .map(|ratio| ratio >= requirement.min_improvement_ratio)
                .unwrap_or(false);

            TradeoffRequirementOutcome {
                metric: metric_key.to_string(),
                probe: requirement.probe.clone(),
                required_change: required_change(requirement),
                observed_change: Some(delta.pct),
                satisfied,
                status: if satisfied {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Fail
                },
                reason: (!satisfied).then_some(format!(
                    "requires improvement ratio >= {:.6}",
                    requirement.min_improvement_ratio
                )),
            }
        })
        .collect()
}

fn evaluate_allowances(
    allowances: &[TradeoffAllowance],
    probe_index: &BTreeMap<String, ProbeCompareObservation>,
) -> Vec<TradeoffAllowanceOutcome> {
    allowances
        .iter()
        .map(|allowance| {
            let metric_key = allowance.metric.as_str();
            let Some(probe) = probe_index.get(&allowance.probe) else {
                return TradeoffAllowanceOutcome {
                    metric: metric_key.to_string(),
                    probe: allowance.probe.clone(),
                    max_regression: allowance.max_regression,
                    observed_regression: None,
                    satisfied: false,
                    status: MetricStatus::Fail,
                    reason: Some(format!("allowed probe '{}' missing", allowance.probe)),
                };
            };

            let Some(delta) = probe.deltas.get(metric_key) else {
                return TradeoffAllowanceOutcome {
                    metric: metric_key.to_string(),
                    probe: allowance.probe.clone(),
                    max_regression: allowance.max_regression,
                    observed_regression: None,
                    satisfied: false,
                    status: MetricStatus::Fail,
                    reason: Some(format!(
                        "allowed probe '{}' metric '{metric_key}' missing",
                        allowance.probe
                    )),
                };
            };

            let observed_regression = delta.regression;
            let satisfied = observed_regression <= allowance.max_regression + f64::EPSILON;

            TradeoffAllowanceOutcome {
                metric: metric_key.to_string(),
                probe: allowance.probe.clone(),
                max_regression: allowance.max_regression,
                observed_regression: Some(observed_regression),
                satisfied,
                status: if satisfied {
                    MetricStatus::Pass
                } else {
                    MetricStatus::Fail
                },
                reason: (!satisfied).then_some(format!(
                    "regression {:.6} exceeds cap {:.6}",
                    observed_regression, allowance.max_regression
                )),
            }
        })
        .collect()
}

fn evaluate_probe_requirement(
    requirement: &TradeoffRequirement,
    probe_index: &BTreeMap<String, ProbeCompareObservation>,
) -> TradeoffRequirementOutcome {
    let metric_key = requirement.metric.as_str();
    let Some(probe_name) = requirement.probe.as_deref() else {
        return TradeoffRequirementOutcome {
            metric: metric_key.to_string(),
            probe: None,
            required_change: required_change(requirement),
            observed_change: None,
            satisfied: false,
            status: MetricStatus::Fail,
            reason: Some("required probe missing".to_string()),
        };
    };

    let Some(probe) = probe_index.get(probe_name) else {
        return TradeoffRequirementOutcome {
            metric: metric_key.to_string(),
            probe: Some(probe_name.to_string()),
            required_change: required_change(requirement),
            observed_change: None,
            satisfied: false,
            status: MetricStatus::Fail,
            reason: Some(format!("required probe '{probe_name}' missing")),
        };
    };

    let Some(delta) = probe.deltas.get(metric_key) else {
        return TradeoffRequirementOutcome {
            metric: metric_key.to_string(),
            probe: Some(probe_name.to_string()),
            required_change: required_change(requirement),
            observed_change: None,
            satisfied: false,
            status: MetricStatus::Fail,
            reason: Some(format!(
                "required probe '{probe_name}' metric '{metric_key}' missing"
            )),
        };
    };

    let observed_ratio = improvement_ratio(requirement.metric, delta);
    let satisfied = observed_ratio
        .map(|ratio| ratio >= requirement.min_improvement_ratio)
        .unwrap_or(false);

    TradeoffRequirementOutcome {
        metric: metric_key.to_string(),
        probe: Some(probe_name.to_string()),
        required_change: required_change(requirement),
        observed_change: Some(delta.pct),
        satisfied,
        status: if satisfied {
            MetricStatus::Pass
        } else {
            MetricStatus::Fail
        },
        reason: (!satisfied).then_some(format!(
            "requires improvement ratio >= {:.6}",
            requirement.min_improvement_ratio
        )),
    }
}

fn index_probe_compares(
    probe_compares: &[ProbeCompareReceipt],
) -> (BTreeMap<String, ProbeCompareObservation>, Vec<String>) {
    let mut index = BTreeMap::new();
    let mut warnings = Vec::new();
    for receipt in probe_compares {
        warnings.extend(
            receipt
                .warnings
                .iter()
                .map(|warning| format!("probe compare warning: {warning}")),
        );
        for probe in &receipt.probes {
            if index.insert(probe.name.clone(), probe.clone()).is_some() {
                warnings.push(format!(
                    "probe '{}' appeared in more than one probe compare receipt; last value used",
                    probe.name
                ));
            }
        }
    }
    (index, warnings)
}

fn tradeoff_probe_outcomes(
    probe_index: &BTreeMap<String, ProbeCompareObservation>,
) -> Vec<TradeoffProbeOutcome> {
    probe_index
        .values()
        .map(|probe| TradeoffProbeOutcome {
            name: probe.name.clone(),
            scope: probe.scope,
            weight: None,
            deltas: probe.deltas.clone(),
            status: probe.status,
            reason: probe.reasons.first().cloned(),
        })
        .collect()
}

fn required_change(requirement: &TradeoffRequirement) -> f64 {
    match requirement.metric.default_direction() {
        perfgate_types::Direction::Higher => requirement.min_improvement_ratio - 1.0,
        perfgate_types::Direction::Lower => (1.0 / requirement.min_improvement_ratio) - 1.0,
    }
}

fn downgrade_status(downgrade: TradeoffDowngrade) -> MetricStatus {
    match downgrade {
        TradeoffDowngrade::Warn => MetricStatus::Warn,
        TradeoffDowngrade::Pass => MetricStatus::Pass,
    }
}

fn verdict_from_weighted_deltas(weighted_deltas: &BTreeMap<String, Delta>) -> Verdict {
    let statuses: Vec<_> = weighted_deltas.values().map(|delta| delta.status).collect();
    aggregate_verdict(&statuses)
}

fn non_pass_reason_tokens(weighted_deltas: &BTreeMap<String, Delta>) -> Vec<String> {
    weighted_deltas
        .iter()
        .filter_map(|(metric_key, delta)| {
            if matches!(delta.status, MetricStatus::Pass) {
                return None;
            }
            Metric::parse_key(metric_key).map(|metric| reason_token(metric, delta.status))
        })
        .collect()
}

fn metric_status_from_verdict(verdict: &Verdict) -> MetricStatus {
    match verdict.status {
        perfgate_types::VerdictStatus::Pass => MetricStatus::Pass,
        perfgate_types::VerdictStatus::Warn => MetricStatus::Warn,
        perfgate_types::VerdictStatus::Fail => MetricStatus::Fail,
        perfgate_types::VerdictStatus::Skip => MetricStatus::Skip,
    }
}

fn decision_reason(
    accepted: bool,
    review_required: bool,
    rule_outcomes: &[TradeoffRuleOutcome],
    verdict: &Verdict,
) -> String {
    if accepted && let Some(rule) = rule_outcomes.iter().find(|outcome| outcome.accepted) {
        return format!("tradeoff '{}' accepted", rule.name);
    }

    if review_required
        && let Some(rule) = rule_outcomes
            .iter()
            .find(|outcome| matches!(outcome.status, TradeoffDecisionStatus::NeedsReview))
    {
        return format!("tradeoff '{}' requires review", rule.name);
    }

    if matches!(verdict.status, perfgate_types::VerdictStatus::Fail) {
        "no configured tradeoff accepted the failing metric".to_string()
    } else {
        "no failing metric required a tradeoff decision".to_string()
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn make_run_meta() -> RunMeta {
    let now = OffsetDateTime::now_utc();
    let timestamp = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    RunMeta {
        id: Uuid::new_v4().to_string(),
        started_at: timestamp.clone(),
        ended_at: timestamp,
        host: HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        PROBE_COMPARE_SCHEMA_V1, ProbeCompareObservation, ProbeCompareReceipt, ProbeScope,
        SCENARIO_SCHEMA_V1, ScenarioMeta, VerdictCounts, VerdictStatus,
    };

    fn scenario_receipt(wall_current: f64, memory_status: MetricStatus) -> ScenarioReceipt {
        ScenarioReceipt {
            schema: SCENARIO_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
            run: make_run_meta(),
            scenario: ScenarioMeta {
                name: "release_workload".to_string(),
                weight: 1.0,
                description: None,
                command: None,
            },
            baseline_ref: None,
            current_ref: None,
            components: Vec::new(),
            weighted_deltas: BTreeMap::from([
                (
                    "wall_ms".to_string(),
                    delta(100.0, wall_current, MetricStatus::Pass),
                ),
                ("max_rss_kb".to_string(), delta(100.0, 120.0, memory_status)),
            ]),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 1,
                    skip: 0,
                },
                reasons: vec!["max_rss_kb_fail".to_string()],
            },
            warnings: Vec::new(),
        }
    }

    fn delta(baseline: f64, current: f64, status: MetricStatus) -> Delta {
        Delta {
            baseline,
            current,
            ratio: current / baseline,
            pct: (current - baseline) / baseline,
            regression: if current > baseline {
                (current - baseline) / baseline
            } else {
                0.0
            },
            cv: None,
            noise_threshold: None,
            statistic: perfgate_types::MetricStatistic::Median,
            significance: None,
            status,
        }
    }

    fn delta_with_cv(baseline: f64, current: f64, status: MetricStatus, cv: Option<f64>) -> Delta {
        Delta {
            cv,
            ..delta(baseline, current, status)
        }
    }

    fn low_noise_policy(max_cv: f64) -> DecisionPolicyConfig {
        DecisionPolicyConfig {
            require_low_noise_for_acceptance: true,
            max_cv: Some(max_cv),
            missing_noise: MissingNoisePolicy::NeedsReview,
        }
    }

    fn memory_for_speed_rule(downgrade_to: TradeoffDowngrade) -> TradeoffRule {
        TradeoffRule {
            name: "memory_for_speed".to_string(),
            if_failed: Metric::MaxRssKb,
            require: vec![TradeoffRequirement {
                metric: Metric::WallMs,
                probe: None,
                min_improvement_ratio: 1.10,
            }],
            allow: Vec::new(),
            downgrade_to,
        }
    }

    fn memory_for_probe_speed_rule(downgrade_to: TradeoffDowngrade) -> TradeoffRule {
        TradeoffRule {
            name: "memory_for_probe_speed".to_string(),
            if_failed: Metric::MaxRssKb,
            require: vec![TradeoffRequirement {
                metric: Metric::WallMs,
                probe: Some("parser.batch_loop".to_string()),
                min_improvement_ratio: 1.10,
            }],
            allow: Vec::new(),
            downgrade_to,
        }
    }

    fn memory_for_probe_speed_rule_with_allow(
        downgrade_to: TradeoffDowngrade,
        max_regression: f64,
    ) -> TradeoffRule {
        TradeoffRule {
            name: "memory_for_probe_speed".to_string(),
            if_failed: Metric::MaxRssKb,
            require: vec![TradeoffRequirement {
                metric: Metric::WallMs,
                probe: Some("parser.batch_loop".to_string()),
                min_improvement_ratio: 1.10,
            }],
            allow: vec![TradeoffAllowance {
                metric: Metric::WallMs,
                probe: "parser.tokenize".to_string(),
                max_regression,
            }],
            downgrade_to,
        }
    }

    fn tradeoff_rule_for_requirement(
        if_failed: Metric,
        required_metric: Metric,
        downgrade_to: TradeoffDowngrade,
    ) -> TradeoffRule {
        TradeoffRule {
            name: "directional_tradeoff".to_string(),
            if_failed,
            require: vec![TradeoffRequirement {
                metric: required_metric,
                probe: None,
                min_improvement_ratio: 1.10,
            }],
            allow: Vec::new(),
            downgrade_to,
        }
    }

    fn tradeoff_rule_for_probe_requirement(
        if_failed: Metric,
        required_metric: Metric,
        probe: &str,
        downgrade_to: TradeoffDowngrade,
    ) -> TradeoffRule {
        TradeoffRule {
            name: "probe_directional_tradeoff".to_string(),
            if_failed,
            require: vec![TradeoffRequirement {
                metric: required_metric,
                probe: Some(probe.to_string()),
                min_improvement_ratio: 1.10,
            }],
            allow: Vec::new(),
            downgrade_to,
        }
    }

    fn tradeoff_rule_for_probe_requirement_with_allow(
        if_failed: Metric,
        required_metric: Metric,
        required_probe: &str,
        allowed_metric: Metric,
        allowed_probe: &str,
        max_regression: f64,
        downgrade_to: TradeoffDowngrade,
    ) -> TradeoffRule {
        TradeoffRule {
            name: "probe_directional_tradeoff_with_allow".to_string(),
            if_failed,
            require: vec![TradeoffRequirement {
                metric: required_metric,
                probe: Some(required_probe.to_string()),
                min_improvement_ratio: 1.10,
            }],
            allow: vec![TradeoffAllowance {
                metric: allowed_metric,
                probe: allowed_probe.to_string(),
                max_regression,
            }],
            downgrade_to,
        }
    }

    fn probe_compare_receipt(probe_name: &str, wall_current: f64) -> ProbeCompareReceipt {
        probe_compare_receipt_many_with_cv(&[(
            probe_name,
            wall_current,
            ProbeScope::Dominant,
            None,
        )])
    }

    fn probe_compare_receipt_many(probes: &[(&str, f64, ProbeScope)]) -> ProbeCompareReceipt {
        let probes: Vec<_> = probes
            .iter()
            .map(|(name, current, scope)| (*name, *current, *scope, None))
            .collect();
        probe_compare_receipt_many_with_cv(&probes)
    }

    fn probe_compare_receipt_many_with_cv(
        probes: &[(&str, f64, ProbeScope, Option<f64>)],
    ) -> ProbeCompareReceipt {
        ProbeCompareReceipt {
            schema: PROBE_COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
            run: make_run_meta(),
            bench: None,
            scenario: Some("release_workload".to_string()),
            baseline_ref: None,
            current_ref: None,
            probes: probes
                .iter()
                .map(
                    |(probe_name, wall_current, scope, cv)| ProbeCompareObservation {
                        name: (*probe_name).to_string(),
                        parent: None,
                        scope: Some(*scope),
                        baseline_count: 1,
                        current_count: 1,
                        deltas: BTreeMap::from([(
                            "wall_ms".to_string(),
                            delta_with_cv(100.0, *wall_current, MetricStatus::Pass, *cv),
                        )]),
                        status: MetricStatus::Pass,
                        reasons: Vec::new(),
                    },
                )
                .collect(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    fn scenario_receipt_with_metric_deltas(
        deltas: Vec<(Metric, Delta)>,
        failed_metric: Metric,
    ) -> ScenarioReceipt {
        let weighted_deltas: BTreeMap<String, Delta> = deltas
            .into_iter()
            .map(|(metric, delta)| (metric.as_str().to_string(), delta))
            .collect();
        let fail_count = weighted_deltas
            .values()
            .filter(|delta| delta.status == MetricStatus::Fail)
            .count() as u32;
        let warn_count = weighted_deltas
            .values()
            .filter(|delta| delta.status == MetricStatus::Warn)
            .count() as u32;
        let pass_count = weighted_deltas
            .values()
            .filter(|delta| delta.status == MetricStatus::Pass)
            .count() as u32;

        ScenarioReceipt {
            schema: SCENARIO_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
            run: make_run_meta(),
            scenario: ScenarioMeta {
                name: "release_workload".to_string(),
                weight: 1.0,
                description: None,
                command: None,
            },
            baseline_ref: None,
            current_ref: None,
            components: Vec::new(),
            weighted_deltas,
            verdict: Verdict {
                status: if fail_count > 0 {
                    VerdictStatus::Fail
                } else if warn_count > 0 {
                    VerdictStatus::Warn
                } else {
                    VerdictStatus::Pass
                },
                counts: VerdictCounts {
                    pass: pass_count,
                    warn: warn_count,
                    fail: fail_count,
                    skip: 0,
                },
                reasons: vec![format!("{}_fail", failed_metric.as_str())],
            },
            warnings: Vec::new(),
        }
    }

    fn metric_delta(metric: Metric, baseline: f64, current: f64, status: MetricStatus) -> Delta {
        Delta {
            baseline,
            current,
            ratio: current / baseline,
            pct: (current - baseline) / baseline,
            regression: crate::domain::budget::calculate_regression(
                baseline,
                current,
                metric.default_direction(),
            ),
            cv: None,
            noise_threshold: None,
            statistic: perfgate_types::MetricStatistic::Median,
            significance: None,
            status,
        }
    }

    fn probe_compare_receipt_with_metric(
        probe_name: &str,
        metric: Metric,
        current: f64,
        scope: ProbeScope,
    ) -> ProbeCompareReceipt {
        probe_compare_receipt_with_metric_many(&[(probe_name, metric, current, scope)])
    }

    fn probe_compare_receipt_with_metric_many(
        probes: &[(&str, Metric, f64, ProbeScope)],
    ) -> ProbeCompareReceipt {
        ProbeCompareReceipt {
            schema: PROBE_COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
            run: make_run_meta(),
            bench: None,
            scenario: Some("release_workload".to_string()),
            baseline_ref: None,
            current_ref: None,
            probes: probes
                .iter()
                .map(|(probe_name, metric, current, scope)| {
                    let delta = metric_delta(*metric, 100.0, *current, MetricStatus::Pass);
                    ProbeCompareObservation {
                        name: (*probe_name).to_string(),
                        parent: None,
                        scope: Some(*scope),
                        baseline_count: 1,
                        current_count: 1,
                        deltas: BTreeMap::from([(metric.as_str().to_string(), delta)]),
                        status: MetricStatus::Pass,
                        reasons: Vec::new(),
                    }
                })
                .collect(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: probes.len() as u32,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn tradeoff_evaluate_accepts_satisfied_rule() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(80.0, MetricStatus::Fail),
            probe_compares: Vec::new(),
            rules: vec![memory_for_speed_rule(TradeoffDowngrade::Warn)],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert_eq!(receipt.schema, TRADEOFF_SCHEMA_V1);
        assert!(receipt.decision.accepted_tradeoff);
        assert_eq!(
            receipt.weighted_deltas["max_rss_kb"].status,
            MetricStatus::Warn
        );
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(
            receipt.rules[0].requirements[0].observed_change,
            Some(-0.20)
        );
    }

    #[test]
    fn tradeoff_evaluate_rejects_unsatisfied_rule() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: Vec::new(),
            rules: vec![memory_for_speed_rule(TradeoffDowngrade::Pass)],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(!receipt.decision.accepted_tradeoff);
        assert_eq!(
            receipt.weighted_deltas["max_rss_kb"].status,
            MetricStatus::Fail
        );
        assert_eq!(receipt.verdict.status, VerdictStatus::Fail);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Rejected);
        assert!(
            receipt
                .verdict
                .reasons
                .contains(&VERDICT_REASON_TRADEOFF_RULE_NOT_SATISFIED.to_string())
        );
    }

    #[test]
    fn tradeoff_evaluate_accepts_satisfied_probe_requirement() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt("parser.batch_loop", 80.0)],
            rules: vec![memory_for_probe_speed_rule(TradeoffDowngrade::Warn)],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(
            receipt.rules[0].requirements[0].probe.as_deref(),
            Some("parser.batch_loop")
        );
        assert_eq!(
            receipt.rules[0].requirements[0].observed_change,
            Some(-0.20)
        );
        assert_eq!(receipt.probes.len(), 1);
        assert_eq!(receipt.probes[0].name, "parser.batch_loop");
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
    }

    #[test]
    fn tradeoff_evaluate_accepts_satisfied_probe_requirement_with_low_noise() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt_many_with_cv(&[(
                "parser.batch_loop",
                80.0,
                ProbeScope::Dominant,
                Some(0.05),
            )])],
            rules: vec![memory_for_probe_speed_rule(TradeoffDowngrade::Warn)],
            decision_policy: low_noise_policy(0.10),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert!(!receipt.decision.review_required);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
    }

    #[test]
    fn tradeoff_evaluate_marks_noisy_accepted_tradeoff_needs_review() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt_many_with_cv(&[(
                "parser.batch_loop",
                80.0,
                ProbeScope::Dominant,
                Some(0.18),
            )])],
            rules: vec![memory_for_probe_speed_rule(TradeoffDowngrade::Warn)],
            decision_policy: low_noise_policy(0.10),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(!receipt.decision.accepted_tradeoff);
        assert!(receipt.decision.review_required);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::NeedsReview);
        assert!(receipt.decision.review_reasons[0].contains("tradeoff evidence exceeds max_cv"));
        assert_eq!(
            receipt.weighted_deltas["max_rss_kb"].status,
            MetricStatus::Warn
        );
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
    }

    #[test]
    fn tradeoff_evaluate_marks_missing_noise_evidence_needs_review() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt("parser.batch_loop", 80.0)],
            rules: vec![memory_for_probe_speed_rule(TradeoffDowngrade::Warn)],
            decision_policy: low_noise_policy(0.10),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(!receipt.decision.accepted_tradeoff);
        assert!(receipt.decision.review_required);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::NeedsReview);
        assert!(receipt.decision.review_reasons[0].contains("noise evidence missing"));
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
    }

    #[test]
    fn tradeoff_evaluate_accepts_missing_noise_when_policy_allows_it() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt("parser.batch_loop", 80.0)],
            rules: vec![memory_for_probe_speed_rule(TradeoffDowngrade::Warn)],
            decision_policy: DecisionPolicyConfig {
                require_low_noise_for_acceptance: true,
                max_cv: Some(0.10),
                missing_noise: MissingNoisePolicy::Accept,
            },
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert!(!receipt.decision.review_required);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
    }

    #[test]
    fn tradeoff_evaluate_accepts_allowed_local_regression_cap() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt_many(&[
                ("parser.batch_loop", 80.0, ProbeScope::Dominant),
                ("parser.tokenize", 102.1, ProbeScope::Local),
            ])],
            rules: vec![memory_for_probe_speed_rule_with_allow(
                TradeoffDowngrade::Warn,
                0.03,
            )],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(receipt.rules[0].allowances[0].probe, "parser.tokenize");
        assert_eq!(
            receipt.rules[0].allowances[0].observed_regression,
            Some(0.020999999999999943)
        );
        assert!(receipt.rules[0].allowances[0].satisfied);
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
    }

    #[test]
    fn tradeoff_evaluate_rejects_local_regression_over_cap() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt_many(&[
                ("parser.batch_loop", 80.0, ProbeScope::Dominant),
                ("parser.tokenize", 105.0, ProbeScope::Local),
            ])],
            rules: vec![memory_for_probe_speed_rule_with_allow(
                TradeoffDowngrade::Warn,
                0.03,
            )],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(!receipt.decision.accepted_tradeoff);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Rejected);
        assert_eq!(
            receipt.rules[0].allowances[0].observed_regression,
            Some(0.05)
        );
        assert!(!receipt.rules[0].allowances[0].satisfied);
        assert!(
            receipt.rules[0].allowances[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("exceeds cap"))
        );
        assert_eq!(receipt.verdict.status, VerdictStatus::Fail);
    }

    #[test]
    fn tradeoff_evaluate_marks_missing_allowed_local_probe_needs_review() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt("parser.batch_loop", 80.0)],
            rules: vec![memory_for_probe_speed_rule_with_allow(
                TradeoffDowngrade::Warn,
                0.03,
            )],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(!receipt.decision.accepted_tradeoff);
        assert!(receipt.decision.review_required);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::NeedsReview);
        assert_eq!(receipt.rules[0].allowances[0].observed_regression, None);
        assert!(
            receipt.rules[0].allowances[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("allowed probe"))
        );
        assert_eq!(
            receipt.weighted_deltas["max_rss_kb"].status,
            MetricStatus::Warn
        );
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
        assert!(
            receipt
                .verdict
                .reasons
                .contains(&perfgate_types::VERDICT_REASON_TRADEOFF_REVIEW_REQUIRED.to_string())
        );
    }

    #[test]
    fn tradeoff_evaluate_marks_missing_probe_requirement_needs_review() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt("parser.tokenize", 80.0)],
            rules: vec![memory_for_probe_speed_rule(TradeoffDowngrade::Pass)],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(!receipt.decision.accepted_tradeoff);
        assert!(receipt.decision.review_required);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::NeedsReview);
        assert_eq!(receipt.rules[0].requirements[0].observed_change, None);
        assert!(
            receipt.rules[0].requirements[0]
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("required probe"))
        );
        assert_eq!(receipt.probes.len(), 1);
        assert_eq!(receipt.probes[0].name, "parser.tokenize");
        assert_eq!(receipt.verdict.status, VerdictStatus::Warn);
    }

    #[test]
    fn tradeoff_evaluate_rejects_unsatisfied_present_probe_requirement() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt(96.0, MetricStatus::Fail),
            probe_compares: vec![probe_compare_receipt("parser.batch_loop", 96.0)],
            rules: vec![memory_for_probe_speed_rule(TradeoffDowngrade::Pass)],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(!receipt.decision.accepted_tradeoff);
        assert!(!receipt.decision.review_required);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Rejected);
        assert_eq!(receipt.verdict.status, VerdictStatus::Fail);
    }

    #[test]
    fn tradeoff_accepts_latency_improvement_while_throughput_regresses() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt_with_metric_deltas(
                vec![
                    (
                        Metric::ThroughputPerS,
                        metric_delta(Metric::ThroughputPerS, 100.0, 80.0, MetricStatus::Fail),
                    ),
                    (
                        Metric::WallMs,
                        metric_delta(Metric::WallMs, 100.0, 80.0, MetricStatus::Pass),
                    ),
                ],
                Metric::ThroughputPerS,
            ),
            probe_compares: Vec::new(),
            rules: vec![tradeoff_rule_for_requirement(
                Metric::ThroughputPerS,
                Metric::WallMs,
                TradeoffDowngrade::Warn,
            )],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(
            receipt.rules[0].requirements[0].observed_change,
            Some(-0.20)
        );
        assert_eq!(
            receipt.weighted_deltas["throughput_per_s"].status,
            MetricStatus::Warn
        );
    }

    #[test]
    fn tradeoff_accepts_throughput_improvement_while_latency_regresses() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt_with_metric_deltas(
                vec![
                    (
                        Metric::WallMs,
                        metric_delta(Metric::WallMs, 100.0, 120.0, MetricStatus::Fail),
                    ),
                    (
                        Metric::ThroughputPerS,
                        metric_delta(Metric::ThroughputPerS, 100.0, 125.0, MetricStatus::Pass),
                    ),
                ],
                Metric::WallMs,
            ),
            probe_compares: Vec::new(),
            rules: vec![tradeoff_rule_for_requirement(
                Metric::WallMs,
                Metric::ThroughputPerS,
                TradeoffDowngrade::Warn,
            )],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(receipt.rules[0].requirements[0].observed_change, Some(0.25));
        assert_eq!(
            receipt.weighted_deltas["wall_ms"].status,
            MetricStatus::Warn
        );
    }

    #[test]
    fn tradeoff_accepts_higher_is_better_dominant_probe_improvement() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt_with_metric_deltas(
                vec![(
                    Metric::WallMs,
                    metric_delta(Metric::WallMs, 100.0, 120.0, MetricStatus::Fail),
                )],
                Metric::WallMs,
            ),
            probe_compares: vec![probe_compare_receipt_with_metric(
                "parser.batch_loop",
                Metric::ThroughputPerS,
                125.0,
                ProbeScope::Dominant,
            )],
            rules: vec![tradeoff_rule_for_probe_requirement(
                Metric::WallMs,
                Metric::ThroughputPerS,
                "parser.batch_loop",
                TradeoffDowngrade::Warn,
            )],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(receipt.rules[0].requirements[0].observed_change, Some(0.25));
        assert_eq!(
            receipt.rules[0].requirements[0].probe.as_deref(),
            Some("parser.batch_loop")
        );
    }

    #[test]
    fn tradeoff_accepts_local_probe_regression_when_dominant_throughput_improves() {
        let outcome = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario: scenario_receipt_with_metric_deltas(
                vec![(
                    Metric::WallMs,
                    metric_delta(Metric::WallMs, 100.0, 120.0, MetricStatus::Fail),
                )],
                Metric::WallMs,
            ),
            probe_compares: vec![probe_compare_receipt_with_metric_many(&[
                (
                    "parser.batch_loop",
                    Metric::ThroughputPerS,
                    125.0,
                    ProbeScope::Dominant,
                ),
                ("parser.tokenize", Metric::WallMs, 102.0, ProbeScope::Local),
            ])],
            rules: vec![tradeoff_rule_for_probe_requirement_with_allow(
                Metric::WallMs,
                Metric::ThroughputPerS,
                "parser.batch_loop",
                Metric::WallMs,
                "parser.tokenize",
                0.03,
                TradeoffDowngrade::Warn,
            )],
            decision_policy: DecisionPolicyConfig::default(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
        })
        .expect("evaluate tradeoff");

        let receipt = outcome.receipt;
        assert!(receipt.decision.accepted_tradeoff);
        assert_eq!(receipt.rules[0].status, TradeoffDecisionStatus::Accepted);
        assert_eq!(receipt.rules[0].requirements[0].observed_change, Some(0.25));
        assert_eq!(
            receipt.rules[0].allowances[0].observed_regression,
            Some(0.02)
        );
        assert!(receipt.rules[0].allowances[0].satisfied);
    }
}
