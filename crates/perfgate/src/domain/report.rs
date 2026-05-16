use perfgate_types::{
    CHECK_ID_BUDGET, CompareReceipt, FINDING_CODE_METRIC_FAIL, FINDING_CODE_METRIC_WARN,
    MetricStatus, VerdictStatus,
};

use super::metric_to_string;

// ============================================================================
// Report Derivation
// ============================================================================

/// Data for a single finding in a report.
#[derive(Debug, Clone, PartialEq)]
pub struct FindingData {
    /// The metric name (e.g., "wall_ms", "max_rss_kb", "throughput_per_s").
    pub metric_name: String,
    /// The benchmark name.
    pub bench_name: String,
    /// Baseline value for the metric.
    pub baseline: f64,
    /// Current value for the metric.
    pub current: f64,
    /// Regression percentage (e.g., 0.15 means 15% regression).
    pub regression_pct: f64,
    /// The threshold that was exceeded (for fail) or approached (for warn).
    pub threshold: f64,
}

/// A single finding in a report.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Finding code: "metric_warn" or "metric_fail".
    pub code: String,
    /// Check identifier: always "perf.budget".
    pub check_id: String,
    /// Finding data containing metric details.
    pub data: FindingData,
}

/// Report derived from a CompareReceipt.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The overall verdict status, matching the compare verdict.
    pub verdict: VerdictStatus,
    /// Findings for metrics that have Warn or Fail status.
    /// Ordered deterministically by metric name, then bench name.
    pub findings: Vec<Finding>,
}

/// Derives a report from a CompareReceipt.
///
/// Creates findings for each metric delta with status Warn or Fail.
/// Findings are ordered deterministically by metric name (then bench name if
/// multiple benches were compared, though currently CompareReceipt is per-bench).
///
/// # Invariants
///
/// - Number of findings equals count of warn + fail status deltas
/// - Report verdict matches compare verdict
/// - Findings are ordered deterministically (by metric name)
///
/// # Examples
///
/// ```
/// use perfgate::domain::derive_report;
/// use perfgate_types::*;
/// use std::collections::BTreeMap;
///
/// let receipt = CompareReceipt {
///     schema: COMPARE_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     bench: BenchMeta {
///         name: "my-bench".into(), cwd: None,
///         command: vec!["echo".into()], repeat: 5, warmup: 0,
///         work_units: None, timeout_ms: None,
///     },
///     baseline_ref: CompareRef { path: None, run_id: None },
///     current_ref: CompareRef { path: None, run_id: None },
///     budgets: BTreeMap::new(),
///     deltas: BTreeMap::new(),
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts { pass: 0, warn: 0, fail: 0, skip: 0 },
///         reasons: vec![],
///     },
/// };
///
/// let report = derive_report(&receipt);
/// assert_eq!(report.verdict, VerdictStatus::Pass);
/// assert!(report.findings.is_empty());
/// ```
#[must_use = "pure computation; call site should use the returned Report"]
pub fn derive_report(receipt: &CompareReceipt) -> Report {
    let mut findings = Vec::new();

    // Iterate over deltas in deterministic order (BTreeMap is sorted by key)
    for (metric, delta) in &receipt.deltas {
        match delta.status {
            MetricStatus::Pass | MetricStatus::Skip => continue,
            MetricStatus::Warn | MetricStatus::Fail => {
                let code = match delta.status {
                    MetricStatus::Warn => FINDING_CODE_METRIC_WARN.to_string(),
                    MetricStatus::Fail => FINDING_CODE_METRIC_FAIL.to_string(),
                    _ => unreachable!(),
                };

                // Get the threshold from budgets if available
                let threshold = receipt
                    .budgets
                    .get(metric)
                    .map(|b| b.threshold)
                    .unwrap_or(0.0);

                findings.push(Finding {
                    code,
                    check_id: CHECK_ID_BUDGET.to_string(),
                    data: FindingData {
                        metric_name: metric_to_string(*metric),
                        bench_name: receipt.bench.name.clone(),
                        baseline: delta.baseline,
                        current: delta.current,
                        regression_pct: delta.regression,
                        threshold,
                    },
                });
            }
        }
    }

    // Findings are already sorted by metric name since we iterate over BTreeMap
    // For multi-bench scenarios (future), we would also sort by bench_name
    // Currently sorting is: metric name (from BTreeMap order)

    Report {
        verdict: receipt.verdict.status,
        findings,
    }
}
