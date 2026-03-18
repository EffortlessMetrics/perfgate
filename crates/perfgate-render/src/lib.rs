//! Rendering utilities for perfgate output.
//!
//! This crate provides functions for rendering performance comparison results
//! as markdown tables and GitHub Actions annotations.

use anyhow::Context;
use perfgate_types::{CompareReceipt, Direction, Metric, MetricStatistic, MetricStatus};
use serde_json::json;

/// Render a [`CompareReceipt`] as a Markdown table for PR comments.
pub fn render_markdown(compare: &CompareReceipt) -> String {
    println!("DEBUG: render_markdown called");
    let mut out = String::new();

    let header = match compare.verdict.status {
        perfgate_types::VerdictStatus::Pass => "✅ perfgate: pass",
        perfgate_types::VerdictStatus::Warn => "⚠️ perfgate: warn",
        perfgate_types::VerdictStatus::Fail => "❌ perfgate: fail",
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

        let status_icon = metric_status_icon(delta.status);

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
            MetricStatus::Pass => continue,
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
        _ => return None,
    };

    let metric = Metric::parse_key(metric_part)?;

    Some((metric, status))
}

/// Render a single verdict reason token as a human-readable bullet line.
pub fn render_reason_line(compare: &CompareReceipt, token: &str) -> String {
    if let Some((metric, status)) = parse_reason_token(token) {
        if let (Some(delta), Some(budget)) = (compare.deltas.get(&metric), compare.budgets.get(&metric)) {
            let pct = format_pct(delta.pct);
            let warn_pct = budget.warn_threshold * 100.0;
            let fail_pct = budget.threshold * 100.0;

            return match status {
                MetricStatus::Warn => {
                    format!("- {token}: {pct} (warn >= {warn_pct:.2}%, fail > {fail_pct:.2}%)\n")
                }
                MetricStatus::Fail => {
                    format!("- {token}: {pct} (fail > {fail_pct:.2}%)\n")
                }
                MetricStatus::Pass => format!("- {token}\n"),
            };
        }
    }

    format!("- {token}\n")
}

/// Format a metric value for display.
pub fn format_value(metric: Metric, v: f64) -> String {
    match metric {
        Metric::BinaryBytes
        | Metric::CpuMs
        | Metric::CtxSwitches
        | Metric::MaxRssKb
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
    }
}

/// Return a lowercase string label for a [`MetricStatus`].
pub fn metric_status_str(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Pass => "pass",
        MetricStatus::Warn => "warn",
        MetricStatus::Fail => "fail",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
    };
    use std::collections::BTreeMap;

    fn make_compare_receipt(status: MetricStatus) -> CompareReceipt {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.1,
                direction: Direction::Lower,
            },
        );

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
                },
                reasons: vec!["wall_ms_warn".to_string()],
            },
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
                status: MetricStatus::Fail,
            },
        );

        let lines = github_annotations(&compare);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.starts_with("::warning::")));
        assert!(lines.iter().any(|l| l.starts_with("::error::")));
    }
}
