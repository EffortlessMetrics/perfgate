//! Rendering utilities for perfgate output.
//!
//! This module provides functions for rendering performance comparison results
//! as markdown tables and GitHub Actions annotations.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

pub mod summary;

use anyhow::Context;
use perfgate_types::{
    CompareReceipt, ComplexityGateResult, ComplexityGateStatus, Delta, Direction, Metric,
    MetricStatistic, MetricStatus, TradeoffAllowanceOutcome, TradeoffDecisionStatus,
    TradeoffReceipt, TradeoffRequirementOutcome,
};
use serde_json::json;

/// Render a [`CompareReceipt`] as a Markdown table for PR comments.
pub fn render_markdown(compare: &CompareReceipt) -> String {
    let mut out = String::new();

    let header = match compare.verdict.status {
        perfgate_types::VerdictStatus::Pass => "✅ perfgate: pass",
        perfgate_types::VerdictStatus::Warn => "⚠️ perfgate: warn",
        perfgate_types::VerdictStatus::Fail => "❌ perfgate: fail",
        perfgate_types::VerdictStatus::Skip => "⏭️ perfgate: skip",
    };

    out.push_str(header);
    out.push_str("\n\n");

    out.push_str(&format!("**Bench:** `{}`\n\n", compare.bench.name));

    out.push_str("| metric | baseline (median) | current (median) | delta | budget | status |\n");
    out.push_str("|---|---:|---:|---:|---:|---|\n");

    for (metric, delta) in &compare.deltas {
        let budget = compare.budgets.get(metric);
        let (budget_str, direction_str) = if let Some(b) = budget {
            (
                format!("{:.1}%", b.threshold * 100.0),
                direction_str(b.direction),
            )
        } else {
            ("".to_string(), "")
        };

        let mut status_icon = metric_status_icon(delta.status).to_string();

        // If noisy, append noise info
        if let (Some(cv), Some(limit)) = (delta.cv, delta.noise_threshold)
            && cv > limit
        {
            status_icon.push_str(" (noisy)");
        }

        out.push_str(&format!(
            "| `{metric}` | {b} {u} | {c} {u} | {pct} | {budget} ({dir}) | {status} |\n",
            metric = format_metric_with_statistic(*metric, delta.statistic),
            b = format_value(*metric, delta.baseline),
            c = format_value(*metric, delta.current),
            u = metric.display_unit(),
            pct = format_pct(delta.pct),
            budget = budget_str,
            dir = direction_str,
            status = status_icon,
        ));
    }

    if !compare.verdict.reasons.is_empty() {
        out.push_str("\n**Notes:**\n");
        for r in &compare.verdict.reasons {
            out.push_str(&render_reason_line(compare, r));
        }
    }

    out
}

/// Render a [`TradeoffReceipt`] as Markdown for review comments and local diagnostics.
pub fn render_tradeoff_markdown(tradeoff: &TradeoffReceipt) -> String {
    let mut out = String::new();

    let header = match tradeoff.verdict.status {
        perfgate_types::VerdictStatus::Pass => "✅ perfgate tradeoff: pass",
        perfgate_types::VerdictStatus::Warn => "⚠️ perfgate tradeoff: warn",
        perfgate_types::VerdictStatus::Fail => "❌ perfgate tradeoff: fail",
        perfgate_types::VerdictStatus::Skip => "⏭️ perfgate tradeoff: skip",
    };

    out.push_str(header);
    out.push_str("\n\n");

    out.push_str("### Summary\n\n");
    out.push_str("| field | value |\n");
    out.push_str("|---|---|\n");
    out.push_str(&format!(
        "| final verdict | {} {} |\n",
        metric_status_icon(tradeoff.decision.status),
        metric_status_str(tradeoff.decision.status)
    ));
    if let Some(scenario) = &tradeoff.scenario {
        out.push_str(&format!("| scenario | `{scenario}` |\n"));
    }
    out.push_str(&format!("| decision | {} |\n", tradeoff.decision.reason));
    out.push_str(&format!(
        "| accepted tradeoff | {} |\n",
        if tradeoff.decision.accepted_tradeoff {
            "yes"
        } else {
            "no"
        }
    ));
    out.push_str(&format!(
        "| review required | {} |\n",
        if tradeoff.decision.review_required {
            "yes"
        } else {
            "no"
        }
    ));
    out.push('\n');

    if tradeoff.decision.review_required {
        out.push_str("### Review Required\n\n");
        if tradeoff.decision.review_reasons.is_empty() {
            out.push_str("- evidence is incomplete; review required\n");
        } else {
            for reason in &tradeoff.decision.review_reasons {
                out.push_str(&format!("- {reason}\n"));
            }
        }
        out.push('\n');
    }

    out.push_str("### Weighted Workload\n\n");
    if tradeoff.weighted_deltas.is_empty() {
        out.push_str("No weighted workload deltas recorded.\n\n");
    } else {
        out.push_str("| metric | baseline | current | delta | status |\n");
        out.push_str("|---|---:|---:|---:|---|\n");
        for (metric_key, delta) in &tradeoff.weighted_deltas {
            let (baseline, current) = format_delta_values(metric_key, delta);
            out.push_str(&format!(
                "| `{metric_key}` | {baseline} | {current} | {delta_pct} | {status} |\n",
                delta_pct = format_pct(delta.pct),
                status = metric_status_icon(delta.status),
            ));
        }
        out.push('\n');
    }

    out.push_str("### Probe Evidence\n\n");
    if tradeoff.probes.is_empty() {
        out.push_str("No probe evidence attached.\n\n");
    } else {
        out.push_str("| probe | scope | metric | baseline | current | delta | status | reason |\n");
        out.push_str("|---|---|---|---:|---:|---:|---|---|\n");
        for probe in &tradeoff.probes {
            let scope = probe
                .scope
                .map(|scope| format!("{:?}", scope).to_lowercase())
                .unwrap_or_else(|| "-".to_string());
            let reason = probe.reason.as_deref().unwrap_or("-");
            if probe.deltas.is_empty() {
                out.push_str(&format!(
                    "| `{}` | `{}` | `-` | - | - | - | {} | {} |\n",
                    probe.name,
                    scope,
                    metric_status_icon(probe.status),
                    reason
                ));
                continue;
            }
            for (metric_key, delta) in &probe.deltas {
                let (baseline, current) = format_delta_values(metric_key, delta);
                out.push_str(&format!(
                    "| `{}` | `{}` | `{}` | {} | {} | {} | {} | {} |\n",
                    probe.name,
                    scope,
                    metric_key,
                    baseline,
                    current,
                    format_pct(delta.pct),
                    metric_status_icon(delta.status),
                    reason
                ));
            }
        }
        out.push('\n');
    }

    out.push_str("### Accepted / Rejected Tradeoffs\n\n");
    if tradeoff.rules.is_empty() {
        out.push_str("No tradeoff rules evaluated.\n\n");
    } else {
        out.push_str("| rule | decision | downgrade | requirements | local caps | reason |\n");
        out.push_str("|---|---|---|---|---|---|\n");
        for rule in &tradeoff.rules {
            let requirements = if rule.requirements.is_empty() {
                "none".to_string()
            } else {
                rule.requirements
                    .iter()
                    .map(render_tradeoff_requirement)
                    .collect::<Vec<_>>()
                    .join("<br>")
            };
            let allowances = if rule.allowances.is_empty() {
                "none".to_string()
            } else {
                rule.allowances
                    .iter()
                    .map(render_tradeoff_allowance)
                    .collect::<Vec<_>>()
                    .join("<br>")
            };
            let downgrade = rule
                .downgrade_to
                .map(tradeoff_downgrade_label)
                .unwrap_or("-");
            let reason = rule.reason.as_deref().unwrap_or("-");
            out.push_str(&format!(
                "| `{}` | {} | `{}` | {} | {} | {} |\n",
                rule.name,
                tradeoff_decision_label(rule.status),
                downgrade,
                requirements,
                allowances,
                reason
            ));
        }
        out.push('\n');
    }

    out.push_str("### Policy Reasons\n\n");
    if tradeoff.verdict.reasons.is_empty() && tradeoff.warnings.is_empty() {
        out.push_str("- none\n");
    } else {
        for reason in &tradeoff.verdict.reasons {
            out.push_str(&format!("- `{reason}`\n"));
        }
        for warning in &tradeoff.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
    }
    out.push('\n');

    out.push_str("### Evidence Files\n\n");
    out.push_str("- `scenario.json`: weighted workload evidence\n");
    out.push_str("- `tradeoff.json`: structured tradeoff decision receipt\n");
    out.push_str("- `decision.md`: this review summary\n");
    if let Some(reference) = &tradeoff.baseline_ref {
        if let Some(path) = &reference.path {
            out.push_str(&format!("- baseline: `{path}`\n"));
        }
        if let Some(run_id) = &reference.run_id {
            out.push_str(&format!("- baseline run: `{run_id}`\n"));
        }
    }
    if let Some(reference) = &tradeoff.current_ref {
        if let Some(path) = &reference.path {
            out.push_str(&format!("- current: `{path}`\n"));
        }
        if let Some(run_id) = &reference.run_id {
            out.push_str(&format!("- current run: `{run_id}`\n"));
        }
    }
    out.push('\n');

    out.push_str("### Local Reproduction\n\n");
    out.push_str("```bash\n");
    out.push_str("perfgate decision evaluate --config perfgate.toml\n");
    out.push_str("```\n\n");

    out
}

fn render_tradeoff_requirement(requirement: &TradeoffRequirementOutcome) -> String {
    let observed = requirement
        .observed_change
        .map(format_pct)
        .unwrap_or_else(|| "missing".to_string());
    let target = requirement
        .probe
        .as_deref()
        .map(|probe| format!("probe `{probe}` `{}`", requirement.metric))
        .unwrap_or_else(|| format!("`{}`", requirement.metric));
    let reason = requirement
        .reason
        .as_deref()
        .map(|reason| format!(" ({reason})"))
        .unwrap_or_default();
    format!(
        "{target} observed {observed} / required {required} {status}{reason}",
        required = format_pct(requirement.required_change),
        status = metric_status_icon(requirement.status)
    )
}

fn render_tradeoff_allowance(allowance: &TradeoffAllowanceOutcome) -> String {
    let observed = allowance
        .observed_regression
        .map(format_pct)
        .unwrap_or_else(|| "missing".to_string());
    let reason = allowance
        .reason
        .as_deref()
        .map(|reason| format!(" ({reason})"))
        .unwrap_or_default();
    format!(
        "probe `{probe}` `{metric}` regression {observed} / cap {cap} {status}{reason}",
        probe = allowance.probe,
        metric = allowance.metric,
        cap = format_pct(allowance.max_regression),
        status = metric_status_icon(allowance.status)
    )
}

fn format_delta_values(metric_key: &str, delta: &Delta) -> (String, String) {
    Metric::parse_key(metric_key)
        .map(|metric| {
            (
                format_value_with_unit(metric, delta.baseline),
                format_value_with_unit(metric, delta.current),
            )
        })
        .unwrap_or_else(|| {
            (
                format!("{:.3}", delta.baseline),
                format!("{:.3}", delta.current),
            )
        })
}

fn format_value_with_unit(metric: Metric, value: f64) -> String {
    let value = format_value(metric, value);
    let unit = metric.display_unit();
    if unit.is_empty() {
        value
    } else {
        format!("{value} {unit}")
    }
}

/// Render a complexity-gate section for markdown reports.
pub fn render_complexity_section(complexity: &ComplexityGateResult) -> String {
    let mut out = String::new();
    out.push_str("\n### Complexity Gate\n\n");
    let status = match complexity.status {
        ComplexityGateStatus::Pass => "✅ pass",
        ComplexityGateStatus::Fail => "❌ fail",
        ComplexityGateStatus::Inconclusive => "❔ inconclusive",
    };
    out.push_str(&format!("**Status:** {status}\n\n"));
    if let Some(expected) = &complexity.expected {
        out.push_str(&format!("* Expected: `{expected}`\n"));
    }
    if let Some(observed) = &complexity.observed {
        out.push_str(&format!("* Observed: `{observed}`\n"));
    }
    if let Some(r_squared) = complexity.r_squared {
        out.push_str(&format!(
            "* R²: `{r_squared:.4}` (threshold `{:.4}`)\n",
            complexity.r_squared_threshold
        ));
    }
    out.push_str(&format!("* Details: {}\n", complexity.message));
    out
}

/// Render a [`CompareReceipt`] using a custom [Handlebars](https://docs.rs/handlebars) template.
pub fn render_markdown_template(
    compare: &CompareReceipt,
    template: &str,
) -> anyhow::Result<String> {
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.set_strict_mode(true);
    handlebars
        .register_template_string("markdown", template)
        .context("parse markdown template")?;

    let context = markdown_template_context(compare);
    handlebars
        .render("markdown", &context)
        .context("render markdown template")
}

/// Produce GitHub Actions annotation strings from a [`CompareReceipt`].
pub fn github_annotations(compare: &CompareReceipt) -> Vec<String> {
    let mut lines = Vec::new();

    for (metric, delta) in &compare.deltas {
        let prefix = match delta.status {
            MetricStatus::Fail => "::error",
            MetricStatus::Warn => "::warning",
            MetricStatus::Pass | MetricStatus::Skip => continue,
        };

        let msg = format!(
            "perfgate {bench} {metric}: {pct} (baseline {b}{u}, current {c}{u})",
            bench = compare.bench.name,
            metric = format_metric_with_statistic(*metric, delta.statistic),
            pct = format_pct(delta.pct),
            b = format_value(*metric, delta.baseline),
            c = format_value(*metric, delta.current),
            u = metric.display_unit(),
        );

        lines.push(format!("{prefix}::{msg}"));
    }

    lines
}

/// Return the canonical string key for a [`Metric`].
pub fn format_metric(metric: Metric) -> &'static str {
    metric.as_str()
}

/// Format a metric key, appending the statistic name when it is not the default (median).
pub fn format_metric_with_statistic(metric: Metric, statistic: MetricStatistic) -> String {
    if statistic == MetricStatistic::Median {
        format_metric(metric).to_string()
    } else {
        format!("{} ({})", format_metric(metric), statistic.as_str())
    }
}

/// Build the JSON context object used by [`render_markdown_template`].
pub fn markdown_template_context(compare: &CompareReceipt) -> serde_json::Value {
    let header = match compare.verdict.status {
        perfgate_types::VerdictStatus::Pass => "✅ perfgate: pass",
        perfgate_types::VerdictStatus::Warn => "⚠️ perfgate: warn",
        perfgate_types::VerdictStatus::Fail => "❌ perfgate: fail",
        perfgate_types::VerdictStatus::Skip => "⏭️ perfgate: skip",
    };

    let rows: Vec<serde_json::Value> = compare
        .deltas
        .iter()
        .map(|(metric, delta)| {
            let budget = compare.budgets.get(metric);
            let (budget_threshold_pct, budget_direction) = budget
                .map(|b| (b.threshold * 100.0, direction_str(b.direction).to_string()))
                .unwrap_or((0.0, String::new()));

            json!({
                "metric": format_metric(*metric),
                "metric_with_statistic": format_metric_with_statistic(*metric, delta.statistic),
                "statistic": delta.statistic.as_str(),
                "baseline": format_value(*metric, delta.baseline),
                "current": format_value(*metric, delta.current),
                "unit": metric.display_unit(),
                "delta_pct": format_pct(delta.pct),
                "budget_threshold_pct": budget_threshold_pct,
                "budget_direction": budget_direction,
                "status": metric_status_str(delta.status),
                "status_icon": metric_status_icon(delta.status),
                "raw": {
                    "baseline": delta.baseline,
                    "current": delta.current,
                    "pct": delta.pct,
                    "regression": delta.regression,
                    "statistic": delta.statistic.as_str(),
                    "significance": delta.significance
                }
            })
        })
        .collect();

    json!({
        "header": header,
        "bench": compare.bench,
        "verdict": compare.verdict,
        "rows": rows,
        "reasons": compare.verdict.reasons,
        "compare": compare
    })
}

/// Parse a verdict reason token like `"wall_ms_warn"` into its metric and status.
pub fn parse_reason_token(token: &str) -> Option<(Metric, MetricStatus)> {
    let (metric_part, status_part) = token.rsplit_once('_')?;

    let status = match status_part {
        "warn" => MetricStatus::Warn,
        "fail" => MetricStatus::Fail,
        "skip" => MetricStatus::Skip,
        _ => return None,
    };

    let metric = Metric::parse_key(metric_part)?;

    Some((metric, status))
}

/// Render a single verdict reason token as a human-readable bullet line.
pub fn render_reason_line(compare: &CompareReceipt, token: &str) -> String {
    if let Some(rule_name) = token
        .strip_prefix("tradeoff_")
        .and_then(|rest| rest.strip_suffix("_applied"))
    {
        return format!(
            "- tradeoff applied (`{rule_name}`): metric breach downgraded per config\n"
        );
    }
    if token == "tradeoff_rule_not_satisfied" {
        return "- tradeoff rule not satisfied; original budget verdict kept\n".to_string();
    }
    if token == "tradeoff_missing_required_metric" {
        return "- tradeoff could not be evaluated: required metric missing\n".to_string();
    }
    if token == "tradeoff_review_required" {
        return "- tradeoff requires review: evidence is incomplete\n".to_string();
    }

    let context = parse_reason_token(token).and_then(|(metric, status)| {
        compare
            .deltas
            .get(&metric)
            .zip(compare.budgets.get(&metric))
            .map(|(delta, budget)| (status, delta, budget))
    });

    if let Some((status, delta, budget)) = context {
        let pct = format_pct(delta.pct);
        let warn_pct = budget.warn_threshold * 100.0;
        let fail_pct = budget.threshold * 100.0;

        return match status {
            MetricStatus::Warn => {
                let mut msg =
                    format!("- {token}: {pct} (warn >= {warn_pct:.2}%, fail > {fail_pct:.2}%)");
                if let (Some(cv), Some(limit)) = (delta.cv, delta.noise_threshold)
                    && cv > limit
                {
                    msg.push_str(&format!(
                        " [NOISY: CV {:.2}% > limit {:.2}%]",
                        cv * 100.0,
                        limit * 100.0
                    ));
                }
                msg.push('\n');
                msg
            }
            MetricStatus::Fail => {
                format!("- {token}: {pct} (fail > {fail_pct:.2}%)\n")
            }
            MetricStatus::Skip => {
                let mut msg = format!("- {token}: skipped");
                if let (Some(cv), Some(limit)) = (delta.cv, delta.noise_threshold)
                    && cv > limit
                {
                    msg.push_str(&format!(
                        " [NOISY: CV {:.2}% > limit {:.2}%]",
                        cv * 100.0,
                        limit * 100.0
                    ));
                }
                msg.push('\n');
                msg
            }
            MetricStatus::Pass => String::new(),
        };
    }

    format!("- {token}\n")
}

/// Format a metric value for display.
pub fn format_value(metric: Metric, v: f64) -> String {
    match metric {
        Metric::BinaryBytes
        | Metric::CpuMs
        | Metric::CtxSwitches
        | Metric::EnergyUj
        | Metric::IoReadBytes
        | Metric::IoWriteBytes
        | Metric::MaxRssKb
        | Metric::NetworkPackets
        | Metric::PageFaults
        | Metric::WallMs => format!("{:.0}", v),
        Metric::ThroughputPerS => format!("{:.3}", v),
    }
}

/// Format a fractional change as a percentage string.
pub fn format_pct(pct: f64) -> String {
    let sign = if pct > 0.0 { "+" } else { "" };
    format!("{}{:.2}%", sign, pct * 100.0)
}

/// Return a human-readable label for a budget [`Direction`].
pub fn direction_str(direction: Direction) -> &'static str {
    match direction {
        Direction::Lower => "lower",
        Direction::Higher => "higher",
    }
}

/// Return an emoji icon for a [`MetricStatus`].
pub fn metric_status_icon(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Pass => "✅",
        MetricStatus::Warn => "⚠️",
        MetricStatus::Fail => "❌",
        MetricStatus::Skip => "⏭️",
    }
}

/// Return a lowercase string label for a [`MetricStatus`].
pub fn metric_status_str(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Pass => "pass",
        MetricStatus::Warn => "warn",
        MetricStatus::Fail => "fail",
        MetricStatus::Skip => "skip",
    }
}

fn tradeoff_decision_label(status: TradeoffDecisionStatus) -> &'static str {
    match status {
        TradeoffDecisionStatus::Accepted => "accepted",
        TradeoffDecisionStatus::Rejected => "rejected",
        TradeoffDecisionStatus::NeedsReview => "needs review",
        TradeoffDecisionStatus::NotEvaluated => "not evaluated",
    }
}

fn tradeoff_downgrade_label(downgrade: perfgate_types::TradeoffDowngrade) -> &'static str {
    match downgrade {
        perfgate_types::TradeoffDowngrade::Warn => "warn",
        perfgate_types::TradeoffDowngrade::Pass => "pass",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, ProbeScope, RunMeta, ToolInfo, TradeoffDecision,
        TradeoffProbeOutcome, TradeoffRuleOutcome, Verdict, VerdictCounts, VerdictStatus,
    };
    use std::collections::BTreeMap;

    fn make_compare_receipt(status: MetricStatus) -> CompareReceipt {
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::WallMs, Budget::new(0.2, 0.1, Direction::Lower));

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 115.0,
                ratio: 1.15,
                pct: 0.15,
                regression: 0.15,
                statistic: MetricStatistic::Median,
                significance: None,
                cv: None,
                noise_threshold: None,
                status,
            },
        );

        CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "bench".into(),
                cwd: None,
                command: vec!["true".into()],
                repeat: 1,
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
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 1,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec!["wall_ms_warn".to_string()],
            },
        }
    }

    fn make_tradeoff_receipt(status: MetricStatus) -> TradeoffReceipt {
        let mut weighted_deltas = BTreeMap::new();
        weighted_deltas.insert(
            "wall_ms".to_string(),
            Delta {
                baseline: 100.0,
                current: 88.0,
                ratio: 0.88,
                pct: -0.12,
                regression: 0.0,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        weighted_deltas.insert(
            "max_rss_kb".to_string(),
            Delta {
                baseline: 100.0,
                current: 115.0,
                ratio: 1.15,
                pct: 0.15,
                regression: 0.15,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status,
            },
        );

        TradeoffReceipt {
            schema: perfgate_types::TRADEOFF_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.16.0".to_string(),
            },
            run: RunMeta {
                id: "tradeoff-run".to_string(),
                started_at: "2026-05-08T00:00:00Z".to_string(),
                ended_at: "2026-05-08T00:00:01Z".to_string(),
                host: perfgate_types::HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            scenario: Some("release_workload".to_string()),
            baseline_ref: None,
            current_ref: None,
            configured_rules: Vec::new(),
            rules: vec![TradeoffRuleOutcome {
                name: "memory_for_speed".to_string(),
                status: TradeoffDecisionStatus::Accepted,
                accepted: true,
                downgrade_to: Some(perfgate_types::TradeoffDowngrade::Warn),
                reason: Some("all required compensating improvements were satisfied".to_string()),
                requirements: vec![perfgate_types::TradeoffRequirementOutcome {
                    metric: "wall_ms".to_string(),
                    probe: None,
                    required_change: -0.10,
                    observed_change: Some(-0.12),
                    satisfied: true,
                    status: MetricStatus::Pass,
                    reason: None,
                }],
                allowances: vec![perfgate_types::TradeoffAllowanceOutcome {
                    metric: "wall_ms".to_string(),
                    probe: "parser.tokenize".to_string(),
                    max_regression: 0.03,
                    observed_regression: Some(0.021),
                    satisfied: true,
                    status: MetricStatus::Pass,
                    reason: None,
                }],
            }],
            probes: vec![
                TradeoffProbeOutcome {
                    name: "parser.tokenize".to_string(),
                    scope: Some(ProbeScope::Local),
                    weight: None,
                    deltas: BTreeMap::from([(
                        "wall_ms".to_string(),
                        Delta {
                            baseline: 100.0,
                            current: 102.1,
                            ratio: 1.021,
                            pct: 0.021,
                            regression: 0.021,
                            cv: None,
                            noise_threshold: None,
                            statistic: MetricStatistic::Median,
                            significance: None,
                            status: MetricStatus::Warn,
                        },
                    )]),
                    status: MetricStatus::Warn,
                    reason: Some("local phase regressed".to_string()),
                },
                TradeoffProbeOutcome {
                    name: "parser.batch_loop".to_string(),
                    scope: Some(ProbeScope::Dominant),
                    weight: None,
                    deltas: BTreeMap::from([(
                        "wall_ms".to_string(),
                        Delta {
                            baseline: 100.0,
                            current: 89.6,
                            ratio: 0.896,
                            pct: -0.104,
                            regression: 0.0,
                            cv: None,
                            noise_threshold: None,
                            statistic: MetricStatistic::Median,
                            significance: None,
                            status: MetricStatus::Pass,
                        },
                    )]),
                    status: MetricStatus::Pass,
                    reason: None,
                },
            ],
            weighted_deltas,
            decision: TradeoffDecision {
                accepted_tradeoff: true,
                review_required: false,
                review_reasons: Vec::new(),
                status,
                reason: "tradeoff 'memory_for_speed' accepted".to_string(),
            },
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 1,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec!["tradeoff_memory_for_speed_applied".to_string()],
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn markdown_renders_table() {
        let receipt = make_compare_receipt(MetricStatus::Pass);
        let md = render_markdown(&receipt);
        assert!(md.contains("| metric | baseline"));
        assert!(md.contains("wall_ms"));
    }

    #[test]
    fn tradeoff_markdown_renders_decision_and_rules() {
        let receipt = make_tradeoff_receipt(MetricStatus::Warn);
        let md = render_tradeoff_markdown(&receipt);

        assert!(md.contains("perfgate tradeoff: warn"));
        assert!(md.contains("### Summary"));
        assert!(md.contains("| scenario | `release_workload` |"));
        assert!(md.contains("tradeoff 'memory_for_speed' accepted"));
        assert!(md.contains("### Weighted Workload"));
        assert!(md.contains("| `max_rss_kb` |"));
        assert!(md.contains("### Probe Evidence"));
        assert!(md.contains("| `parser.tokenize` | `local` | `wall_ms`"));
        assert!(md.contains("+2.10%"));
        assert!(md.contains("| `parser.batch_loop` | `dominant` | `wall_ms`"));
        assert!(md.contains("-10.40%"));
        assert!(md.contains("### Accepted / Rejected Tradeoffs"));
        assert!(md.contains("| `memory_for_speed` | accepted | `warn` |"));
        assert!(md.contains("`wall_ms` observed -12.00% / required -10.00%"));
        assert!(md.contains("### Policy Reasons"));
        assert!(md.contains("### Evidence Files"));
        assert!(md.contains("### Local Reproduction"));
    }

    #[test]
    fn tradeoff_markdown_renders_review_required() {
        let mut receipt = make_tradeoff_receipt(MetricStatus::Warn);
        receipt.decision.accepted_tradeoff = false;
        receipt.decision.review_required = true;
        receipt.decision.review_reasons =
            vec!["tradeoff 'memory_for_speed' requires review: evidence is incomplete".to_string()];
        receipt.decision.reason = "tradeoff 'memory_for_speed' requires review".to_string();
        receipt.rules[0].status = TradeoffDecisionStatus::NeedsReview;
        receipt.rules[0].accepted = false;

        let md = render_tradeoff_markdown(&receipt);

        assert!(md.contains("| review required | yes |"));
        assert!(md.contains("### Review Required"));
        assert!(md.contains("evidence is incomplete"));
        assert!(md.contains("| `memory_for_speed` | needs review |"));
    }

    #[test]
    fn markdown_template_renders_context_rows() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let template = "{{header}}\nbench={{bench.name}}\n{{#each rows}}metric={{metric}} status={{status}}\n{{/each}}";

        let rendered = render_markdown_template(&compare, template).expect("render template");
        assert!(rendered.contains("bench=bench"));
        assert!(rendered.contains("metric=wall_ms"));
        assert!(rendered.contains("status=warn"));
    }

    #[test]
    fn parse_reason_token_handles_valid_and_invalid() {
        let parsed = parse_reason_token("wall_ms_warn");
        assert!(parsed.is_some());
        let (metric, status) = parsed.unwrap();
        assert_eq!(metric, Metric::WallMs);
        assert_eq!(status, MetricStatus::Warn);

        assert!(parse_reason_token("wall_ms_pass").is_none());
        assert!(parse_reason_token("unknown_warn").is_none());
    }

    #[test]
    fn github_annotations_only_warn_and_fail() {
        let mut compare = make_compare_receipt(MetricStatus::Warn);
        compare.deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 100.0,
                current: 150.0,
                ratio: 1.5,
                pct: 0.5,
                regression: 0.5,
                statistic: MetricStatistic::Median,
                significance: None,
                cv: None,
                noise_threshold: None,
                status: MetricStatus::Fail,
            },
        );

        let lines = github_annotations(&compare);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.starts_with("::warning::")));
        assert!(lines.iter().any(|l| l.starts_with("::error::")));
    }

    #[test]
    fn render_reason_line_handles_tradeoff_tokens() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let applied = render_reason_line(&compare, "tradeoff_memory_for_speed_applied");
        let missing = render_reason_line(&compare, "tradeoff_missing_required_metric");
        let unsatisfied = render_reason_line(&compare, "tradeoff_rule_not_satisfied");
        let review = render_reason_line(&compare, "tradeoff_review_required");

        assert!(applied.contains("tradeoff applied"));
        assert!(missing.contains("required metric missing"));
        assert!(unsatisfied.contains("not satisfied"));
        assert!(review.contains("requires review"));
    }
}
