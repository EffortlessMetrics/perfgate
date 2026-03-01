//! Rendering utilities for perfgate output.
//!
//! This crate provides functions for rendering performance comparison results
//! as markdown tables and GitHub Actions annotations.
//!
//! # Example
//!
//! ```
//! use perfgate_render::{render_markdown, github_annotations};
//! use perfgate_types::{CompareReceipt, Delta, Metric, MetricStatus, MetricStatistic};
//! use std::collections::BTreeMap;
//!
//! fn example() {
//!     // Create a CompareReceipt (simplified example)
//!     // let compare = CompareReceipt { ... };
//!     // let markdown = render_markdown(&compare);
//!     // let annotations = github_annotations(&compare);
//! }
//! ```

use anyhow::Context;
use perfgate_types::{CompareReceipt, Direction, Metric, MetricStatistic, MetricStatus};
use serde_json::json;

/// Render a [`CompareReceipt`] as a Markdown table for PR comments.
///
/// ```
/// # use std::collections::BTreeMap;
/// # use perfgate_types::*;
/// let compare = CompareReceipt {
///     schema: COMPARE_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     bench: BenchMeta {
///         name: "my-bench".into(), cwd: None,
///         command: vec!["echo".into()], repeat: 3, warmup: 0,
///         work_units: None, timeout_ms: None,
///     },
///     baseline_ref: CompareRef { path: None, run_id: None },
///     current_ref: CompareRef { path: None, run_id: None },
///     budgets: BTreeMap::from([(Metric::WallMs, Budget {
///         threshold: 0.20, warn_threshold: 0.18, direction: Direction::Lower,
///     })]),
///     deltas: BTreeMap::from([(Metric::WallMs, Delta {
///         baseline: 100.0, current: 110.0, ratio: 1.1, pct: 0.1,
///         regression: 0.1, statistic: MetricStatistic::Median,
///         significance: None, status: MetricStatus::Pass,
///     })]),
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts { pass: 1, warn: 0, fail: 0 },
///         reasons: vec![],
///     },
/// };
/// let md = perfgate_render::render_markdown(&compare);
/// assert!(md.contains("✅ perfgate: pass"));
/// assert!(md.contains("wall_ms"));
/// ```
pub fn render_markdown(compare: &CompareReceipt) -> String {
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
///
/// # Examples
///
/// ```
/// # use std::collections::BTreeMap;
/// # use perfgate_types::*;
/// let compare = CompareReceipt {
///     schema: COMPARE_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     bench: BenchMeta {
///         name: "my-bench".into(), cwd: None,
///         command: vec!["echo".into()], repeat: 3, warmup: 0,
///         work_units: None, timeout_ms: None,
///     },
///     baseline_ref: CompareRef { path: None, run_id: None },
///     current_ref: CompareRef { path: None, run_id: None },
///     budgets: BTreeMap::new(),
///     deltas: BTreeMap::from([(Metric::WallMs, Delta {
///         baseline: 100.0, current: 110.0, ratio: 1.1, pct: 0.1,
///         regression: 0.1, statistic: MetricStatistic::Median,
///         significance: None, status: MetricStatus::Pass,
///     })]),
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts { pass: 1, warn: 0, fail: 0 },
///         reasons: vec![],
///     },
/// };
/// let rendered = perfgate_render::render_markdown_template(
///     &compare,
///     "Bench: {{bench.name}}, Verdict: {{verdict.status}}",
/// ).unwrap();
/// assert!(rendered.contains("Bench: my-bench"));
/// assert!(rendered.contains("Verdict: pass"));
/// ```
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
///
/// Only failing/warning metrics generate annotations; passing metrics are skipped.
///
/// ```
/// # use std::collections::BTreeMap;
/// # use perfgate_types::*;
/// let compare = CompareReceipt {
///     schema: COMPARE_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     bench: BenchMeta {
///         name: "my-bench".into(), cwd: None,
///         command: vec!["echo".into()], repeat: 3, warmup: 0,
///         work_units: None, timeout_ms: None,
///     },
///     baseline_ref: CompareRef { path: None, run_id: None },
///     current_ref: CompareRef { path: None, run_id: None },
///     budgets: BTreeMap::new(),
///     deltas: BTreeMap::from([(Metric::WallMs, Delta {
///         baseline: 100.0, current: 130.0, ratio: 1.3, pct: 0.3,
///         regression: 0.3, statistic: MetricStatistic::Median,
///         significance: None, status: MetricStatus::Fail,
///     })]),
///     verdict: Verdict {
///         status: VerdictStatus::Fail,
///         counts: VerdictCounts { pass: 0, warn: 0, fail: 1 },
///         reasons: vec![],
///     },
/// };
/// let annotations = perfgate_render::github_annotations(&compare);
/// assert_eq!(annotations.len(), 1);
/// assert!(annotations[0].starts_with("::error::"));
/// ```
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
///
/// # Examples
///
/// ```
/// use perfgate_types::Metric;
/// assert_eq!(perfgate_render::format_metric(Metric::WallMs), "wall_ms");
/// assert_eq!(perfgate_render::format_metric(Metric::MaxRssKb), "max_rss_kb");
/// ```
pub fn format_metric(metric: Metric) -> &'static str {
    metric.as_str()
}

/// Format a metric key, appending the statistic name when it is not the default (median).
///
/// # Examples
///
/// ```
/// use perfgate_types::{Metric, MetricStatistic};
/// assert_eq!(
///     perfgate_render::format_metric_with_statistic(Metric::WallMs, MetricStatistic::Median),
///     "wall_ms",
/// );
/// assert_eq!(
///     perfgate_render::format_metric_with_statistic(Metric::WallMs, MetricStatistic::P95),
///     "wall_ms (p95)",
/// );
/// ```
pub fn format_metric_with_statistic(metric: Metric, statistic: MetricStatistic) -> String {
    if statistic == MetricStatistic::Median {
        format_metric(metric).to_string()
    } else {
        format!("{} ({})", format_metric(metric), statistic.as_str())
    }
}

/// Build the JSON context object used by [`render_markdown_template`].
///
/// # Examples
///
/// ```
/// # use std::collections::BTreeMap;
/// # use perfgate_types::*;
/// let compare = CompareReceipt {
///     schema: COMPARE_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     bench: BenchMeta {
///         name: "my-bench".into(), cwd: None,
///         command: vec!["echo".into()], repeat: 1, warmup: 0,
///         work_units: None, timeout_ms: None,
///     },
///     baseline_ref: CompareRef { path: None, run_id: None },
///     current_ref: CompareRef { path: None, run_id: None },
///     budgets: BTreeMap::new(),
///     deltas: BTreeMap::new(),
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts { pass: 0, warn: 0, fail: 0 },
///         reasons: vec![],
///     },
/// };
/// let ctx = perfgate_render::markdown_template_context(&compare);
/// assert_eq!(ctx["header"], "✅ perfgate: pass");
/// assert!(ctx["rows"].as_array().unwrap().is_empty());
/// ```
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
///
/// Returns `None` for unrecognised metrics or non-warn/fail statuses.
///
/// # Examples
///
/// ```
/// use perfgate_types::{Metric, MetricStatus};
/// let (metric, status) = perfgate_render::parse_reason_token("wall_ms_warn").unwrap();
/// assert_eq!(metric, Metric::WallMs);
/// assert_eq!(status, MetricStatus::Warn);
///
/// assert!(perfgate_render::parse_reason_token("unknown_warn").is_none());
/// ```
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
///
/// # Examples
///
/// ```
/// # use std::collections::BTreeMap;
/// # use perfgate_types::*;
/// let compare = CompareReceipt {
///     schema: COMPARE_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     bench: BenchMeta {
///         name: "b".into(), cwd: None,
///         command: vec!["echo".into()], repeat: 1, warmup: 0,
///         work_units: None, timeout_ms: None,
///     },
///     baseline_ref: CompareRef { path: None, run_id: None },
///     current_ref: CompareRef { path: None, run_id: None },
///     budgets: BTreeMap::from([(Metric::WallMs, Budget {
///         threshold: 0.20, warn_threshold: 0.10, direction: Direction::Lower,
///     })]),
///     deltas: BTreeMap::from([(Metric::WallMs, Delta {
///         baseline: 100.0, current: 115.0, ratio: 1.15, pct: 0.15,
///         regression: 0.15, statistic: MetricStatistic::Median,
///         significance: None, status: MetricStatus::Warn,
///     })]),
///     verdict: Verdict {
///         status: VerdictStatus::Warn,
///         counts: VerdictCounts { pass: 0, warn: 1, fail: 0 },
///         reasons: vec!["wall_ms_warn".into()],
///     },
/// };
/// let line = perfgate_render::render_reason_line(&compare, "wall_ms_warn");
/// assert!(line.starts_with("- wall_ms_warn:"));
/// assert!(line.contains("+15.00%"));
/// ```
pub fn render_reason_line(compare: &CompareReceipt, token: &str) -> String {
    if let Some((metric, status)) = parse_reason_token(token)
        && let (Some(delta), Some(budget)) =
            (compare.deltas.get(&metric), compare.budgets.get(&metric))
    {
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

    format!("- {token}\n")
}

/// Format a metric value for display.
///
/// Integer metrics (wall_ms, max_rss_kb, …) are rounded; throughput uses 3 decimals.
///
/// ```
/// use perfgate_types::Metric;
/// assert_eq!(perfgate_render::format_value(Metric::WallMs, 123.4), "123");
/// assert_eq!(perfgate_render::format_value(Metric::ThroughputPerS, 1.5), "1.500");
/// assert_eq!(perfgate_render::format_value(Metric::MaxRssKb, 2048.0), "2048");
/// ```
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
///
/// ```
/// assert_eq!(perfgate_render::format_pct(0.1), "+10.00%");
/// assert_eq!(perfgate_render::format_pct(-0.05), "-5.00%");
/// assert_eq!(perfgate_render::format_pct(0.0), "0.00%");
/// ```
pub fn format_pct(pct: f64) -> String {
    let sign = if pct > 0.0 { "+" } else { "" };
    format!("{}{:.2}%", sign, pct * 100.0)
}

/// Return a human-readable label for a budget [`Direction`].
///
/// # Examples
///
/// ```
/// use perfgate_types::Direction;
/// assert_eq!(perfgate_render::direction_str(Direction::Lower), "lower");
/// assert_eq!(perfgate_render::direction_str(Direction::Higher), "higher");
/// ```
pub fn direction_str(direction: Direction) -> &'static str {
    match direction {
        Direction::Lower => "lower",
        Direction::Higher => "higher",
    }
}

/// Return an emoji icon for a [`MetricStatus`].
///
/// # Examples
///
/// ```
/// use perfgate_types::MetricStatus;
/// assert_eq!(perfgate_render::metric_status_icon(MetricStatus::Pass), "✅");
/// assert_eq!(perfgate_render::metric_status_icon(MetricStatus::Warn), "⚠️");
/// assert_eq!(perfgate_render::metric_status_icon(MetricStatus::Fail), "❌");
/// ```
pub fn metric_status_icon(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Pass => "✅",
        MetricStatus::Warn => "⚠️",
        MetricStatus::Fail => "❌",
    }
}

/// Return a lowercase string label for a [`MetricStatus`].
///
/// # Examples
///
/// ```
/// use perfgate_types::MetricStatus;
/// assert_eq!(perfgate_render::metric_status_str(MetricStatus::Pass), "pass");
/// assert_eq!(perfgate_render::metric_status_str(MetricStatus::Warn), "warn");
/// assert_eq!(perfgate_render::metric_status_str(MetricStatus::Fail), "fail");
/// ```
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
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.18,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 1000.0,
                current: 1100.0,
                ratio: 1.1,
                pct: 0.1,
                regression: 0.1,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );

        let compare = CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "demo".into(),
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
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                },
                reasons: vec![],
            },
        };

        let md = render_markdown(&compare);
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
    fn markdown_template_strict_mode_rejects_unknown_fields() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let err = render_markdown_template(&compare, "{{does_not_exist}}").unwrap_err();
        assert!(
            err.to_string().contains("render markdown template"),
            "unexpected error: {}",
            err
        );
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
    fn render_reason_line_formats_thresholds() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let line = render_reason_line(&compare, "wall_ms_warn");
        assert!(line.contains("warn >="));
        assert!(line.contains("fail >"));
        assert!(line.contains("+15.00%"));
    }

    #[test]
    fn render_reason_line_falls_back_when_missing_budget() {
        let mut compare = make_compare_receipt(MetricStatus::Warn);
        compare.budgets.clear();
        let line = render_reason_line(&compare, "wall_ms_warn");
        assert_eq!(line, "- wall_ms_warn\n");
    }

    #[test]
    fn format_value_and_pct_render_expected_strings() {
        assert_eq!(format_value(Metric::ThroughputPerS, 1.23456), "1.235");
        assert_eq!(format_value(Metric::WallMs, 123.0), "123");
        assert_eq!(format_pct(0.1), "+10.00%");
        assert_eq!(format_pct(-0.1), "-10.00%");
        assert_eq!(format_pct(0.0), "0.00%");
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
        compare.deltas.insert(
            Metric::ThroughputPerS,
            Delta {
                baseline: 100.0,
                current: 90.0,
                ratio: 0.9,
                pct: -0.1,
                regression: 0.0,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );

        let lines = github_annotations(&compare);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().any(|l| l.starts_with("::warning::")));
        assert!(lines.iter().any(|l| l.starts_with("::error::")));
        assert!(lines.iter().all(|l| !l.contains("throughput_per_s")));
    }

    #[test]
    fn format_metric_with_statistic_displays_correctly() {
        assert_eq!(
            format_metric_with_statistic(Metric::WallMs, MetricStatistic::Median),
            "wall_ms"
        );
        assert_eq!(
            format_metric_with_statistic(Metric::WallMs, MetricStatistic::P95),
            "wall_ms (p95)"
        );
    }

    #[test]
    fn direction_str_returns_correct_strings() {
        assert_eq!(direction_str(Direction::Lower), "lower");
        assert_eq!(direction_str(Direction::Higher), "higher");
    }

    #[test]
    fn metric_status_str_returns_correct_strings() {
        assert_eq!(metric_status_str(MetricStatus::Pass), "pass");
        assert_eq!(metric_status_str(MetricStatus::Warn), "warn");
        assert_eq!(metric_status_str(MetricStatus::Fail), "fail");
    }

    #[test]
    fn metric_status_icon_returns_correct_emojis() {
        assert_eq!(metric_status_icon(MetricStatus::Pass), "✅");
        assert_eq!(metric_status_icon(MetricStatus::Warn), "⚠️");
        assert_eq!(metric_status_icon(MetricStatus::Fail), "❌");
    }

    #[test]
    fn snapshot_markdown_rendering() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let md = render_markdown(&compare);
        insta::assert_snapshot!(md, @r###"
        ⚠️ perfgate: warn

        **Bench:** `bench`

        | metric | baseline (median) | current (median) | delta | budget | status |
        |---|---:|---:|---:|---:|---|
        | `wall_ms` | 100 ms | 115 ms | +15.00% | 20.0% (lower) | ⚠️ |

        **Notes:**
        - wall_ms_warn: +15.00% (warn >= 10.00%, fail > 20.00%)
        "###);
    }

    #[test]
    fn template_custom_basic_variables() {
        let compare = make_compare_receipt(MetricStatus::Pass);
        let template = "Verdict: {{verdict.status}}\nBench: {{bench.name}}\nHeader: {{header}}";
        let rendered = render_markdown_template(&compare, template).expect("basic variables");
        assert!(rendered.contains("Bench: bench"));
        assert!(rendered.contains("Header:"));
    }

    #[test]
    fn template_missing_variable_returns_error() {
        let compare = make_compare_receipt(MetricStatus::Pass);
        let result = render_markdown_template(&compare, "{{nonexistent_var}}");
        assert!(
            result.is_err(),
            "strict mode should reject missing variables"
        );
    }

    #[test]
    fn template_empty_deltas_renders_no_rows() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.deltas.clear();
        compare.budgets.clear();
        let template = "rows:{{#each rows}}[{{metric}}]{{/each}}end";
        let rendered = render_markdown_template(&compare, template).expect("empty data");
        assert_eq!(rendered, "rows:end");
    }

    #[test]
    fn template_conditional_verdict_pass() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.verdict.status = VerdictStatus::Pass;
        // Handlebars doesn't have built-in `eq` helper, so use string comparison approach
        let template = "{{verdict.status}}";
        let rendered = render_markdown_template(&compare, template).expect("verdict pass");
        assert_eq!(rendered, "pass");
    }

    #[test]
    fn template_conditional_verdict_warn() {
        let mut compare = make_compare_receipt(MetricStatus::Warn);
        compare.verdict.status = VerdictStatus::Warn;
        let template = "status={{verdict.status}}";
        let rendered = render_markdown_template(&compare, template).expect("verdict warn");
        assert_eq!(rendered, "status=warn");
    }

    #[test]
    fn template_conditional_verdict_fail() {
        let mut compare = make_compare_receipt(MetricStatus::Fail);
        compare.verdict.status = VerdictStatus::Fail;
        let template = "{{#if verdict.reasons}}REASONS:{{#each verdict.reasons}}{{this}},{{/each}}{{else}}NO_REASONS{{/if}}";
        let rendered = render_markdown_template(&compare, template).expect("verdict fail");
        assert!(rendered.contains("REASONS:"));
        assert!(rendered.contains("wall_ms_warn"));
    }

    #[test]
    fn template_conditional_on_rows_status() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        // Handlebars without custom helpers - use simpler approach
        let template = "{{#each rows}}{{status_icon}} {{metric}} is {{status}}\n{{/each}}";
        let rendered = render_markdown_template(&compare, template).expect("row status");
        assert!(rendered.contains("wall_ms is warn"));
        assert!(rendered.contains("⚠️"));
    }

    #[test]
    fn snapshot_github_annotations() {
        let mut compare = make_compare_receipt(MetricStatus::Fail);
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
                status: MetricStatus::Warn,
            },
        );
        let annotations = github_annotations(&compare);
        insta::assert_debug_snapshot!(annotations, @r###"
        [
            "::warning::perfgate bench max_rss_kb: +50.00% (baseline 100KB, current 150KB)",
            "::error::perfgate bench wall_ms: +15.00% (baseline 100ms, current 115ms)",
        ]
        "###);
    }

    // ── Template rendering error paths ──────────────────────────────────

    #[test]
    fn template_invalid_syntax_returns_error() {
        let compare = make_compare_receipt(MetricStatus::Pass);
        let result = render_markdown_template(&compare, "{{#if}}unclosed");
        assert!(result.is_err(), "malformed template should fail to parse");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("parse markdown template"),
        );
    }

    #[test]
    fn template_nested_missing_variable_returns_error() {
        let compare = make_compare_receipt(MetricStatus::Pass);
        let result = render_markdown_template(&compare, "{{bench.nonexistent_field}}");
        assert!(
            result.is_err(),
            "strict mode should reject missing nested variable"
        );
    }

    #[test]
    fn snapshot_markdown_empty_deltas() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.deltas.clear();
        compare.budgets.clear();
        compare.verdict.reasons.clear();
        compare.verdict.status = perfgate_types::VerdictStatus::Pass;
        compare.verdict.counts = VerdictCounts {
            pass: 0,
            warn: 0,
            fail: 0,
        };
        let md = render_markdown(&compare);
        insta::assert_snapshot!(md, @r###"
        ✅ perfgate: pass

        **Bench:** `bench`

        | metric | baseline (median) | current (median) | delta | budget | status |
        |---|---:|---:|---:|---:|---|
        "###);
    }

    #[test]
    fn github_annotations_empty_deltas() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.deltas.clear();
        let annotations = github_annotations(&compare);
        assert!(annotations.is_empty());
    }

    #[test]
    fn snapshot_markdown_all_passing_multi_metric() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.verdict.status = perfgate_types::VerdictStatus::Pass;
        compare.verdict.reasons.clear();
        compare.verdict.counts = VerdictCounts {
            pass: 3,
            warn: 0,
            fail: 0,
        };
        compare.budgets.insert(
            Metric::MaxRssKb,
            Budget {
                threshold: 0.3,
                warn_threshold: 0.2,
                direction: Direction::Lower,
            },
        );
        compare.budgets.insert(
            Metric::ThroughputPerS,
            Budget {
                threshold: 0.15,
                warn_threshold: 0.1,
                direction: Direction::Higher,
            },
        );
        compare.deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 500.0,
                current: 490.0,
                ratio: 0.98,
                pct: -0.02,
                regression: 0.0,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        compare.deltas.insert(
            Metric::ThroughputPerS,
            Delta {
                baseline: 50.0,
                current: 52.0,
                ratio: 1.04,
                pct: 0.04,
                regression: 0.0,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        let md = render_markdown(&compare);
        // All passing → no Notes section
        assert!(!md.contains("**Notes:**"));
        insta::assert_snapshot!(md, @r###"
        ✅ perfgate: pass

        **Bench:** `bench`

        | metric | baseline (median) | current (median) | delta | budget | status |
        |---|---:|---:|---:|---:|---|
        | `max_rss_kb` | 500 KB | 490 KB | -2.00% | 30.0% (lower) | ✅ |
        | `throughput_per_s` | 50.000 /s | 52.000 /s | +4.00% | 15.0% (higher) | ✅ |
        | `wall_ms` | 100 ms | 115 ms | +15.00% | 20.0% (lower) | ✅ |
        "###);
    }

    #[test]
    fn github_annotations_all_passing_yields_none() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 100.0,
                current: 95.0,
                ratio: 0.95,
                pct: -0.05,
                regression: 0.0,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        let annotations = github_annotations(&compare);
        assert!(
            annotations.is_empty(),
            "all-pass should produce no annotations"
        );
    }

    // ── render_reason_line edge cases ───────────────────────────────────

    #[test]
    fn render_reason_line_empty_string() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let line = render_reason_line(&compare, "");
        assert_eq!(line, "- \n");
    }

    #[test]
    fn render_reason_line_no_underscore() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let line = render_reason_line(&compare, "nounderscore");
        assert_eq!(line, "- nounderscore\n");
    }

    #[test]
    fn render_reason_line_single_underscore() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let line = render_reason_line(&compare, "_warn");
        // metric_part="" which won't parse → fallback
        assert_eq!(line, "- _warn\n");
    }

    #[test]
    fn render_reason_line_very_long_token() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let long_token = format!("{}_warn", "a".repeat(500));
        let line = render_reason_line(&compare, &long_token);
        // Unknown metric → fallback
        assert!(line.starts_with("- "));
        assert!(line.contains(&"a".repeat(500)));
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn render_reason_line_fail_status() {
        let compare = make_compare_receipt(MetricStatus::Fail);
        let line = render_reason_line(&compare, "wall_ms_fail");
        assert!(line.contains("fail >"));
        assert!(line.contains("+15.00%"));
    }

    // ── parse_reason_token edge cases ───────────────────────────────────

    #[test]
    fn parse_reason_token_empty_string() {
        assert!(parse_reason_token("").is_none());
    }

    #[test]
    fn parse_reason_token_only_underscores() {
        assert!(parse_reason_token("___").is_none());
    }

    #[test]
    fn parse_reason_token_valid_fail() {
        let result = parse_reason_token("max_rss_kb_fail");
        assert!(result.is_some());
        let (metric, status) = result.unwrap();
        assert_eq!(metric, Metric::MaxRssKb);
        assert_eq!(status, MetricStatus::Fail);
    }

    #[test]
    fn parse_reason_token_trailing_underscore() {
        // "wall_ms_" → status_part="" → None
        assert!(parse_reason_token("wall_ms_").is_none());
    }

    // ── Template rendering with special metric values ───────────────────

    #[test]
    fn template_renders_extreme_pct_values() {
        let mut compare = make_compare_receipt(MetricStatus::Fail);
        compare.deltas.get_mut(&Metric::WallMs).unwrap().pct = 99.99;
        let template = "{{#each rows}}{{delta_pct}}{{/each}}";
        let rendered = render_markdown_template(&compare, template).expect("extreme pct");
        assert!(rendered.contains("+9999.00%"));
    }

    #[test]
    fn template_renders_negative_pct() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.deltas.get_mut(&Metric::WallMs).unwrap().pct = -0.5;
        let template = "{{#each rows}}{{delta_pct}}{{/each}}";
        let rendered = render_markdown_template(&compare, template).expect("negative pct");
        assert!(rendered.contains("-50.00%"));
    }

    #[test]
    fn template_renders_zero_baseline_values() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        let d = compare.deltas.get_mut(&Metric::WallMs).unwrap();
        d.baseline = 0.0;
        d.current = 0.0;
        d.pct = 0.0;
        let template = "{{#each rows}}b={{baseline}} c={{current}} d={{delta_pct}}{{/each}}";
        let rendered = render_markdown_template(&compare, template).expect("zero values");
        assert!(rendered.contains("b=0"));
        assert!(rendered.contains("c=0"));
        assert!(rendered.contains("d=0.00%"));
    }

    #[test]
    fn template_renders_raw_fields() {
        let compare = make_compare_receipt(MetricStatus::Warn);
        let template = "{{#each rows}}raw_pct={{raw.pct}} sig={{raw.significance}}{{/each}}";
        let rendered = render_markdown_template(&compare, template).expect("raw fields");
        assert!(rendered.contains("raw_pct=0.15"));
        assert!(rendered.contains("sig="));
    }

    #[test]
    fn template_empty_string_renders_empty() {
        let compare = make_compare_receipt(MetricStatus::Pass);
        let rendered = render_markdown_template(&compare, "").expect("empty template");
        assert_eq!(rendered, "");
    }

    #[test]
    fn template_literal_text_only() {
        let compare = make_compare_receipt(MetricStatus::Pass);
        let rendered =
            render_markdown_template(&compare, "just literal text").expect("literal template");
        assert_eq!(rendered, "just literal text");
    }

    #[test]
    fn snapshot_markdown_template_context_empty_deltas() {
        let mut compare = make_compare_receipt(MetricStatus::Pass);
        compare.deltas.clear();
        compare.budgets.clear();
        compare.verdict.reasons.clear();
        let ctx = markdown_template_context(&compare);
        let rows = ctx["rows"].as_array().unwrap();
        assert!(rows.is_empty());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
    };
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    fn non_empty_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,20}".prop_map(|s| s)
    }

    fn tool_info_strategy() -> impl Strategy<Value = ToolInfo> {
        (non_empty_string(), non_empty_string())
            .prop_map(|(name, version)| ToolInfo { name, version })
    }

    fn bench_meta_strategy() -> impl Strategy<Value = BenchMeta> {
        (
            non_empty_string(),
            proptest::option::of(non_empty_string()),
            proptest::collection::vec(non_empty_string(), 1..5),
            1u32..100,
            0u32..10,
            proptest::option::of(1u64..10000),
            proptest::option::of(100u64..60000),
        )
            .prop_map(
                |(name, cwd, command, repeat, warmup, work_units, timeout_ms)| BenchMeta {
                    name,
                    cwd,
                    command,
                    repeat,
                    warmup,
                    work_units,
                    timeout_ms,
                },
            )
    }

    fn compare_ref_strategy() -> impl Strategy<Value = CompareRef> {
        (
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
        )
            .prop_map(|(path, run_id)| CompareRef { path, run_id })
    }

    fn direction_strategy() -> impl Strategy<Value = Direction> {
        prop_oneof![Just(Direction::Lower), Just(Direction::Higher),]
    }

    fn budget_strategy() -> impl Strategy<Value = Budget> {
        (0.01f64..1.0, 0.01f64..1.0, direction_strategy()).prop_map(
            |(threshold, warn_factor, direction)| {
                let warn_threshold = threshold * warn_factor;
                Budget {
                    threshold,
                    warn_threshold,
                    direction,
                }
            },
        )
    }

    fn metric_status_strategy() -> impl Strategy<Value = MetricStatus> {
        prop_oneof![
            Just(MetricStatus::Pass),
            Just(MetricStatus::Warn),
            Just(MetricStatus::Fail),
        ]
    }

    fn delta_strategy() -> impl Strategy<Value = Delta> {
        (0.1f64..10000.0, 0.1f64..10000.0, metric_status_strategy()).prop_map(
            |(baseline, current, status)| {
                let ratio = current / baseline;
                let pct = (current - baseline) / baseline;
                let regression = if pct > 0.0 { pct } else { 0.0 };
                Delta {
                    baseline,
                    current,
                    ratio,
                    pct,
                    regression,
                    statistic: MetricStatistic::Median,
                    significance: None,
                    status,
                }
            },
        )
    }

    fn verdict_status_strategy() -> impl Strategy<Value = VerdictStatus> {
        prop_oneof![
            Just(VerdictStatus::Pass),
            Just(VerdictStatus::Warn),
            Just(VerdictStatus::Fail),
        ]
    }

    fn verdict_counts_strategy() -> impl Strategy<Value = VerdictCounts> {
        (0u32..10, 0u32..10, 0u32..10).prop_map(|(pass, warn, fail)| VerdictCounts {
            pass,
            warn,
            fail,
        })
    }

    fn verdict_strategy() -> impl Strategy<Value = Verdict> {
        (
            verdict_status_strategy(),
            verdict_counts_strategy(),
            proptest::collection::vec("[a-zA-Z0-9 ]{1,50}", 0..5),
        )
            .prop_map(|(status, counts, reasons)| Verdict {
                status,
                counts,
                reasons,
            })
    }

    fn metric_strategy() -> impl Strategy<Value = Metric> {
        prop_oneof![
            Just(Metric::BinaryBytes),
            Just(Metric::CpuMs),
            Just(Metric::CtxSwitches),
            Just(Metric::WallMs),
            Just(Metric::MaxRssKb),
            Just(Metric::PageFaults),
            Just(Metric::ThroughputPerS),
        ]
    }

    fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
        proptest::collection::btree_map(metric_strategy(), budget_strategy(), 0..8)
    }

    fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
        proptest::collection::btree_map(metric_strategy(), delta_strategy(), 0..8)
    }

    fn compare_receipt_strategy() -> impl Strategy<Value = CompareReceipt> {
        (
            tool_info_strategy(),
            bench_meta_strategy(),
            compare_ref_strategy(),
            compare_ref_strategy(),
            budgets_map_strategy(),
            deltas_map_strategy(),
            verdict_strategy(),
        )
            .prop_map(
                |(tool, bench, baseline_ref, current_ref, budgets, deltas, verdict)| {
                    CompareReceipt {
                        schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
                        tool,
                        bench,
                        baseline_ref,
                        current_ref,
                        budgets,
                        deltas,
                        verdict,
                    }
                },
            )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn markdown_rendering_completeness(receipt in compare_receipt_strategy()) {
            let md = render_markdown(&receipt);

            let expected_emoji = match receipt.verdict.status {
                VerdictStatus::Pass => "✅",
                VerdictStatus::Warn => "⚠️",
                VerdictStatus::Fail => "❌",
            };
            prop_assert!(
                md.contains(expected_emoji),
                "Markdown should contain verdict emoji '{}' for status {:?}. Got:\n{}",
                expected_emoji,
                receipt.verdict.status,
                md
            );

            let expected_status_word = match receipt.verdict.status {
                VerdictStatus::Pass => "pass",
                VerdictStatus::Warn => "warn",
                VerdictStatus::Fail => "fail",
            };
            prop_assert!(
                md.contains(expected_status_word),
                "Markdown should contain status word '{}'. Got:\n{}",
                expected_status_word,
                md
            );

            prop_assert!(
                md.contains(&receipt.bench.name),
                "Markdown should contain benchmark name '{}'. Got:\n{}",
                receipt.bench.name,
                md
            );

            prop_assert!(
                md.contains("| metric |"),
                "Markdown should contain table header. Got:\n{}",
                md
            );

            for metric in receipt.deltas.keys() {
                let metric_name = metric.as_str();
                prop_assert!(
                    md.contains(metric_name),
                    "Markdown should contain metric '{}'. Got:\n{}",
                    metric_name,
                    md
                );
            }

            for reason in &receipt.verdict.reasons {
                prop_assert!(
                    md.contains(reason),
                    "Markdown should contain verdict reason '{}'. Got:\n{}",
                    reason,
                    md
                );
            }

            if !receipt.verdict.reasons.is_empty() {
                prop_assert!(
                    md.contains("**Notes:**"),
                    "Markdown should contain Notes section when there are reasons. Got:\n{}",
                    md
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn github_annotation_generation(receipt in compare_receipt_strategy()) {
            let annotations = github_annotations(&receipt);

            let expected_fail_count = receipt.deltas.values()
                .filter(|d| d.status == MetricStatus::Fail)
                .count();
            let expected_warn_count = receipt.deltas.values()
                .filter(|d| d.status == MetricStatus::Warn)
                .count();
            let expected_pass_count = receipt.deltas.values()
                .filter(|d| d.status == MetricStatus::Pass)
                .count();

            let actual_error_count = annotations.iter()
                .filter(|a| a.starts_with("::error::"))
                .count();
            let actual_warning_count = annotations.iter()
                .filter(|a| a.starts_with("::warning::"))
                .count();

            prop_assert_eq!(
                actual_error_count,
                expected_fail_count,
                "Expected {} ::error:: annotations for {} Fail metrics, got {}. Annotations: {:?}",
                expected_fail_count,
                expected_fail_count,
                actual_error_count,
                annotations
            );

            prop_assert_eq!(
                actual_warning_count,
                expected_warn_count,
                "Expected {} ::warning:: annotations for {} Warn metrics, got {}. Annotations: {:?}",
                expected_warn_count,
                expected_warn_count,
                actual_warning_count,
                annotations
            );

            let total_annotations = annotations.len();
            let expected_total = expected_fail_count + expected_warn_count;
            prop_assert_eq!(
                total_annotations,
                expected_total,
                "Expected {} total annotations (fail: {}, warn: {}, pass: {} should produce none), got {}. Annotations: {:?}",
                expected_total,
                expected_fail_count,
                expected_warn_count,
                expected_pass_count,
                total_annotations,
                annotations
            );

            for (metric, delta) in &receipt.deltas {
                if delta.status == MetricStatus::Pass {
                    continue;
                }

                let metric_name = metric.as_str();
                let matching_annotation = annotations.iter().find(|a| a.contains(metric_name));

                prop_assert!(
                    matching_annotation.is_some(),
                    "Expected annotation for metric '{}' with status {:?}. Annotations: {:?}",
                    metric_name,
                    delta.status,
                    annotations
                );

                let annotation = matching_annotation.unwrap();

                prop_assert!(
                    annotation.contains(&receipt.bench.name),
                    "Annotation should contain bench name '{}'. Got: {}",
                    receipt.bench.name,
                    annotation
                );

                prop_assert!(
                    annotation.contains(metric_name),
                    "Annotation should contain metric name '{}'. Got: {}",
                    metric_name,
                    annotation
                );

                let pct_str = format_pct(delta.pct);
                prop_assert!(
                    annotation.contains(&pct_str),
                    "Annotation should contain delta percentage '{}'. Got: {}",
                    pct_str,
                    annotation
                );

                match delta.status {
                    MetricStatus::Fail => {
                        prop_assert!(
                            annotation.starts_with("::error::"),
                            "Fail metric should produce ::error:: annotation. Got: {}",
                            annotation
                        );
                    }
                    MetricStatus::Warn => {
                        prop_assert!(
                            annotation.starts_with("::warning::"),
                            "Warn metric should produce ::warning:: annotation. Got: {}",
                            annotation
                        );
                    }
                    MetricStatus::Pass => unreachable!(),
                }
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Same CompareReceipt always produces identical markdown output.
        #[test]
        fn markdown_rendering_determinism(receipt in compare_receipt_strategy()) {
            let md1 = render_markdown(&receipt);
            let md2 = render_markdown(&receipt);
            prop_assert_eq!(
                md1, md2,
                "render_markdown must be deterministic"
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Every annotation string starts with a valid GitHub Actions level prefix.
        #[test]
        fn annotation_valid_levels(receipt in compare_receipt_strategy()) {
            let annotations = github_annotations(&receipt);
            for annotation in &annotations {
                prop_assert!(
                    annotation.starts_with("::error::")
                        || annotation.starts_with("::warning::")
                        || annotation.starts_with("::notice::"),
                    "Annotation should start with a valid level (::error::, ::warning::, ::notice::). Got: {}",
                    annotation
                );
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Rendering an HTML-like template produces output with valid structure tags.
        #[test]
        fn template_html_structure(receipt in compare_receipt_strategy()) {
            let template = concat!(
                "<html><body>",
                "<h1>{{header}}</h1>",
                "<p>Bench: {{bench.name}}</p>",
                "<table>",
                "{{#each rows}}<tr><td>{{metric}}</td><td>{{status}}</td></tr>{{/each}}",
                "</table>",
                "</body></html>",
            );
            let rendered = render_markdown_template(&receipt, template)
                .expect("HTML template should render");

            prop_assert!(rendered.contains("<html>"), "Missing <html> tag. Got:\n{}", rendered);
            prop_assert!(rendered.contains("</html>"), "Missing </html> tag. Got:\n{}", rendered);
            prop_assert!(rendered.contains("<body>"), "Missing <body> tag. Got:\n{}", rendered);
            prop_assert!(rendered.contains("</body>"), "Missing </body> tag. Got:\n{}", rendered);
            prop_assert!(rendered.contains("<table>"), "Missing <table> tag. Got:\n{}", rendered);
            prop_assert!(rendered.contains("</table>"), "Missing </table> tag. Got:\n{}", rendered);
            prop_assert!(
                rendered.contains(&receipt.bench.name),
                "HTML should contain bench name '{}'. Got:\n{}",
                receipt.bench.name,
                rendered
            );

            for metric in receipt.deltas.keys() {
                let metric_name = metric.as_str();
                prop_assert!(
                    rendered.contains(metric_name),
                    "HTML should contain metric '{}'. Got:\n{}",
                    metric_name,
                    rendered
                );
            }
        }
    }
}
