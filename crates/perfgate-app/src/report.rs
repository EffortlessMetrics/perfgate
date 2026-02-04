//! Report use case for generating cockpit-compatible report envelopes.
//!
//! This module provides functionality for wrapping a CompareReceipt into
//! a `perfgate.report.v1` envelope suitable for cockpit integration and
//! CI dashboard display.

use perfgate_domain::derive_report;
use perfgate_types::{
    CompareReceipt, Direction, FindingData, PerfgateReport, ReportFinding, ReportSummary, Severity,
    REPORT_SCHEMA_V1,
};

/// Request for generating a report from a compare receipt.
#[derive(Debug, Clone)]
pub struct ReportRequest {
    /// The compare receipt to wrap into a report.
    pub compare: CompareReceipt,
}

/// Result of a report generation operation.
#[derive(Debug, Clone)]
pub struct ReportResult {
    /// The generated report.
    pub report: PerfgateReport,
}

/// Use case for generating perfgate reports.
pub struct ReportUseCase;

impl ReportUseCase {
    /// Execute the report generation.
    ///
    /// Creates a PerfgateReport from a CompareReceipt by:
    /// - Setting report_type to "perfgate.report.v1"
    /// - Copying verdict from compare receipt
    /// - Including the full compare receipt
    /// - Deriving findings from domain logic (warn/fail metrics)
    /// - Computing summary counts
    ///
    /// # Invariants
    ///
    /// - Report verdict matches compare verdict
    /// - Finding count equals warn + fail count in deltas
    /// - Output is deterministic (same input -> same output)
    pub fn execute(req: ReportRequest) -> ReportResult {
        let domain_report = derive_report(&req.compare);

        // Convert domain findings to types findings
        let findings: Vec<ReportFinding> = domain_report
            .findings
            .into_iter()
            .map(|f| {
                let severity = if f.code == "metric_fail" {
                    Severity::Fail
                } else {
                    Severity::Warn
                };

                let direction = req
                    .compare
                    .budgets
                    .iter()
                    .find(|(metric, _)| metric_to_string(**metric) == f.data.metric_name)
                    .map(|(_, budget)| budget.direction)
                    .unwrap_or(Direction::Lower);

                let message = format!(
                    "{} for {}: {:.2}% regression (threshold: {:.2}%)",
                    if severity == Severity::Fail {
                        "Performance regression exceeded threshold"
                    } else {
                        "Performance regression near threshold"
                    },
                    f.data.metric_name,
                    f.data.regression_pct * 100.0,
                    f.data.threshold * 100.0
                );

                ReportFinding {
                    check_id: f.check_id,
                    code: f.code,
                    severity,
                    message,
                    data: Some(FindingData {
                        metric_name: f.data.metric_name,
                        baseline: f.data.baseline,
                        current: f.data.current,
                        regression_pct: f.data.regression_pct,
                        threshold: f.data.threshold,
                        direction,
                    }),
                }
            })
            .collect();

        let summary = ReportSummary {
            pass_count: req.compare.verdict.counts.pass,
            warn_count: req.compare.verdict.counts.warn,
            fail_count: req.compare.verdict.counts.fail,
            total_count: req.compare.verdict.counts.pass
                + req.compare.verdict.counts.warn
                + req.compare.verdict.counts.fail,
        };

        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: req.compare.verdict.clone(),
            compare: Some(req.compare),
            findings,
            summary,
        };

        ReportResult { report }
    }
}

/// Converts a Metric enum to its string representation.
fn metric_to_string(metric: perfgate_types::Metric) -> String {
    match metric {
        perfgate_types::Metric::WallMs => "wall_ms".to_string(),
        perfgate_types::Metric::MaxRssKb => "max_rss_kb".to_string(),
        perfgate_types::Metric::ThroughputPerS => "throughput_per_s".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, Direction, Metric, MetricStatus, ToolInfo, Verdict,
        VerdictCounts, VerdictStatus, COMPARE_SCHEMA_V1,
    };
    use std::collections::BTreeMap;

    fn create_pass_compare_receipt() -> CompareReceipt {
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
                current: 900.0,
                ratio: 0.9,
                pct: -0.1,
                regression: 0.0,
                status: MetricStatus::Pass,
            },
        );

        CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baseline.json".to_string()),
                run_id: Some("baseline-001".to_string()),
            },
            current_ref: CompareRef {
                path: Some("current.json".to_string()),
                run_id: Some("current-001".to_string()),
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
        }
    }

    fn create_warn_compare_receipt() -> CompareReceipt {
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
                current: 1190.0,
                ratio: 1.19,
                pct: 0.19,
                regression: 0.19,
                status: MetricStatus::Warn,
            },
        );

        CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baseline.json".to_string()),
                run_id: Some("baseline-001".to_string()),
            },
            current_ref: CompareRef {
                path: Some("current.json".to_string()),
                run_id: Some("current-001".to_string()),
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

    fn create_fail_compare_receipt() -> CompareReceipt {
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
                current: 1500.0,
                ratio: 1.5,
                pct: 0.5,
                regression: 0.5,
                status: MetricStatus::Fail,
            },
        );

        CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "hello".to_string()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baseline.json".to_string()),
                run_id: Some("baseline-001".to_string()),
            },
            current_ref: CompareRef {
                path: Some("current.json".to_string()),
                run_id: Some("current-001".to_string()),
            },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 1,
                },
                reasons: vec!["wall_ms_fail".to_string()],
            },
        }
    }

    #[test]
    fn test_report_from_pass_compare() {
        let compare = create_pass_compare_receipt();
        let result = ReportUseCase::execute(ReportRequest { compare });

        assert_eq!(result.report.report_type, REPORT_SCHEMA_V1);
        assert_eq!(result.report.verdict.status, VerdictStatus::Pass);
        assert!(result.report.findings.is_empty());
        assert_eq!(result.report.summary.pass_count, 1);
        assert_eq!(result.report.summary.warn_count, 0);
        assert_eq!(result.report.summary.fail_count, 0);
        assert_eq!(result.report.summary.total_count, 1);
    }

    #[test]
    fn test_report_from_warn_compare() {
        let compare = create_warn_compare_receipt();
        let result = ReportUseCase::execute(ReportRequest { compare });

        assert_eq!(result.report.report_type, REPORT_SCHEMA_V1);
        assert_eq!(result.report.verdict.status, VerdictStatus::Warn);
        assert_eq!(result.report.findings.len(), 1);
        assert_eq!(result.report.findings[0].code, "metric_warn");
        assert_eq!(result.report.findings[0].severity, Severity::Warn);
        assert_eq!(result.report.summary.warn_count, 1);
    }

    #[test]
    fn test_report_from_fail_compare() {
        let compare = create_fail_compare_receipt();
        let result = ReportUseCase::execute(ReportRequest { compare });

        assert_eq!(result.report.report_type, REPORT_SCHEMA_V1);
        assert_eq!(result.report.verdict.status, VerdictStatus::Fail);
        assert_eq!(result.report.findings.len(), 1);
        assert_eq!(result.report.findings[0].code, "metric_fail");
        assert_eq!(result.report.findings[0].severity, Severity::Fail);
        assert_eq!(result.report.summary.fail_count, 1);
    }

    #[test]
    fn test_report_verdict_matches_compare_verdict() {
        let pass_compare = create_pass_compare_receipt();
        let pass_result = ReportUseCase::execute(ReportRequest {
            compare: pass_compare.clone(),
        });
        assert_eq!(
            pass_result.report.verdict.status,
            pass_compare.verdict.status
        );

        let warn_compare = create_warn_compare_receipt();
        let warn_result = ReportUseCase::execute(ReportRequest {
            compare: warn_compare.clone(),
        });
        assert_eq!(
            warn_result.report.verdict.status,
            warn_compare.verdict.status
        );

        let fail_compare = create_fail_compare_receipt();
        let fail_result = ReportUseCase::execute(ReportRequest {
            compare: fail_compare.clone(),
        });
        assert_eq!(
            fail_result.report.verdict.status,
            fail_compare.verdict.status
        );
    }

    #[test]
    fn test_report_is_deterministic() {
        let compare = create_fail_compare_receipt();

        let result1 = ReportUseCase::execute(ReportRequest {
            compare: compare.clone(),
        });
        let result2 = ReportUseCase::execute(ReportRequest {
            compare: compare.clone(),
        });

        let json1 = serde_json::to_string(&result1.report).unwrap();
        let json2 = serde_json::to_string(&result2.report).unwrap();

        assert_eq!(json1, json2, "Report output should be deterministic");
    }

    #[test]
    fn test_finding_count_equals_warn_plus_fail() {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.18,
                direction: Direction::Lower,
            },
        );
        budgets.insert(
            Metric::MaxRssKb,
            Budget {
                threshold: 0.15,
                warn_threshold: 0.135,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 1000.0,
                current: 1190.0,
                ratio: 1.19,
                pct: 0.19,
                regression: 0.19,
                status: MetricStatus::Warn,
            },
        );
        deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 1024.0,
                current: 1280.0,
                ratio: 1.25,
                pct: 0.25,
                regression: 0.25,
                status: MetricStatus::Fail,
            },
        );

        let compare = CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            bench: BenchMeta {
                name: "test-bench".to_string(),
                cwd: None,
                command: vec!["test".to_string()],
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
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 1,
                    fail: 1,
                },
                reasons: vec![],
            },
        };

        let result = ReportUseCase::execute(ReportRequest { compare });

        // Finding count should equal warn + fail
        assert_eq!(result.report.findings.len(), 2);
        assert_eq!(
            result.report.findings.len(),
            (result.report.summary.warn_count + result.report.summary.fail_count) as usize
        );
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, Direction, Metric, MetricStatus, ToolInfo, Verdict,
        VerdictCounts, VerdictStatus, COMPARE_SCHEMA_V1,
    };
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    // --- Strategies for generating CompareReceipt ---

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
            Just(Metric::WallMs),
            Just(Metric::MaxRssKb),
            Just(Metric::ThroughputPerS),
        ]
    }

    fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
        proptest::collection::btree_map(metric_strategy(), budget_strategy(), 0..4)
    }

    fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
        proptest::collection::btree_map(metric_strategy(), delta_strategy(), 0..4)
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
                        schema: COMPARE_SCHEMA_V1.to_string(),
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

        /// Property: Report verdict always matches compare verdict
        #[test]
        fn report_verdict_matches_compare_verdict(compare in compare_receipt_strategy()) {
            let result = ReportUseCase::execute(ReportRequest { compare: compare.clone() });

            prop_assert_eq!(
                result.report.verdict.status,
                compare.verdict.status,
                "Report verdict should match compare verdict"
            );
        }

        /// Property: Finding count equals warn + fail delta count
        #[test]
        fn finding_count_equals_warn_plus_fail(compare in compare_receipt_strategy()) {
            let result = ReportUseCase::execute(ReportRequest { compare: compare.clone() });

            let warn_fail_count = compare.deltas.values()
                .filter(|d| d.status == MetricStatus::Warn || d.status == MetricStatus::Fail)
                .count();

            prop_assert_eq!(
                result.report.findings.len(),
                warn_fail_count,
                "Finding count should equal warn + fail delta count"
            );
        }

        /// Property: Report is deterministic (same input -> same output)
        #[test]
        fn report_is_deterministic(compare in compare_receipt_strategy()) {
            let result1 = ReportUseCase::execute(ReportRequest { compare: compare.clone() });
            let result2 = ReportUseCase::execute(ReportRequest { compare: compare.clone() });

            let json1 = serde_json::to_string(&result1.report).unwrap();
            let json2 = serde_json::to_string(&result2.report).unwrap();

            prop_assert_eq!(json1, json2, "Report output should be deterministic");
        }

        /// Property: Report type is always perfgate.report.v1
        #[test]
        fn report_type_is_always_v1(compare in compare_receipt_strategy()) {
            let result = ReportUseCase::execute(ReportRequest { compare });

            prop_assert_eq!(
                result.report.report_type,
                REPORT_SCHEMA_V1,
                "Report type should always be perfgate.report.v1"
            );
        }

        /// Property: Summary counts match verdict counts
        #[test]
        fn summary_counts_match_verdict_counts(compare in compare_receipt_strategy()) {
            let result = ReportUseCase::execute(ReportRequest { compare: compare.clone() });

            prop_assert_eq!(
                result.report.summary.pass_count,
                compare.verdict.counts.pass,
                "Summary pass count should match verdict counts"
            );
            prop_assert_eq!(
                result.report.summary.warn_count,
                compare.verdict.counts.warn,
                "Summary warn count should match verdict counts"
            );
            prop_assert_eq!(
                result.report.summary.fail_count,
                compare.verdict.counts.fail,
                "Summary fail count should match verdict counts"
            );
        }

        /// Property: Findings have correct severity
        #[test]
        fn findings_have_correct_severity(compare in compare_receipt_strategy()) {
            let result = ReportUseCase::execute(ReportRequest { compare: compare.clone() });

            for finding in &result.report.findings {
                match finding.code.as_str() {
                    "metric_fail" => {
                        prop_assert_eq!(
                            finding.severity,
                            Severity::Fail,
                            "metric_fail findings should have Fail severity"
                        );
                    }
                    "metric_warn" => {
                        prop_assert_eq!(
                            finding.severity,
                            Severity::Warn,
                            "metric_warn findings should have Warn severity"
                        );
                    }
                    _ => {
                        prop_assert!(false, "Unexpected finding code: {}", finding.code);
                    }
                }
            }
        }
    }
}
