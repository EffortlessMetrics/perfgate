//! Comment body rendering for GitHub PR comments.
//!
//! Produces rich Markdown that includes a verdict badge, metric comparison table,
//! trend indicators, optional blame attribution, and collapsible raw data.

use crate::client::COMMENT_MARKER;
use perfgate_render::{
    direction_str, format_metric_with_statistic, format_pct, format_value, metric_status_icon,
    render_reason_line,
};
use perfgate_types::{CompareReceipt, PerfgateReport, VerdictStatus};

/// Options for customizing the rendered comment.
#[derive(Debug, Clone, Default)]
pub struct CommentOptions {
    /// Optional blame text to include in the comment.
    pub blame_text: Option<String>,
    /// Optional explain text to include in the comment.
    pub explain_text: Option<String>,
}

/// Render a full PR comment body from a `CompareReceipt`.
pub fn render_comment(compare: &CompareReceipt, options: &CommentOptions) -> String {
    let mut out = String::new();

    // Marker for idempotent updates
    out.push_str(COMMENT_MARKER);
    out.push('\n');

    // Verdict header with badge
    out.push_str(&verdict_header(compare.verdict.status));
    out.push_str("\n\n");

    // Bench name
    out.push_str(&format!("**Bench:** `{}`\n\n", compare.bench.name));

    // Summary counts
    let counts = &compare.verdict.counts;
    out.push_str(&format!(
        "**Summary:** {} pass, {} warn, {} fail, {} skip\n\n",
        counts.pass, counts.warn, counts.fail, counts.skip
    ));

    // Metric comparison table with trend indicators
    out.push_str("| Metric | Baseline | Current | Delta | Trend | Budget | Status |\n");
    out.push_str("|--------|--------:|--------:|------:|:-----:|--------|--------|\n");

    for (metric, delta) in &compare.deltas {
        let budget = compare.budgets.get(metric);
        let (budget_str, direction_label) = if let Some(b) = budget {
            (
                format!("{:.1}%", b.threshold * 100.0),
                direction_str(b.direction),
            )
        } else {
            (String::new(), "")
        };

        let trend = trend_indicator(delta.pct);
        let status_icon = metric_status_icon(delta.status);

        out.push_str(&format!(
            "| `{metric}` | {b} {u} | {c} {u} | {pct} | {trend} | {budget} ({dir}) | {status} |\n",
            metric = format_metric_with_statistic(*metric, delta.statistic),
            b = format_value(*metric, delta.baseline),
            c = format_value(*metric, delta.current),
            u = metric.display_unit(),
            pct = format_pct(delta.pct),
            trend = trend,
            budget = budget_str,
            dir = direction_label,
            status = status_icon,
        ));
    }

    // Notes section
    if !compare.verdict.reasons.is_empty() {
        out.push_str("\n### Notes\n\n");
        for r in &compare.verdict.reasons {
            out.push_str(&render_reason_line(compare, r));
        }
    }

    // Blame attribution section
    if let Some(blame) = &options.blame_text {
        out.push_str("\n### Possible Causes\n\n");
        out.push_str(blame);
        out.push('\n');
    }

    // Explain section
    if let Some(explain) = &options.explain_text {
        out.push_str("\n### Diagnostic Hints\n\n");
        out.push_str(explain);
        out.push('\n');
    }

    // Collapsible raw data section
    out.push_str("\n<details>\n<summary>Raw comparison data</summary>\n\n");
    out.push_str("```json\n");
    if let Ok(json) = serde_json::to_string_pretty(compare) {
        out.push_str(&json);
    }
    out.push_str("\n```\n\n</details>\n");

    // Footer
    out.push_str("\n---\n");
    out.push_str("*Posted by [perfgate](https://github.com/EffortlessMetrics/perfgate)*\n");

    out
}

/// Render a full PR comment body from a `PerfgateReport`.
///
/// If the report contains a compare receipt, it delegates to `render_comment`.
/// Otherwise, it renders a minimal summary from the report's verdict and findings.
pub fn render_comment_from_report(report: &PerfgateReport, options: &CommentOptions) -> String {
    if let Some(compare) = &report.compare {
        return render_comment(compare, options);
    }

    // Minimal report when no compare receipt is available
    let mut out = String::new();

    out.push_str(COMMENT_MARKER);
    out.push('\n');
    out.push_str(&verdict_header(report.verdict.status));
    out.push_str("\n\n");

    out.push_str(&format!(
        "**Summary:** {} pass, {} warn, {} fail, {} skip\n\n",
        report.summary.pass_count,
        report.summary.warn_count,
        report.summary.fail_count,
        report.summary.skip_count,
    ));

    if !report.findings.is_empty() {
        out.push_str("### Findings\n\n");
        for finding in &report.findings {
            out.push_str(&format!(
                "- **{}** ({}): {}\n",
                finding.check_id,
                format!("{:?}", finding.severity).to_lowercase(),
                finding.message
            ));
        }
    }

    out.push_str("\n---\n");
    out.push_str("*Posted by [perfgate](https://github.com/EffortlessMetrics/perfgate)*\n");

    out
}

/// Generate a verdict header line with emoji badge.
fn verdict_header(status: VerdictStatus) -> String {
    match status {
        VerdictStatus::Pass => "## :white_check_mark: perfgate: **pass**".to_string(),
        VerdictStatus::Warn => "## :warning: perfgate: **warn**".to_string(),
        VerdictStatus::Fail => "## :x: perfgate: **fail**".to_string(),
        VerdictStatus::Skip => "## :fast_forward: perfgate: **skip**".to_string(),
    }
}

/// Generate a trend indicator with arrow and percentage.
///
/// - Positive changes (regression for lower-is-better): red up arrow
/// - Negative changes (improvement for lower-is-better): green down arrow
/// - Near zero: dash
fn trend_indicator(pct: f64) -> String {
    let abs_pct = (pct * 100.0).abs();
    if abs_pct < 0.5 {
        // Essentially flat
        return "\u{2014}".to_string(); // em dash
    }

    if pct > 0.0 {
        format!("\u{25B2} {:.1}%", abs_pct) // black up-pointing triangle
    } else {
        format!("\u{25BC} {:.1}%", abs_pct) // black down-pointing triangle
    }
}

/// Parse the `GITHUB_REPOSITORY` env var into `(owner, repo)`.
pub fn parse_github_repository(repo_str: &str) -> Option<(String, String)> {
    let (owner, repo) = repo_str.split_once('/')?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

/// Extract the PR number from a GitHub ref like `refs/pull/123/merge`.
pub fn parse_pr_number_from_ref(git_ref: &str) -> Option<u64> {
    let parts: Vec<&str> = git_ref.split('/').collect();
    if parts.len() >= 3 && parts[0] == "refs" && parts[1] == "pull" {
        parts[2].parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, Direction, Metric, MetricStatistic, MetricStatus,
        ToolInfo, Verdict, VerdictCounts,
    };
    use std::collections::BTreeMap;

    fn make_compare_receipt() -> CompareReceipt {
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
                status: MetricStatus::Warn,
            },
        );

        CompareReceipt {
            schema: perfgate_types::COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "my-bench".into(),
                cwd: None,
                command: vec!["true".into()],
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

    #[test]
    fn comment_contains_marker() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(body.contains(COMMENT_MARKER));
    }

    #[test]
    fn comment_contains_verdict_header() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(body.contains("perfgate: **warn**"));
    }

    #[test]
    fn comment_contains_bench_name() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(body.contains("`my-bench`"));
    }

    #[test]
    fn comment_contains_metric_table() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(body.contains("| Metric |"));
        assert!(body.contains("`wall_ms`"));
        assert!(body.contains("+15.00%"));
    }

    #[test]
    fn comment_contains_trend_indicator() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        // 15% regression should show up arrow
        assert!(body.contains("\u{25B2}"));
    }

    #[test]
    fn comment_contains_collapsible_raw_data() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(body.contains("<details>"));
        assert!(body.contains("Raw comparison data"));
        assert!(body.contains("</details>"));
    }

    #[test]
    fn comment_contains_blame_when_provided() {
        let receipt = make_compare_receipt();
        let options = CommentOptions {
            blame_text: Some("Dependency `serde` updated from 1.0 to 2.0".to_string()),
            explain_text: None,
        };
        let body = render_comment(&receipt, &options);
        assert!(body.contains("### Possible Causes"));
        assert!(body.contains("serde"));
    }

    #[test]
    fn comment_omits_blame_when_not_provided() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(!body.contains("### Possible Causes"));
    }

    #[test]
    fn comment_contains_footer() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(body.contains("Posted by [perfgate]"));
    }

    #[test]
    fn comment_contains_notes_section() {
        let receipt = make_compare_receipt();
        let body = render_comment(&receipt, &CommentOptions::default());
        assert!(body.contains("### Notes"));
        assert!(body.contains("wall_ms_warn"));
    }

    #[test]
    fn trend_indicator_flat() {
        let trend = trend_indicator(0.001); // 0.1%, below 0.5% threshold
        assert_eq!(trend, "\u{2014}");
    }

    #[test]
    fn trend_indicator_regression() {
        let trend = trend_indicator(0.15); // +15%
        assert!(trend.contains("\u{25B2}"));
        assert!(trend.contains("15.0%"));
    }

    #[test]
    fn trend_indicator_improvement() {
        let trend = trend_indicator(-0.10); // -10%
        assert!(trend.contains("\u{25BC}"));
        assert!(trend.contains("10.0%"));
    }

    #[test]
    fn parse_github_repository_valid() {
        let (owner, repo) = parse_github_repository("octocat/hello-world").unwrap();
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
    }

    #[test]
    fn parse_github_repository_invalid() {
        assert!(parse_github_repository("no-slash").is_none());
        assert!(parse_github_repository("/repo").is_none());
        assert!(parse_github_repository("owner/").is_none());
    }

    #[test]
    fn parse_pr_number_from_ref_valid() {
        assert_eq!(parse_pr_number_from_ref("refs/pull/123/merge"), Some(123));
        assert_eq!(parse_pr_number_from_ref("refs/pull/1/head"), Some(1));
    }

    #[test]
    fn parse_pr_number_from_ref_invalid() {
        assert!(parse_pr_number_from_ref("refs/heads/main").is_none());
        assert!(parse_pr_number_from_ref("refs/pull/abc/merge").is_none());
    }

    #[test]
    fn verdict_header_variants() {
        assert!(verdict_header(VerdictStatus::Pass).contains("pass"));
        assert!(verdict_header(VerdictStatus::Warn).contains("warn"));
        assert!(verdict_header(VerdictStatus::Fail).contains("fail"));
        assert!(verdict_header(VerdictStatus::Skip).contains("skip"));
    }

    #[test]
    fn render_comment_from_report_without_compare() {
        let report = PerfgateReport {
            report_type: "perfgate.report.v1".to_string(),
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
            compare: None,
            findings: vec![],
            summary: perfgate_types::ReportSummary {
                total_count: 1,
                pass_count: 1,
                warn_count: 0,
                fail_count: 0,
                skip_count: 0,
            },
        };

        let body = render_comment_from_report(&report, &CommentOptions::default());
        assert!(body.contains(COMMENT_MARKER));
        assert!(body.contains("perfgate: **pass**"));
        assert!(body.contains("1 pass"));
    }

    #[test]
    fn render_comment_from_report_with_compare() {
        let compare = make_compare_receipt();
        let report = PerfgateReport {
            report_type: "perfgate.report.v1".to_string(),
            verdict: compare.verdict.clone(),
            compare: Some(compare),
            findings: vec![],
            summary: perfgate_types::ReportSummary {
                total_count: 1,
                pass_count: 0,
                warn_count: 1,
                fail_count: 0,
                skip_count: 0,
            },
        };

        let body = render_comment_from_report(&report, &CommentOptions::default());
        assert!(body.contains("`wall_ms`"));
        assert!(body.contains("| Metric |"));
    }
}
