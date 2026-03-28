//! SVG badge generation for perfgate performance status.
//!
//! Generates shields.io-compatible SVG badges showing performance status,
//! individual metric values, or trend summaries from perfgate reports and
//! comparisons.

use perfgate_render::{format_metric_with_statistic, format_pct, format_value};
use perfgate_types::{CompareReceipt, Metric, MetricStatus, PerfgateReport, VerdictStatus};
use std::path::PathBuf;

// ── Badge colours (shields.io palette) ───────────────────────────

const COLOR_PASS: &str = "#4c1";
const COLOR_WARN: &str = "#dfb317";
const COLOR_FAIL: &str = "#e05d44";
const COLOR_SKIP: &str = "#9f9f9f";

// ── Public types ─────────────────────────────────────────────────

/// Which kind of badge to generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeType {
    /// Overall verdict badge, e.g. "performance | passing".
    Status,
    /// Single-metric badge, e.g. "wall_ms | 142 ms (+3.20%)".
    Metric,
    /// Trend badge, e.g. "perf trend | stable".
    Trend,
}

/// Visual style of the badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadgeStyle {
    /// Rounded ends (default shields.io style).
    #[default]
    Flat,
    /// Square ends.
    FlatSquare,
}

/// Everything needed to produce a badge.
#[derive(Debug, Clone)]
pub struct BadgeRequest {
    /// Path to a compare receipt **or** report receipt.
    pub input_path: PathBuf,
    /// What kind of badge to produce.
    pub badge_type: BadgeType,
    /// Visual style.
    pub style: BadgeStyle,
    /// When `badge_type == Metric`, which metric to render.
    pub metric: Option<String>,
    /// Where to write the SVG. `None` means stdout.
    pub output_path: Option<PathBuf>,
}

/// Result of badge generation.
#[derive(Debug, Clone)]
pub struct BadgeOutcome {
    /// The generated SVG string.
    pub svg: String,
}

/// Intermediate representation of a badge, before SVG rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct Badge {
    pub label: String,
    pub message: String,
    pub color: String,
    pub style: BadgeStyle,
}

// ── Use case ─────────────────────────────────────────────────────

pub struct BadgeUseCase;

/// Input that has been parsed from a file — either a compare receipt or a
/// report.
pub enum BadgeInput {
    Compare(CompareReceipt),
    Report(PerfgateReport),
}

impl BadgeUseCase {
    /// Build a badge from an already-parsed input.
    pub fn execute(
        &self,
        input: &BadgeInput,
        badge_type: BadgeType,
        style: BadgeStyle,
        metric_name: Option<&str>,
    ) -> anyhow::Result<BadgeOutcome> {
        let badge = match badge_type {
            BadgeType::Status => status_badge(input, style),
            BadgeType::Metric => {
                let name = metric_name.ok_or_else(|| {
                    anyhow::anyhow!("--metric is required when --type metric is used")
                })?;
                metric_badge(input, style, name)?
            }
            BadgeType::Trend => trend_badge(input, style),
        };
        Ok(BadgeOutcome {
            svg: render_svg(&badge),
        })
    }
}

// ── Badge builders ───────────────────────────────────────────────

fn status_badge(input: &BadgeInput, style: BadgeStyle) -> Badge {
    let (status_label, color) = match input {
        BadgeInput::Compare(c) => verdict_status_label_color(c.verdict.status),
        BadgeInput::Report(r) => verdict_status_label_color(r.verdict.status),
    };
    Badge {
        label: "performance".to_string(),
        message: status_label.to_string(),
        color: color.to_string(),
        style,
    }
}

fn metric_badge(input: &BadgeInput, style: BadgeStyle, metric_name: &str) -> anyhow::Result<Badge> {
    let compare = match input {
        BadgeInput::Compare(c) => c,
        BadgeInput::Report(r) => r
            .compare
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("report has no compare receipt (no baseline?)"))?,
    };

    let metric = Metric::parse_key(metric_name)
        .ok_or_else(|| anyhow::anyhow!("unknown metric: {metric_name}"))?;

    let delta = compare
        .deltas
        .get(&metric)
        .ok_or_else(|| anyhow::anyhow!("metric {metric_name} not found in deltas"))?;

    let label = format_metric_with_statistic(metric, delta.statistic);
    let value_str = format_value(metric, delta.current);
    let unit = metric.display_unit();
    let pct = format_pct(delta.pct);
    let message = format!("{value_str} {unit} ({pct})");
    let color = metric_status_color(delta.status).to_string();

    Ok(Badge {
        label,
        message,
        color,
        style,
    })
}

fn trend_badge(input: &BadgeInput, style: BadgeStyle) -> Badge {
    let compare = match input {
        BadgeInput::Compare(c) => Some(c),
        BadgeInput::Report(r) => r.compare.as_ref(),
    };

    let (trend_label, color) = match compare {
        Some(c) => {
            let worst = worst_metric_status(c);
            match worst {
                MetricStatus::Pass => ("stable", COLOR_PASS),
                MetricStatus::Warn => ("degraded", COLOR_WARN),
                MetricStatus::Fail => ("regressed", COLOR_FAIL),
                MetricStatus::Skip => ("unknown", COLOR_SKIP),
            }
        }
        None => ("unknown", COLOR_SKIP),
    };

    Badge {
        label: "perf trend".to_string(),
        message: trend_label.to_string(),
        color: color.to_string(),
        style,
    }
}

// ── Helpers ──────────────────────────────────────────────────────

fn verdict_status_label_color(status: VerdictStatus) -> (&'static str, &'static str) {
    match status {
        VerdictStatus::Pass => ("passing", COLOR_PASS),
        VerdictStatus::Warn => ("warning", COLOR_WARN),
        VerdictStatus::Fail => ("failing", COLOR_FAIL),
        VerdictStatus::Skip => ("skipped", COLOR_SKIP),
    }
}

fn metric_status_color(status: MetricStatus) -> &'static str {
    match status {
        MetricStatus::Pass => COLOR_PASS,
        MetricStatus::Warn => COLOR_WARN,
        MetricStatus::Fail => COLOR_FAIL,
        MetricStatus::Skip => COLOR_SKIP,
    }
}

fn worst_metric_status(c: &CompareReceipt) -> MetricStatus {
    let mut worst = MetricStatus::Pass;
    for delta in c.deltas.values() {
        worst = match (worst, delta.status) {
            (MetricStatus::Fail, _) | (_, MetricStatus::Fail) => MetricStatus::Fail,
            (MetricStatus::Warn, _) | (_, MetricStatus::Warn) => MetricStatus::Warn,
            (MetricStatus::Skip, _) | (_, MetricStatus::Skip) => MetricStatus::Skip,
            _ => MetricStatus::Pass,
        };
    }
    if c.deltas.is_empty() {
        return MetricStatus::Skip;
    }
    worst
}

// ── Text width estimation ────────────────────────────────────────

/// Approximate the rendered pixel width of `text` at 11px Verdana (the
/// shields.io default). Uses per-character width buckets derived from the
/// shields.io source.
pub fn text_width(text: &str) -> f64 {
    let mut w: f64 = 0.0;
    for ch in text.chars() {
        w += char_width(ch);
    }
    w
}

/// Per-character width at 11px Verdana, matching the shields.io badge
/// generator. The widths are averages for the character classes.
fn char_width(ch: char) -> f64 {
    match ch {
        // Narrow characters
        'i' | 'l' | '!' | '|' | ',' | '.' | ':' | ';' | '\'' => 3.7,
        'I' | 'j' | 'f' | 'r' | 't' | '(' | ')' | '[' | ']' | '{' | '}' => 4.5,
        // Slightly narrow
        '1' => 5.0,
        // Medium-narrow
        ' ' | '-' | '_' => 5.0,
        // Wide uppercase
        'M' | 'W' => 9.5,
        'm' | 'w' => 8.5,
        // Standard uppercase
        'A'..='Z' => 7.5,
        // Standard lowercase / digits
        'a'..='z' | '0'..='9' => 6.5,
        // Other characters (symbols, unicode)
        '+' | '=' | '<' | '>' | '~' | '^' | '%' | '#' | '@' | '&' | '*' | '/' | '\\' | '?'
        | '$' => 6.5,
        _ => 6.5,
    }
}

// ── SVG rendering ────────────────────────────────────────────────

/// Render a `Badge` to an SVG string, compatible with shields.io.
pub fn render_svg(badge: &Badge) -> String {
    let label_width = text_width(&badge.label) + 10.0; // 5px padding each side
    let msg_width = text_width(&badge.message) + 10.0;
    let total_width = label_width + msg_width;

    let label_x = label_width / 2.0;
    let msg_x = label_width + msg_width / 2.0;

    let radius = match badge.style {
        BadgeStyle::Flat => 3,
        BadgeStyle::FlatSquare => 0,
    };

    let gradient = match badge.style {
        BadgeStyle::Flat => {
            r##"<linearGradient id="s" x2="0" y2="100%"><stop offset="0" stop-color="#bbb" stop-opacity=".1"/><stop offset="1" stop-opacity=".1"/></linearGradient>"##
        }
        BadgeStyle::FlatSquare => "",
    };

    let gradient_fill = match badge.style {
        BadgeStyle::Flat => r##"<rect rx="3" width="{tw}" height="20" fill="url(#s)"/>"##,
        BadgeStyle::FlatSquare => "",
    };

    // Build the gradient overlay with the actual width
    let gradient_overlay = gradient_fill.replace("{tw}", &format!("{total_width:.0}"));

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{tw:.0}" height="20" role="img" aria-label="{label}: {msg}"><title>{label}: {msg}</title>{gradient}<clipPath id="r"><rect width="{tw:.0}" height="20" rx="{radius}" fill="#fff"/></clipPath><g clip-path="url(#r)"><rect width="{lw:.0}" height="20" fill="#555"/><rect x="{lw:.0}" width="{mw:.0}" height="20" fill="{color}"/>{gradient_overlay}</g><g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,DejaVu Sans,sans-serif" text-rendering="geometricPrecision" font-size="110"><text aria-hidden="true" x="{lx:.0}" y="150" fill="#010101" fill-opacity=".3" transform="scale(.1)" textLength="{ltl:.0}">{label}</text><text x="{lx:.0}" y="140" transform="scale(.1)" fill="#fff" textLength="{ltl:.0}">{label}</text><text aria-hidden="true" x="{mx:.0}" y="150" fill="#010101" fill-opacity=".3" transform="scale(.1)" textLength="{mtl:.0}">{msg}</text><text x="{mx:.0}" y="140" transform="scale(.1)" fill="#fff" textLength="{mtl:.0}">{msg}</text></g></svg>"##,
        tw = total_width,
        lw = label_width,
        mw = msg_width,
        color = badge.color,
        gradient = gradient,
        gradient_overlay = gradient_overlay,
        radius = radius,
        lx = label_x * 10.0,
        mx = msg_x * 10.0,
        ltl = (label_width - 10.0) * 10.0,
        mtl = (msg_width - 10.0) * 10.0,
        label = xml_escape(&badge.label),
        msg = xml_escape(&badge.message),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, Direction, MetricStatistic, ToolInfo, Verdict,
        VerdictCounts,
    };
    use std::collections::BTreeMap;

    fn make_compare(verdict_status: VerdictStatus, metric_status: MetricStatus) -> CompareReceipt {
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
                status: metric_status,
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
                status: verdict_status,
                counts: VerdictCounts {
                    pass: if verdict_status == VerdictStatus::Pass {
                        1
                    } else {
                        0
                    },
                    warn: if verdict_status == VerdictStatus::Warn {
                        1
                    } else {
                        0
                    },
                    fail: if verdict_status == VerdictStatus::Fail {
                        1
                    } else {
                        0
                    },
                    skip: 0,
                },
                reasons: vec![],
            },
        }
    }

    fn make_report(verdict_status: VerdictStatus) -> PerfgateReport {
        let compare = make_compare(verdict_status, MetricStatus::Pass);
        PerfgateReport {
            report_type: perfgate_types::REPORT_SCHEMA_V1.to_string(),
            verdict: compare.verdict.clone(),
            compare: Some(compare),
            findings: vec![],
            complexity: None,
            summary: perfgate_types::ReportSummary {
                pass_count: 1,
                warn_count: 0,
                fail_count: 0,
                skip_count: 0,
                total_count: 1,
            },
            profile_path: None,
        }
    }

    // ── Color mapping ────────────────────────────────────────────

    #[test]
    fn verdict_pass_is_green() {
        let (label, color) = verdict_status_label_color(VerdictStatus::Pass);
        assert_eq!(label, "passing");
        assert_eq!(color, COLOR_PASS);
    }

    #[test]
    fn verdict_warn_is_yellow() {
        let (label, color) = verdict_status_label_color(VerdictStatus::Warn);
        assert_eq!(label, "warning");
        assert_eq!(color, COLOR_WARN);
    }

    #[test]
    fn verdict_fail_is_red() {
        let (label, color) = verdict_status_label_color(VerdictStatus::Fail);
        assert_eq!(label, "failing");
        assert_eq!(color, COLOR_FAIL);
    }

    #[test]
    fn verdict_skip_is_grey() {
        let (label, color) = verdict_status_label_color(VerdictStatus::Skip);
        assert_eq!(label, "skipped");
        assert_eq!(color, COLOR_SKIP);
    }

    #[test]
    fn metric_status_colors_match() {
        assert_eq!(metric_status_color(MetricStatus::Pass), COLOR_PASS);
        assert_eq!(metric_status_color(MetricStatus::Warn), COLOR_WARN);
        assert_eq!(metric_status_color(MetricStatus::Fail), COLOR_FAIL);
        assert_eq!(metric_status_color(MetricStatus::Skip), COLOR_SKIP);
    }

    // ── Text width ───────────────────────────────────────────────

    #[test]
    fn text_width_empty_is_zero() {
        assert!((text_width("") - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn text_width_increases_with_length() {
        let short = text_width("hi");
        let long = text_width("performance");
        assert!(long > short, "long={long}, short={short}");
    }

    #[test]
    fn narrow_chars_are_narrower_than_wide() {
        let narrow = text_width("iii");
        let wide = text_width("MMM");
        assert!(wide > narrow, "wide={wide}, narrow={narrow}");
    }

    // ── SVG rendering ────────────────────────────────────────────

    #[test]
    fn svg_contains_label_and_message() {
        let badge = Badge {
            label: "performance".into(),
            message: "passing".into(),
            color: COLOR_PASS.into(),
            style: BadgeStyle::Flat,
        };
        let svg = render_svg(&badge);
        assert!(svg.contains("performance"), "missing label");
        assert!(svg.contains("passing"), "missing message");
        assert!(svg.contains(COLOR_PASS), "missing color");
        assert!(svg.starts_with("<svg"), "not an SVG");
    }

    #[test]
    fn flat_square_has_zero_radius() {
        let badge = Badge {
            label: "test".into(),
            message: "ok".into(),
            color: COLOR_PASS.into(),
            style: BadgeStyle::FlatSquare,
        };
        let svg = render_svg(&badge);
        assert!(svg.contains(r#"rx="0""#), "expected rx=0 for flat-square");
    }

    #[test]
    fn flat_has_rounded_radius() {
        let badge = Badge {
            label: "test".into(),
            message: "ok".into(),
            color: COLOR_PASS.into(),
            style: BadgeStyle::Flat,
        };
        let svg = render_svg(&badge);
        assert!(svg.contains(r#"rx="3""#), "expected rx=3 for flat");
    }

    #[test]
    fn svg_escapes_special_characters() {
        let badge = Badge {
            label: "a<b".into(),
            message: "c&d".into(),
            color: COLOR_PASS.into(),
            style: BadgeStyle::Flat,
        };
        let svg = render_svg(&badge);
        assert!(svg.contains("a&lt;b"), "< not escaped");
        assert!(svg.contains("c&amp;d"), "& not escaped");
    }

    // ── Status badge ─────────────────────────────────────────────

    #[test]
    fn status_badge_from_compare_pass() {
        let compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        let badge = status_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat);
        assert_eq!(badge.label, "performance");
        assert_eq!(badge.message, "passing");
        assert_eq!(badge.color, COLOR_PASS);
    }

    #[test]
    fn status_badge_from_compare_fail() {
        let compare = make_compare(VerdictStatus::Fail, MetricStatus::Fail);
        let badge = status_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat);
        assert_eq!(badge.message, "failing");
        assert_eq!(badge.color, COLOR_FAIL);
    }

    #[test]
    fn status_badge_from_report() {
        let report = make_report(VerdictStatus::Warn);
        let badge = status_badge(&BadgeInput::Report(report), BadgeStyle::FlatSquare);
        assert_eq!(badge.message, "warning");
        assert_eq!(badge.color, COLOR_WARN);
        assert_eq!(badge.style, BadgeStyle::FlatSquare);
    }

    // ── Metric badge ─────────────────────────────────────────────

    #[test]
    fn metric_badge_from_compare() {
        let compare = make_compare(VerdictStatus::Warn, MetricStatus::Warn);
        let badge =
            metric_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat, "wall_ms").unwrap();
        assert_eq!(badge.label, "wall_ms");
        assert!(badge.message.contains("115"), "missing current value");
        assert!(badge.message.contains("ms"), "missing unit");
        assert!(
            badge.message.contains("+15.00%"),
            "missing pct: {}",
            badge.message
        );
        assert_eq!(badge.color, COLOR_WARN);
    }

    #[test]
    fn metric_badge_unknown_metric_errors() {
        let compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        let result = metric_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat, "no_such");
        assert!(result.is_err());
    }

    #[test]
    fn metric_badge_missing_delta_errors() {
        let compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        let result = metric_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat, "cpu_ms");
        assert!(result.is_err());
    }

    #[test]
    fn metric_badge_from_report_without_compare_errors() {
        let report = PerfgateReport {
            report_type: perfgate_types::REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            complexity: None,
            summary: perfgate_types::ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 0,
                skip_count: 0,
                total_count: 0,
            },
            profile_path: None,
        };
        let result = metric_badge(&BadgeInput::Report(report), BadgeStyle::Flat, "wall_ms");
        assert!(result.is_err());
    }

    // ── Trend badge ──────────────────────────────────────────────

    #[test]
    fn trend_badge_stable_when_all_pass() {
        let compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        let badge = trend_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat);
        assert_eq!(badge.label, "perf trend");
        assert_eq!(badge.message, "stable");
        assert_eq!(badge.color, COLOR_PASS);
    }

    #[test]
    fn trend_badge_degraded_when_warn() {
        let compare = make_compare(VerdictStatus::Warn, MetricStatus::Warn);
        let badge = trend_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat);
        assert_eq!(badge.message, "degraded");
        assert_eq!(badge.color, COLOR_WARN);
    }

    #[test]
    fn trend_badge_regressed_when_fail() {
        let compare = make_compare(VerdictStatus::Fail, MetricStatus::Fail);
        let badge = trend_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat);
        assert_eq!(badge.message, "regressed");
        assert_eq!(badge.color, COLOR_FAIL);
    }

    #[test]
    fn trend_badge_unknown_when_no_compare() {
        let report = PerfgateReport {
            report_type: perfgate_types::REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Skip,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            complexity: None,
            summary: perfgate_types::ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 0,
                skip_count: 0,
                total_count: 0,
            },
            profile_path: None,
        };
        let badge = trend_badge(&BadgeInput::Report(report), BadgeStyle::Flat);
        assert_eq!(badge.message, "unknown");
        assert_eq!(badge.color, COLOR_SKIP);
    }

    #[test]
    fn trend_badge_empty_deltas_is_unknown() {
        let mut compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        compare.deltas.clear();
        let badge = trend_badge(&BadgeInput::Compare(compare), BadgeStyle::Flat);
        assert_eq!(badge.message, "unknown");
    }

    // ── Use case end-to-end ──────────────────────────────────────

    #[test]
    fn usecase_status_from_compare() {
        let compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        let uc = BadgeUseCase;
        let outcome = uc
            .execute(
                &BadgeInput::Compare(compare),
                BadgeType::Status,
                BadgeStyle::Flat,
                None,
            )
            .unwrap();
        assert!(outcome.svg.starts_with("<svg"));
        assert!(outcome.svg.contains("passing"));
    }

    #[test]
    fn usecase_metric_requires_metric_name() {
        let compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        let uc = BadgeUseCase;
        let result = uc.execute(
            &BadgeInput::Compare(compare),
            BadgeType::Metric,
            BadgeStyle::Flat,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn usecase_metric_with_name() {
        let compare = make_compare(VerdictStatus::Pass, MetricStatus::Pass);
        let uc = BadgeUseCase;
        let outcome = uc
            .execute(
                &BadgeInput::Compare(compare),
                BadgeType::Metric,
                BadgeStyle::Flat,
                Some("wall_ms"),
            )
            .unwrap();
        assert!(outcome.svg.contains("wall_ms"));
    }

    #[test]
    fn usecase_trend() {
        let compare = make_compare(VerdictStatus::Fail, MetricStatus::Fail);
        let uc = BadgeUseCase;
        let outcome = uc
            .execute(
                &BadgeInput::Compare(compare),
                BadgeType::Trend,
                BadgeStyle::FlatSquare,
                None,
            )
            .unwrap();
        assert!(outcome.svg.contains("regressed"));
    }

    // ── xml_escape ───────────────────────────────────────────────

    #[test]
    fn xml_escape_covers_all_entities() {
        let raw = r#"<>&"'"#;
        let escaped = xml_escape(raw);
        assert_eq!(escaped, "&lt;&gt;&amp;&quot;&#39;");
    }

    // ── worst_metric_status ──────────────────────────────────────

    #[test]
    fn worst_metric_status_picks_fail_over_warn() {
        let mut compare = make_compare(VerdictStatus::Fail, MetricStatus::Warn);
        compare.deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 100.0,
                current: 200.0,
                ratio: 2.0,
                pct: 1.0,
                regression: 1.0,
                statistic: MetricStatistic::Median,
                significance: None,
                cv: None,
                noise_threshold: None,
                status: MetricStatus::Fail,
            },
        );
        assert_eq!(worst_metric_status(&compare), MetricStatus::Fail);
    }
}
