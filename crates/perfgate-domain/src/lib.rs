//! Domain logic for perfgate.
//!
//! This crate is intentionally I/O-free: it does math and policy.

mod paired;

pub use paired::{PairedComparison, compare_paired_stats, compute_paired_stats};

pub use perfgate_host_detect::detect_host_mismatch;

pub use perfgate_budget::{
    BudgetError, BudgetResult, aggregate_verdict, calculate_regression, determine_status,
    evaluate_budget, evaluate_budgets, reason_token,
};

pub use perfgate_significance::{compute_significance, mean_and_variance};
pub use perfgate_stats::{median_f64_sorted, median_u64_sorted, summarize_f64, summarize_u64};

use perfgate_types::{
    Budget, CHECK_ID_BUDGET, CompareReceipt, Delta, FINDING_CODE_METRIC_FAIL,
    FINDING_CODE_METRIC_WARN, Metric, MetricStatistic, MetricStatus, RunReceipt, Stats, Verdict,
    VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

pub use perfgate_error::StatsError;

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("no samples to summarize")]
    NoSamples,

    #[error(transparent)]
    Stats(#[from] StatsError),

    #[error("baseline value for {0:?} must be > 0")]
    InvalidBaseline(Metric),

    #[error("significance alpha must be between 0.0 and 1.0, got {0}")]
    InvalidAlpha(f64),
}

#[cfg(test)]
mod advanced_analytics_tests {
    use super::*;
    use perfgate_types::{BenchMeta, Direction, HostInfo, RunMeta, RunReceipt, Sample, ToolInfo};

    fn make_run_receipt_with_walls(name: &str, walls: &[u64]) -> RunReceipt {
        let samples: Vec<Sample> = walls
            .iter()
            .map(|&wall_ms| Sample {
                wall_ms,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            })
            .collect();

        let stats = compute_stats(&samples, None).expect("compute stats");

        RunReceipt {
            schema: perfgate_types::RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "test".to_string(),
            },
            run: RunMeta {
                id: format!("run-{}", name),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: name.to_string(),
                cwd: None,
                command: vec!["echo".to_string(), "ok".to_string()],
                repeat: walls.len() as u32,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples,
            stats,
        }
    }

    fn wall_budget(threshold: f64) -> BTreeMap<Metric, Budget> {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                noise_threshold: None,
                noise_policy: perfgate_types::NoisePolicy::Ignore,
                threshold,
                warn_threshold: threshold * 0.9,
                direction: Direction::Lower,
            },
        );
        budgets
    }

    #[test]
    fn compare_runs_uses_p95_when_requested() {
        let baseline =
            make_run_receipt_with_walls("bench", &[100, 100, 100, 100, 100, 100, 100, 200]);
        let current =
            make_run_receipt_with_walls("bench", &[100, 100, 100, 100, 100, 100, 100, 300]);

        let budgets = wall_budget(0.20);
        let mut stats = BTreeMap::new();
        stats.insert(Metric::WallMs, MetricStatistic::P95);

        let comparison =
            compare_runs(&baseline, &current, &budgets, &stats, None).expect("compare runs");

        let delta = comparison.deltas.get(&Metric::WallMs).expect("wall delta");
        assert_eq!(delta.statistic, MetricStatistic::P95);
        assert!(delta.current > delta.baseline);
        assert_eq!(delta.status, MetricStatus::Fail);
    }

    #[test]
    fn compare_runs_can_require_significance() {
        let baseline =
            make_run_receipt_with_walls("bench", &[50, 60, 70, 80, 90, 100, 110, 120, 130, 140]);
        let current =
            make_run_receipt_with_walls("bench", &[56, 66, 76, 86, 96, 106, 116, 126, 136, 146]);
        let budgets = wall_budget(0.05);
        let stats = BTreeMap::new();

        let advisory = compare_runs(
            &baseline,
            &current,
            &budgets,
            &stats,
            Some(SignificancePolicy {
                alpha: 0.05,
                min_samples: 8,
                require_significance: false,
            }),
        )
        .expect("compare advisory");
        let advisory_delta = advisory.deltas.get(&Metric::WallMs).expect("wall delta");
        assert_eq!(advisory_delta.status, MetricStatus::Fail);
        assert!(
            advisory_delta
                .significance
                .as_ref()
                .map(|s| !s.significant)
                .unwrap_or(false)
        );

        let enforced = compare_runs(
            &baseline,
            &current,
            &budgets,
            &stats,
            Some(SignificancePolicy {
                alpha: 0.05,
                min_samples: 8,
                require_significance: true,
            }),
        )
        .expect("compare enforced");
        let enforced_delta = enforced.deltas.get(&Metric::WallMs).expect("wall delta");
        assert_eq!(enforced_delta.status, MetricStatus::Pass);
    }
}

/// Compute perfgate stats from samples.
///
/// Warmup samples (`sample.warmup == true`) are excluded.
///
/// # Examples
///
/// ```
/// use perfgate_domain::compute_stats;
/// use perfgate_types::Sample;
///
/// let samples = vec![
///     Sample {
///         wall_ms: 100, exit_code: 0, warmup: false, timed_out: false,
///         cpu_ms: None, page_faults: None, ctx_switches: None,
///         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
///         network_packets: None, energy_uj: None, binary_bytes: None, stdout: None, stderr: None,
///     },
///     Sample {
///         wall_ms: 120, exit_code: 0, warmup: false, timed_out: false,
///         cpu_ms: None, page_faults: None, ctx_switches: None,
///         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
///         network_packets: None, energy_uj: None, binary_bytes: None, stdout: None, stderr: None,
///     },
/// ];
///
/// let stats = compute_stats(&samples, None).unwrap();
/// assert_eq!(stats.wall_ms.min, 100);
/// assert_eq!(stats.wall_ms.max, 120);
/// ```
pub fn compute_stats(
    samples: &[perfgate_types::Sample],
    work_units: Option<u64>,
) -> Result<Stats, DomainError> {
    let measured: Vec<&perfgate_types::Sample> = samples.iter().filter(|s| !s.warmup).collect();
    if measured.is_empty() {
        return Err(DomainError::NoSamples);
    }

    let wall: Vec<u64> = measured.iter().map(|s| s.wall_ms).collect();
    let wall_ms = summarize_u64(&wall)?;

    let cpu_vals: Vec<u64> = measured.iter().filter_map(|s| s.cpu_ms).collect();
    let cpu_ms = if cpu_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&cpu_vals)?)
    };

    let page_fault_vals: Vec<u64> = measured.iter().filter_map(|s| s.page_faults).collect();
    let page_faults = if page_fault_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&page_fault_vals)?)
    };

    let ctx_switch_vals: Vec<u64> = measured.iter().filter_map(|s| s.ctx_switches).collect();
    let ctx_switches = if ctx_switch_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&ctx_switch_vals)?)
    };

    let rss_vals: Vec<u64> = measured.iter().filter_map(|s| s.max_rss_kb).collect();
    let max_rss_kb = if rss_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&rss_vals)?)
    };

    let io_read_vals: Vec<u64> = measured.iter().filter_map(|s| s.io_read_bytes).collect();
    let io_read_bytes = if io_read_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&io_read_vals)?)
    };

    let io_write_vals: Vec<u64> = measured.iter().filter_map(|s| s.io_write_bytes).collect();
    let io_write_bytes = if io_write_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&io_write_vals)?)
    };

    let network_vals: Vec<u64> = measured.iter().filter_map(|s| s.network_packets).collect();
    let network_packets = if network_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&network_vals)?)
    };

    let energy_vals: Vec<u64> = measured.iter().filter_map(|s| s.energy_uj).collect();
    let energy_uj = if energy_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&energy_vals)?)
    };

    let binary_vals: Vec<u64> = measured.iter().filter_map(|s| s.binary_bytes).collect();
    let binary_bytes = if binary_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&binary_vals)?)
    };

    let throughput_per_s = match work_units {
        Some(work) => {
            let thr: Vec<f64> = measured
                .iter()
                .map(|s| {
                    let secs = (s.wall_ms as f64) / 1000.0;
                    if secs <= 0.0 {
                        0.0
                    } else {
                        (work as f64) / secs
                    }
                })
                .collect();
            Some(summarize_f64(&thr)?)
        }
        None => None,
    };

    Ok(Stats {
        wall_ms,
        cpu_ms,
        page_faults,
        ctx_switches,
        max_rss_kb,
        io_read_bytes,
        io_write_bytes,
        network_packets,
        energy_uj,
        binary_bytes,
        throughput_per_s,
    })
}

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
/// use perfgate_domain::compare_stats;
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
pub fn compare_stats(
    baseline: &Stats,
    current: &Stats,
    budgets: &BTreeMap<Metric, Budget>,
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

        let result = evaluate_budget(bv, cv, budget, current_cv)
            .map_err(|_| DomainError::InvalidBaseline(*metric))?;

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

        let result = evaluate_budget(bv, cv, budget, current_cv)
            .map_err(|_| DomainError::InvalidBaseline(*metric))?;

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

    let verdict = aggregate_verdict_from_counts(counts, reasons);

    Ok(Comparison { deltas, verdict })
}

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
/// use perfgate_domain::derive_report;
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

fn metric_cv(stats: &Stats, metric: Metric) -> Option<f64> {
    match metric {
        Metric::BinaryBytes => stats.binary_bytes.as_ref().and_then(|s| s.cv()),
        Metric::CpuMs => stats.cpu_ms.as_ref().and_then(|s| s.cv()),
        Metric::CtxSwitches => stats.ctx_switches.as_ref().and_then(|s| s.cv()),
        Metric::EnergyUj => stats.energy_uj.as_ref().and_then(|s| s.cv()),
        Metric::IoReadBytes => stats.io_read_bytes.as_ref().and_then(|s| s.cv()),
        Metric::IoWriteBytes => stats.io_write_bytes.as_ref().and_then(|s| s.cv()),
        Metric::MaxRssKb => stats.max_rss_kb.as_ref().and_then(|s| s.cv()),
        Metric::NetworkPackets => stats.network_packets.as_ref().and_then(|s| s.cv()),
        Metric::PageFaults => stats.page_faults.as_ref().and_then(|s| s.cv()),
        Metric::ThroughputPerS => stats.throughput_per_s.as_ref().and_then(|s| s.cv()),
        Metric::WallMs => stats.wall_ms.cv(),
    }
}

/// Converts a Metric enum to its string representation.
fn metric_to_string(metric: Metric) -> String {
    metric.as_str().to_string()
}

fn metric_value(stats: &Stats, metric: Metric) -> Option<f64> {
    match metric {
        Metric::BinaryBytes => stats.binary_bytes.as_ref().map(|s| s.median as f64),
        Metric::CpuMs => stats.cpu_ms.as_ref().map(|s| s.median as f64),
        Metric::CtxSwitches => stats.ctx_switches.as_ref().map(|s| s.median as f64),
        Metric::EnergyUj => stats.energy_uj.as_ref().map(|s| s.median as f64),
        Metric::IoReadBytes => stats.io_read_bytes.as_ref().map(|s| s.median as f64),
        Metric::IoWriteBytes => stats.io_write_bytes.as_ref().map(|s| s.median as f64),
        Metric::MaxRssKb => stats.max_rss_kb.as_ref().map(|s| s.median as f64),
        Metric::NetworkPackets => stats.network_packets.as_ref().map(|s| s.median as f64),
        Metric::PageFaults => stats.page_faults.as_ref().map(|s| s.median as f64),
        Metric::ThroughputPerS => stats.throughput_per_s.as_ref().map(|s| s.median),
        Metric::WallMs => Some(stats.wall_ms.median as f64),
    }
}

fn metric_value_from_run(
    run: &RunReceipt,
    metric: Metric,
    statistic: MetricStatistic,
) -> Option<f64> {
    match statistic {
        MetricStatistic::Median => metric_value(&run.stats, metric),
        MetricStatistic::P95 => {
            let values = metric_series_from_run(run, metric);
            if values.is_empty() {
                metric_value(&run.stats, metric)
            } else {
                percentile(values, 0.95)
            }
        }
    }
}

fn metric_series_from_run(run: &RunReceipt, metric: Metric) -> Vec<f64> {
    let measured = run.samples.iter().filter(|s| !s.warmup);

    match metric {
        Metric::BinaryBytes => measured
            .filter_map(|s| s.binary_bytes.map(|v| v as f64))
            .collect(),
        Metric::CpuMs => measured
            .filter_map(|s| s.cpu_ms.map(|v| v as f64))
            .collect(),
        Metric::CtxSwitches => measured
            .filter_map(|s| s.ctx_switches.map(|v| v as f64))
            .collect(),
        Metric::EnergyUj => measured
            .filter_map(|s| s.energy_uj.map(|v| v as f64))
            .collect(),
        Metric::IoReadBytes => measured
            .filter_map(|s| s.io_read_bytes.map(|v| v as f64))
            .collect(),
        Metric::IoWriteBytes => measured
            .filter_map(|s| s.io_write_bytes.map(|v| v as f64))
            .collect(),
        Metric::MaxRssKb => measured
            .filter_map(|s| s.max_rss_kb.map(|v| v as f64))
            .collect(),
        Metric::NetworkPackets => measured
            .filter_map(|s| s.network_packets.map(|v| v as f64))
            .collect(),
        Metric::PageFaults => measured
            .filter_map(|s| s.page_faults.map(|v| v as f64))
            .collect(),
        Metric::ThroughputPerS => {
            let Some(work) = run.bench.work_units else {
                return Vec::new();
            };
            measured
                .map(|s| {
                    let secs = (s.wall_ms as f64) / 1000.0;
                    if secs <= 0.0 {
                        0.0
                    } else {
                        (work as f64) / secs
                    }
                })
                .collect()
        }
        Metric::WallMs => measured.map(|s| s.wall_ms as f64).collect(),
    }
}

fn percentile(mut values: Vec<f64>, q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    if values.len() == 1 {
        return Some(values[0]);
    }

    let rank = q.clamp(0.0, 1.0) * (values.len() as f64 - 1.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;

    if lower == upper {
        return Some(values[lower]);
    }

    let weight = rank - lower as f64;
    Some(values[lower] + (values[upper] - values[lower]) * weight)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        Budget, Direction, F64Summary, Metric, MetricStatus, Sample, Stats, U64Summary,
        VerdictStatus,
    };
    use proptest::prelude::*;

    #[test]
    fn summarize_u64_median_even_rounds_down() {
        let s = summarize_u64(&[10, 20]).unwrap();
        assert_eq!(s.median, 15);
    }

    // =========================================================================
    // Property-Based Tests
    // =========================================================================

    /// **Validates: Requirements 3.1, 3.2, 3.3**
    ///
    /// Property 1: Statistics Computation Correctness
    ///
    /// For any non-empty list of u64 values, the computed summary SHALL have:
    /// - `median` equal to the middle value (or average of two middle values for even-length lists)
    /// - `min` equal to the smallest value
    /// - `max` equal to the largest value
    mod property_tests {
        use super::*;

        /// Helper function to compute the expected median for a sorted slice.
        /// For even-length lists, computes the average of the two middle values,
        /// matching the implementation's rounding behavior.
        fn expected_median(sorted: &[u64]) -> u64 {
            let n = sorted.len();
            let mid = n / 2;
            if n % 2 == 1 {
                sorted[mid]
            } else {
                // Match the implementation's rounding: avoid overflow by splitting
                let a = sorted[mid - 1];
                let b = sorted[mid];
                (a / 2) + (b / 2) + ((a % 2 + b % 2) / 2)
            }
        }

        proptest! {
            /// **Validates: Requirements 3.1, 3.2, 3.3**
            ///
            /// Property 1: Statistics Computation Correctness
            ///
            /// For any non-empty list of u64 values:
            /// - min equals the smallest value
            /// - max equals the largest value
            /// - median equals the middle value (or average of two middle for even-length)
            #[test]
            fn prop_summarize_u64_correctness(values in prop::collection::vec(any::<u64>(), 1..100)) {
                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                // Sort the values to compute expected results
                let mut sorted = values.clone();
                sorted.sort_unstable();

                // Property: min is the smallest value
                let expected_min = *sorted.first().unwrap();
                prop_assert_eq!(
                    summary.min, expected_min,
                    "min should be the smallest value"
                );

                // Property: max is the largest value
                let expected_max = *sorted.last().unwrap();
                prop_assert_eq!(
                    summary.max, expected_max,
                    "max should be the largest value"
                );

                // Property: median is correct
                let expected_med = expected_median(&sorted);
                prop_assert_eq!(
                    summary.median, expected_med,
                    "median should be the middle value (or average for even-length)"
                );
            }

            /// **Validates: Requirements 3.1, 3.2, 3.3**
            ///
            /// Property: min <= median <= max always holds
            #[test]
            fn prop_summarize_u64_ordering(values in prop::collection::vec(any::<u64>(), 1..100)) {
                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                prop_assert!(
                    summary.min <= summary.median,
                    "min ({}) should be <= median ({})",
                    summary.min, summary.median
                );
                prop_assert!(
                    summary.median <= summary.max,
                    "median ({}) should be <= max ({})",
                    summary.median, summary.max
                );
            }

            /// **Validates: Requirements 3.1, 3.2, 3.3**
            ///
            /// Property: For single-element vectors, min == median == max
            #[test]
            fn prop_summarize_u64_single_element(value: u64) {
                let summary = summarize_u64(&[value]).expect("single element should succeed");

                prop_assert_eq!(summary.min, value, "min should equal the single value");
                prop_assert_eq!(summary.max, value, "max should equal the single value");
                prop_assert_eq!(summary.median, value, "median should equal the single value");
            }
        }

        // =====================================================================
        // Property 2: Statistics Ordering Invariant for f64
        // **Feature: comprehensive-test-coverage, Property 2: Statistics Ordering Invariant**
        // =====================================================================

        /// Strategy to generate finite f64 values (no NaN, no infinity).
        /// This ensures we test the ordering invariant with valid numeric values.
        fn finite_f64_strategy() -> impl Strategy<Value = f64> {
            // Generate finite f64 values in a reasonable range
            prop::num::f64::NORMAL.prop_filter("must be finite", |v| v.is_finite())
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 100,
                ..ProptestConfig::default()
            })]

            /// **Feature: comprehensive-test-coverage, Property 2: Statistics Ordering Invariant**
            /// **Validates: Requirements 4.6**
            ///
            /// Property 2: Statistics Ordering Invariant
            ///
            /// For any non-empty list of finite f64 values, the computed summary SHALL satisfy:
            /// min <= median <= max
            #[test]
            fn prop_summarize_f64_ordering(
                values in prop::collection::vec(finite_f64_strategy(), 1..100)
            ) {
                let summary = summarize_f64(&values).expect("non-empty vec should succeed");

                prop_assert!(
                    summary.min <= summary.median,
                    "min ({}) should be <= median ({})",
                    summary.min, summary.median
                );
                prop_assert!(
                    summary.median <= summary.max,
                    "median ({}) should be <= max ({})",
                    summary.median, summary.max
                );
            }

            /// **Feature: comprehensive-test-coverage, Property 2: Statistics Ordering Invariant**
            /// **Validates: Requirements 4.6**
            ///
            /// Property 2: Statistics Ordering Invariant (single element)
            ///
            /// For single-element vectors, min == median == max
            #[test]
            fn prop_summarize_f64_single_element(value in finite_f64_strategy()) {
                let summary = summarize_f64(&[value]).expect("single element should succeed");

                prop_assert!(
                    (summary.min - value).abs() < f64::EPSILON,
                    "min ({}) should equal the single value ({})",
                    summary.min, value
                );
                prop_assert!(
                    (summary.max - value).abs() < f64::EPSILON,
                    "max ({}) should equal the single value ({})",
                    summary.max, value
                );
                prop_assert!(
                    (summary.median - value).abs() < f64::EPSILON,
                    "median ({}) should equal the single value ({})",
                    summary.median, value
                );
            }

            /// **Feature: comprehensive-test-coverage, Property 2: Statistics Ordering Invariant**
            /// **Validates: Requirements 4.6**
            ///
            /// Property 2: Statistics Ordering Invariant (correctness)
            ///
            /// For any non-empty list of finite f64 values:
            /// - min equals the smallest value
            /// - max equals the largest value
            /// - median equals the middle value (or average of two middle for even-length)
            #[test]
            fn prop_summarize_f64_correctness(
                values in prop::collection::vec(finite_f64_strategy(), 1..100)
            ) {
                let summary = summarize_f64(&values).expect("non-empty vec should succeed");

                // Sort the values to compute expected results
                let mut sorted = values.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

                // Property: min is the smallest value
                let expected_min = *sorted.first().unwrap();
                prop_assert!(
                    (summary.min - expected_min).abs() < f64::EPSILON,
                    "min ({}) should be the smallest value ({})",
                    summary.min, expected_min
                );

                // Property: max is the largest value
                let expected_max = *sorted.last().unwrap();
                prop_assert!(
                    (summary.max - expected_max).abs() < f64::EPSILON,
                    "max ({}) should be the largest value ({})",
                    summary.max, expected_max
                );

                // Property: median is correct
                let n = sorted.len();
                let mid = n / 2;
                let expected_median = if n % 2 == 1 {
                    sorted[mid]
                } else {
                    (sorted[mid - 1] + sorted[mid]) / 2.0
                };
                prop_assert!(
                    (summary.median - expected_median).abs() < f64::EPSILON * 10.0,
                    "median ({}) should be the middle value ({})",
                    summary.median, expected_median
                );
            }

            /// **Feature: comprehensive-test-coverage, Property 2: Statistics Ordering Invariant**
            /// **Validates: Requirements 4.6**
            ///
            /// Property 2: Statistics Ordering Invariant (with infinity)
            ///
            /// The summarize_f64 function handles infinity values by sorting them
            /// appropriately (negative infinity is smallest, positive infinity is largest).
            /// The ordering invariant min <= median <= max should still hold.
            #[test]
            fn prop_summarize_f64_with_infinity(
                finite_values in prop::collection::vec(finite_f64_strategy(), 1..50),
                include_pos_inf in any::<bool>(),
                include_neg_inf in any::<bool>(),
            ) {
                let mut values = finite_values;

                // Optionally add positive infinity
                if include_pos_inf {
                    values.push(f64::INFINITY);
                }

                // Optionally add negative infinity
                if include_neg_inf {
                    values.push(f64::NEG_INFINITY);
                }

                let summary = summarize_f64(&values).expect("non-empty vec should succeed");

                // The ordering invariant should still hold
                prop_assert!(
                    summary.min <= summary.median,
                    "min ({}) should be <= median ({}) even with infinity values",
                    summary.min, summary.median
                );
                prop_assert!(
                    summary.median <= summary.max,
                    "median ({}) should be <= max ({}) even with infinity values",
                    summary.median, summary.max
                );

                // If we included negative infinity, min should be negative infinity
                if include_neg_inf {
                    prop_assert!(
                        summary.min == f64::NEG_INFINITY,
                        "min should be NEG_INFINITY when included, got {}",
                        summary.min
                    );
                }

                // If we included positive infinity, max should be positive infinity
                if include_pos_inf {
                    prop_assert!(
                        summary.max == f64::INFINITY,
                        "max should be INFINITY when included, got {}",
                        summary.max
                    );
                }
            }

            /// **Feature: comprehensive-test-coverage, Property 2: Statistics Ordering Invariant**
            /// **Validates: Requirements 4.6**
            ///
            /// Property 2: Statistics Ordering Invariant (NaN handling)
            ///
            /// The summarize_f64 function uses partial_cmp with Ordering::Equal fallback
            /// for NaN values. This test verifies the function doesn't panic with NaN
            /// and the ordering invariant holds for the non-NaN interpretation.
            #[test]
            fn prop_summarize_f64_with_nan_no_panic(
                finite_values in prop::collection::vec(finite_f64_strategy(), 1..50),
                nan_count in 0usize..3,
            ) {
                let mut values = finite_values;

                // Add some NaN values
                for _ in 0..nan_count {
                    values.push(f64::NAN);
                }

                // The function should not panic
                let result = summarize_f64(&values);
                prop_assert!(result.is_ok(), "summarize_f64 should not panic with NaN values");

                let summary = result.unwrap();

                // Due to NaN comparison behavior, we can only verify the function completes
                // and returns some result. The ordering may not hold strictly with NaN
                // because NaN comparisons are undefined, but the function should not panic.
                // We verify that if all values are finite (no NaN in result), ordering holds.
                if summary.min.is_finite() && summary.median.is_finite() && summary.max.is_finite() {
                    prop_assert!(
                        summary.min <= summary.median,
                        "min ({}) should be <= median ({}) for finite results",
                        summary.min, summary.median
                    );
                    prop_assert!(
                        summary.median <= summary.max,
                        "median ({}) should be <= max ({}) for finite results",
                        summary.median, summary.max
                    );
                }
            }
        }

        // =====================================================================
        // Property 3: Median Algorithm Correctness
        // **Feature: comprehensive-test-coverage, Property 3: Median Algorithm Correctness**
        // =====================================================================

        /// Strategy to generate large u64 values near u64::MAX for overflow testing.
        /// This generates values in the upper 10% of the u64 range.
        fn large_u64_strategy() -> impl Strategy<Value = u64> {
            // Generate values in the range [u64::MAX - u64::MAX/10, u64::MAX]
            // This ensures we test values near the overflow boundary
            let min_val = u64::MAX - (u64::MAX / 10);
            min_val..=u64::MAX
        }

        /// Reference implementation of median for u64 that uses u128 to avoid overflow.
        /// This serves as the oracle for testing the overflow-safe implementation.
        fn reference_median_u64(sorted: &[u64]) -> u64 {
            debug_assert!(!sorted.is_empty());
            let n = sorted.len();
            let mid = n / 2;
            if n % 2 == 1 {
                sorted[mid]
            } else {
                // Use u128 to compute the average without overflow, then truncate
                let a = sorted[mid - 1] as u128;
                let b = sorted[mid] as u128;
                ((a + b) / 2) as u64
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig {
                cases: 100,
                ..ProptestConfig::default()
            })]

            /// **Feature: comprehensive-test-coverage, Property 3: Median Algorithm Correctness**
            /// **Validates: Requirements 8.5**
            ///
            /// Property 3: Median Algorithm Correctness (Overflow Handling)
            ///
            /// For any non-empty list of large u64 values near u64::MAX, the median
            /// algorithm SHALL compute the correct result without overflow.
            /// The implementation uses the formula:
            /// (a/2) + (b/2) + ((a%2 + b%2)/2) to avoid overflow.
            #[test]
            fn prop_median_u64_overflow_handling(
                values in prop::collection::vec(large_u64_strategy(), 2..50)
            ) {
                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                // Sort values to compute expected median using reference implementation
                let mut sorted = values.clone();
                sorted.sort_unstable();

                let expected_median = reference_median_u64(&sorted);

                prop_assert_eq!(
                    summary.median, expected_median,
                    "median should match reference implementation for large values near u64::MAX"
                );

                // Also verify the ordering invariant holds
                prop_assert!(
                    summary.min <= summary.median,
                    "min ({}) should be <= median ({}) for large values",
                    summary.min, summary.median
                );
                prop_assert!(
                    summary.median <= summary.max,
                    "median ({}) should be <= max ({}) for large values",
                    summary.median, summary.max
                );
            }

            /// **Feature: comprehensive-test-coverage, Property 3: Median Algorithm Correctness**
            /// **Validates: Requirements 8.5**
            ///
            /// Property 3: Median Algorithm Correctness (Even Length - Average with Rounding Down)
            ///
            /// For any even-length sorted list of u64 values, the median SHALL equal
            /// the average of the two middle elements, rounded down (floor division).
            #[test]
            fn prop_median_u64_even_length_rounding(
                // Generate pairs of values to ensure even length
                pairs in prop::collection::vec((any::<u64>(), any::<u64>()), 1..50)
            ) {
                // Flatten pairs into a single vector (guaranteed even length)
                let values: Vec<u64> = pairs.into_iter().flat_map(|(a, b)| vec![a, b]).collect();
                prop_assert!(values.len().is_multiple_of(2), "length should be even");

                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                // Sort values to compute expected median
                let mut sorted = values.clone();
                sorted.sort_unstable();

                let n = sorted.len();
                let mid = n / 2;
                let a = sorted[mid - 1];
                let b = sorted[mid];

                // Expected median using reference implementation (u128 to avoid overflow)
                let expected_median = reference_median_u64(&sorted);

                prop_assert_eq!(
                    summary.median, expected_median,
                    "median for even-length list should be floor((a + b) / 2) where a={}, b={}",
                    a, b
                );

                // Verify rounding down behavior: median should be <= true average
                // (true average computed with u128 to avoid overflow)
                let true_avg_x2 = (a as u128) + (b as u128);
                let median_x2 = (summary.median as u128) * 2;
                prop_assert!(
                    median_x2 <= true_avg_x2,
                    "median*2 ({}) should be <= (a+b) ({}) due to floor rounding",
                    median_x2, true_avg_x2
                );
            }

            /// **Feature: comprehensive-test-coverage, Property 3: Median Algorithm Correctness**
            /// **Validates: Requirements 8.5**
            ///
            /// Property 3: Median Algorithm Correctness (Odd Length - Exact Middle)
            ///
            /// For any odd-length sorted list of u64 values, the median SHALL equal
            /// exactly the middle element (no averaging or rounding).
            #[test]
            fn prop_median_u64_odd_length_exact_middle(
                // Generate odd-length vectors by generating n values and adding one more
                base_values in prop::collection::vec(any::<u64>(), 1..50),
                extra_value: u64,
            ) {
                // Ensure odd length by conditionally adding an extra value
                let mut values = base_values;
                if values.len() % 2 == 0 {
                    values.push(extra_value);
                }
                prop_assert!(values.len() % 2 == 1, "length should be odd");

                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                // Sort values to find the exact middle element
                let mut sorted = values.clone();
                sorted.sort_unstable();

                let n = sorted.len();
                let mid = n / 2;
                let expected_median = sorted[mid];

                prop_assert_eq!(
                    summary.median, expected_median,
                    "median for odd-length list should be exactly the middle element at index {}",
                    mid
                );
            }

            /// **Feature: comprehensive-test-coverage, Property 3: Median Algorithm Correctness**
            /// **Validates: Requirements 8.5**
            ///
            /// Property 3: Median Algorithm Correctness (Extreme Values)
            ///
            /// Test with u64::MAX values to ensure no overflow occurs.
            /// When both middle values are u64::MAX, the median should be u64::MAX.
            #[test]
            fn prop_median_u64_max_values(
                count in 2usize..20,
            ) {
                // Create a vector of all u64::MAX values
                let values: Vec<u64> = vec![u64::MAX; count];

                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                // All values are u64::MAX, so median should be u64::MAX
                prop_assert_eq!(
                    summary.median, u64::MAX,
                    "median of all u64::MAX values should be u64::MAX"
                );
                prop_assert_eq!(
                    summary.min, u64::MAX,
                    "min of all u64::MAX values should be u64::MAX"
                );
                prop_assert_eq!(
                    summary.max, u64::MAX,
                    "max of all u64::MAX values should be u64::MAX"
                );
            }

            /// **Feature: comprehensive-test-coverage, Property 3: Median Algorithm Correctness**
            /// **Validates: Requirements 8.5**
            ///
            /// Property 3: Median Algorithm Correctness (Mixed Large Values)
            ///
            /// Test with a mix of u64::MAX and u64::MAX-1 to verify correct averaging
            /// at the overflow boundary.
            #[test]
            fn prop_median_u64_adjacent_max_values(
                max_count in 1usize..10,
                max_minus_one_count in 1usize..10,
            ) {
                // Create a vector with u64::MAX and u64::MAX-1 values
                let mut values: Vec<u64> = Vec::new();
                for _ in 0..max_count {
                    values.push(u64::MAX);
                }
                for _ in 0..max_minus_one_count {
                    values.push(u64::MAX - 1);
                }

                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                // Sort to compute expected median
                let mut sorted = values.clone();
                sorted.sort_unstable();

                let expected_median = reference_median_u64(&sorted);

                prop_assert_eq!(
                    summary.median, expected_median,
                    "median should match reference for mix of u64::MAX and u64::MAX-1"
                );

                // Verify ordering invariant
                prop_assert!(
                    summary.min <= summary.median && summary.median <= summary.max,
                    "ordering invariant should hold: {} <= {} <= {}",
                    summary.min, summary.median, summary.max
                );
            }
        }

        // =====================================================================
        // Property 2: Warmup Sample Exclusion
        // =====================================================================

        /// Helper to generate a non-warmup sample with arbitrary wall_ms
        fn non_warmup_sample(wall_ms: u64) -> Sample {
            Sample {
                wall_ms,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }
        }

        /// Helper to generate a warmup sample with arbitrary wall_ms
        fn warmup_sample(wall_ms: u64) -> Sample {
            Sample {
                wall_ms,
                exit_code: 0,
                warmup: true,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }
        }

        proptest! {
            /// **Validates: Requirements 3.4**
            ///
            /// Property 2: Warmup Sample Exclusion
            ///
            /// For any list of samples containing both warmup and non-warmup samples,
            /// the computed statistics SHALL only reflect non-warmup samples.
            /// Adding or modifying warmup samples SHALL NOT change the computed statistics.
            #[test]
            fn prop_warmup_samples_excluded_from_stats(
                // Generate 1-20 non-warmup sample wall_ms values
                non_warmup_wall_ms in prop::collection::vec(1u64..10000, 1..20),
                // Generate 0-10 warmup sample wall_ms values (can be any values)
                warmup_wall_ms in prop::collection::vec(any::<u64>(), 0..10),
            ) {
                // Create non-warmup samples
                let non_warmup_samples: Vec<Sample> = non_warmup_wall_ms
                    .iter()
                    .map(|&ms| non_warmup_sample(ms))
                    .collect();

                // Create warmup samples
                let warmup_samples: Vec<Sample> = warmup_wall_ms
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Compute stats with only non-warmup samples
                let stats_without_warmup = compute_stats(&non_warmup_samples, None)
                    .expect("non-empty non-warmup samples should succeed");

                // Combine non-warmup and warmup samples
                let mut combined_samples = non_warmup_samples.clone();
                combined_samples.extend(warmup_samples.clone());

                // Compute stats with combined samples (warmup + non-warmup)
                let stats_with_warmup = compute_stats(&combined_samples, None)
                    .expect("combined samples with non-warmup should succeed");

                // Property: Statistics should be identical regardless of warmup samples
                prop_assert_eq!(
                    stats_without_warmup.wall_ms, stats_with_warmup.wall_ms,
                    "wall_ms stats should be identical with or without warmup samples"
                );
                prop_assert_eq!(
                    stats_without_warmup.max_rss_kb, stats_with_warmup.max_rss_kb,
                    "max_rss_kb stats should be identical with or without warmup samples"
                );
                prop_assert_eq!(
                    stats_without_warmup.throughput_per_s, stats_with_warmup.throughput_per_s,
                    "throughput_per_s stats should be identical with or without warmup samples"
                );
            }

            /// **Validates: Requirements 3.4**
            ///
            /// Property 2: Warmup Sample Exclusion (modification variant)
            ///
            /// Modifying warmup sample values SHALL NOT change the computed statistics.
            #[test]
            fn prop_modifying_warmup_samples_does_not_affect_stats(
                // Generate 1-10 non-warmup sample wall_ms values
                non_warmup_wall_ms in prop::collection::vec(1u64..10000, 1..10),
                // Generate 1-5 warmup sample wall_ms values (original)
                warmup_wall_ms_original in prop::collection::vec(any::<u64>(), 1..5),
                // Generate 1-5 warmup sample wall_ms values (modified - different values)
                warmup_wall_ms_modified in prop::collection::vec(any::<u64>(), 1..5),
            ) {
                // Create non-warmup samples
                let non_warmup_samples: Vec<Sample> = non_warmup_wall_ms
                    .iter()
                    .map(|&ms| non_warmup_sample(ms))
                    .collect();

                // Create original warmup samples
                let warmup_samples_original: Vec<Sample> = warmup_wall_ms_original
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Create modified warmup samples (different values)
                let warmup_samples_modified: Vec<Sample> = warmup_wall_ms_modified
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Combine with original warmup samples
                let mut samples_with_original_warmup = non_warmup_samples.clone();
                samples_with_original_warmup.extend(warmup_samples_original);

                // Combine with modified warmup samples
                let mut samples_with_modified_warmup = non_warmup_samples.clone();
                samples_with_modified_warmup.extend(warmup_samples_modified);

                // Compute stats with original warmup samples
                let stats_original = compute_stats(&samples_with_original_warmup, None)
                    .expect("samples with original warmup should succeed");

                // Compute stats with modified warmup samples
                let stats_modified = compute_stats(&samples_with_modified_warmup, None)
                    .expect("samples with modified warmup should succeed");

                // Property: Statistics should be identical regardless of warmup sample values
                prop_assert_eq!(
                    stats_original.wall_ms, stats_modified.wall_ms,
                    "wall_ms stats should be identical regardless of warmup sample values"
                );
            }

            /// **Validates: Requirements 3.4**
            ///
            /// Property 2: Warmup Sample Exclusion (only warmup samples error)
            ///
            /// If all samples are warmup samples, compute_stats SHALL return an error.
            #[test]
            fn prop_only_warmup_samples_returns_error(
                warmup_wall_ms in prop::collection::vec(any::<u64>(), 1..10),
            ) {
                // Create only warmup samples
                let warmup_only_samples: Vec<Sample> = warmup_wall_ms
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Compute stats should fail with NoSamples error
                let result = compute_stats(&warmup_only_samples, None);

                prop_assert!(
                    result.is_err(),
                    "compute_stats should return error when all samples are warmup"
                );

                // Verify it's specifically a NoSamples error
                match result {
                    Err(DomainError::NoSamples) => { /* expected */ }
                    Err(other) => prop_assert!(false, "expected NoSamples error, got: {:?}", other),
                    Ok(_) => prop_assert!(false, "expected error, got Ok"),
                }
            }
        }

        // =====================================================================
        // Property 4: Metric Status Determination
        // =====================================================================

        /// Helper to compute expected regression value based on direction.
        ///
        /// For Direction::Lower: regression = max(0, (current - baseline) / baseline)
        /// For Direction::Higher: regression = max(0, (baseline - current) / baseline)
        fn compute_regression(baseline: f64, current: f64, direction: Direction) -> f64 {
            let pct = (current - baseline) / baseline;
            match direction {
                Direction::Lower => pct.max(0.0),
                Direction::Higher => (-pct).max(0.0),
            }
        }

        /// Helper to compute expected status based on regression and thresholds.
        fn expected_status(regression: f64, threshold: f64, warn_threshold: f64) -> MetricStatus {
            if regression > threshold {
                MetricStatus::Fail
            } else if regression >= warn_threshold {
                MetricStatus::Warn
            } else {
                MetricStatus::Pass
            }
        }

        /// Strategy to generate valid threshold pairs where warn_threshold <= threshold.
        fn threshold_pair_strategy() -> impl Strategy<Value = (f64, f64)> {
            // Generate threshold in range (0.0, 1.0] and warn_factor in range [0.0, 1.0]
            (0.01f64..1.0, 0.0f64..=1.0).prop_map(|(threshold, warn_factor)| {
                let warn_threshold = threshold * warn_factor;
                (threshold, warn_threshold)
            })
        }

        /// Strategy to generate a valid baseline value (must be > 0).
        fn baseline_strategy() -> impl Strategy<Value = f64> {
            // Use positive values, avoiding very small values that could cause precision issues
            1.0f64..10000.0
        }

        /// Strategy to generate a current value (can be any positive value).
        fn current_strategy() -> impl Strategy<Value = f64> {
            // Use positive values
            0.1f64..20000.0
        }

        proptest! {
            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination
            ///
            /// For any baseline value, current value, threshold, warn_threshold, and direction:
            /// - If regression > threshold, status SHALL be Fail
            /// - If warn_threshold <= regression <= threshold, status SHALL be Warn
            /// - If regression < warn_threshold, status SHALL be Pass
            #[test]
            fn prop_metric_status_determination_lower_is_better(
                baseline in baseline_strategy(),
                current in current_strategy(),
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
                let direction = Direction::Lower;

                // Create stats for baseline and current
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(baseline as u64, baseline as u64, baseline as u64),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };

                let current_stats = Stats {
                    wall_ms: U64Summary::new(current as u64, current as u64, current as u64),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };

                // Create budget with the generated thresholds
                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                        threshold,
                        warn_threshold,
                        direction,
                    },
                );

                // Compare stats
                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed with valid inputs");

                // Get the delta for WallMs
                let delta = comparison.deltas.get(&Metric::WallMs)
                    .expect("WallMs delta should exist");

                // Verify the status matches expected
                // Note: We use the actual median values (as u64) for comparison,
                // so we need to recompute expected based on actual values used
                let actual_baseline = baseline_stats.wall_ms.median as f64;
                let actual_current = current_stats.wall_ms.median as f64;
                let actual_regression = compute_regression(actual_baseline, actual_current, direction);
                let actual_expected = expected_status(actual_regression, threshold, warn_threshold);

                prop_assert_eq!(
                    delta.status, actual_expected,
                    "Status mismatch for Direction::Lower: baseline={}, current={}, regression={}, threshold={}, warn_threshold={}",
                    actual_baseline, actual_current, actual_regression, threshold, warn_threshold
                );
            }

            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination (Higher is Better)
            ///
            /// For Direction::Higher (e.g., throughput), regression is computed as
            /// max(0, (baseline - current) / baseline), meaning a decrease in value
            /// is a regression.
            #[test]
            fn prop_metric_status_determination_higher_is_better(
                baseline in baseline_strategy(),
                current in current_strategy(),
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
                let direction = Direction::Higher;

                // Create stats for baseline and current using throughput
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(1000, 1000, 1000),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: Some(F64Summary::new(baseline, baseline, baseline)),                };

                let current_stats = Stats {
                    wall_ms: U64Summary::new(1000, 1000, 1000),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: Some(F64Summary::new(current, current, current)),                };

                // Create budget with the generated thresholds
                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::ThroughputPerS,
                    Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                        threshold,
                        warn_threshold,
                        direction,
                    },
                );

                // Compare stats
                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed with valid inputs");

                // Get the delta for ThroughputPerS
                let delta = comparison.deltas.get(&Metric::ThroughputPerS)
                    .expect("ThroughputPerS delta should exist");

                // Compute expected regression and status
                let regression = compute_regression(baseline, current, direction);
                let expected = expected_status(regression, threshold, warn_threshold);

                prop_assert_eq!(
                    delta.status, expected,
                    "Status mismatch for Direction::Higher: baseline={}, current={}, regression={}, threshold={}, warn_threshold={}",
                    baseline, current, regression, threshold, warn_threshold
                );
            }

            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination (Regression is non-negative)
            ///
            /// The regression value SHALL always be >= 0, regardless of whether
            /// performance improved or degraded.
            #[test]
            fn prop_regression_is_non_negative(
                baseline in baseline_strategy(),
                current in current_strategy(),
                (threshold, warn_threshold) in threshold_pair_strategy(),
                direction_lower in any::<bool>(),
            ) {
                let direction = if direction_lower { Direction::Lower } else { Direction::Higher };

                // Create appropriate stats based on direction
                let (baseline_stats, current_stats, metric, budgets) = if direction_lower {
                    let bs = Stats {
                        wall_ms: U64Summary::new(baseline as u64, baseline as u64, baseline as u64),
                        cpu_ms: None,
                        page_faults: None,
                        ctx_switches: None,
                        max_rss_kb: None,
                        io_read_bytes: None,
                        io_write_bytes: None,
                        network_packets: None,
                        energy_uj: None,
                        binary_bytes: None,
                        throughput_per_s: None,
                        };                    let cs = Stats {
                        wall_ms: U64Summary::new(current as u64, current as u64, current as u64),
                        cpu_ms: None,
                        page_faults: None,
                        ctx_switches: None,
                        max_rss_kb: None,
                        io_read_bytes: None,
                        io_write_bytes: None,
                        network_packets: None,
                        energy_uj: None,
                        binary_bytes: None,
                        throughput_per_s: None,
                        };                    let mut b = BTreeMap::new();
                    b.insert(Metric::WallMs, Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,  threshold, warn_threshold, direction });
                    (bs, cs, Metric::WallMs, b)
                } else {
                    let bs = Stats {
                        wall_ms: U64Summary::new(1000, 1000, 1000),
                        cpu_ms: None,
                        page_faults: None,
                        ctx_switches: None,
                        max_rss_kb: None,
                        io_read_bytes: None,
                        io_write_bytes: None,
                        network_packets: None,
                        energy_uj: None,
                        binary_bytes: None,
                        throughput_per_s: Some(F64Summary::new(baseline, baseline, baseline)),                    };
                    let cs = Stats {
                        wall_ms: U64Summary::new(1000, 1000, 1000),
                        cpu_ms: None,
                        page_faults: None,
                        ctx_switches: None,
                        max_rss_kb: None,
                        io_read_bytes: None,
                        io_write_bytes: None,
                        network_packets: None,
                        energy_uj: None,
                        binary_bytes: None,
                        throughput_per_s: Some(F64Summary::new(current, current, current)),                    };
                    let mut b = BTreeMap::new();
                    b.insert(Metric::ThroughputPerS, Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,  threshold, warn_threshold, direction });
                    (bs, cs, Metric::ThroughputPerS, b)
                };

                // Compare stats
                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed with valid inputs");

                // Get the delta
                let delta = comparison.deltas.get(&metric)
                    .expect("delta should exist");

                // Property: regression is always >= 0
                prop_assert!(
                    delta.regression >= 0.0,
                    "Regression should be non-negative, got: {} for baseline={}, current={}, direction={:?}",
                    delta.regression, baseline, current, direction
                );
            }

            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination (Status boundaries)
            ///
            /// Verify the exact boundary conditions:
            /// - regression == threshold should be Warn (not Fail)
            /// - regression == warn_threshold should be Warn (not Pass)
            #[test]
            fn prop_status_boundary_conditions(
                baseline in 100.0f64..1000.0,
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(1000, 1000, 1000),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: Some(F64Summary::new(baseline, baseline, baseline)),                };

                // For Direction::Higher, regression = max(0, (baseline - current) / baseline)
                // To get regression = threshold, we need: (baseline - current) / baseline = threshold
                // So: current = baseline * (1 - threshold)
                let current_at_threshold_higher = baseline * (1.0 - threshold);

                // Only test if current would be positive
                if current_at_threshold_higher > 0.0 {
                    let current_stats = Stats {
                        wall_ms: U64Summary::new(1000, 1000, 1000),
                        cpu_ms: None,
                        page_faults: None,
                        ctx_switches: None,
                        max_rss_kb: None,
                        io_read_bytes: None,
                        io_write_bytes: None,
                        network_packets: None,
                        energy_uj: None,
                        binary_bytes: None,
                        throughput_per_s: Some(F64Summary::new(current_at_threshold_higher, current_at_threshold_higher, current_at_threshold_higher)),
                    };

                    let mut budgets = BTreeMap::new();
                    budgets.insert(
                        Metric::ThroughputPerS,
                        Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                            threshold,
                            warn_threshold,
                            direction: Direction::Higher,
                        },
                    );

                    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                        .expect("compare_stats should succeed");

                    let delta = comparison.deltas.get(&Metric::ThroughputPerS)
                        .expect("delta should exist");

                    // At exactly threshold, status should be Warn (not Fail)
                    // because the condition is regression > threshold for Fail
                    prop_assert!(
                        delta.status != MetricStatus::Fail || delta.regression > threshold,
                        "At regression={} (threshold={}), status should not be Fail unless regression > threshold",
                        delta.regression, threshold
                    );
                }
            }
        }

        // =====================================================================
        // Property 5: Verdict Aggregation
        // =====================================================================

        /// Strategy to generate a random MetricStatus.
        fn metric_status_strategy() -> impl Strategy<Value = MetricStatus> {
            prop_oneof![
                Just(MetricStatus::Pass),
                Just(MetricStatus::Warn),
                Just(MetricStatus::Fail),
                Just(MetricStatus::Skip),
            ]
        }

        /// Compute the expected verdict status from a set of metric statuses.
        ///
        /// - If any metric has Fail status, verdict SHALL be Fail
        /// - Else if any metric has Warn status, verdict SHALL be Warn
        /// - Else verdict SHALL be Pass
        fn expected_verdict_status(statuses: &[MetricStatus]) -> VerdictStatus {
            if statuses.contains(&MetricStatus::Fail) {
                VerdictStatus::Fail
            } else if statuses.contains(&MetricStatus::Warn) {
                VerdictStatus::Warn
            } else if statuses.contains(&MetricStatus::Pass) {
                VerdictStatus::Pass
            } else {
                VerdictStatus::Skip
            }
        }

        /// Helper to create Stats with a specific wall_ms median value and optional CV.
        fn make_stats_with_wall_ms_and_cv(median: u64, cv: Option<f64>) -> Stats {
            let (mean, stddev) = if let Some(cv_val) = cv {
                let mean = median as f64;
                let stddev = mean * cv_val;
                (Some(mean), Some(stddev))
            } else {
                (None, None)
            };

            Stats {
                wall_ms: U64Summary {
                    median,
                    min: median,
                    max: median,
                    mean,
                    stddev,
                },
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(U64Summary {
                    median,
                    min: median,
                    max: median,
                    mean,
                    stddev,
                }),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: Some(F64Summary {
                    median: median as f64,
                    min: median as f64,
                    max: median as f64,
                    mean,
                    stddev,
                }),
            }
        }

        fn make_stats_with_wall_ms(median: u64) -> Stats {
            make_stats_with_wall_ms_and_cv(median, None)
        }

        /// Helper to compute the current value needed to achieve a specific status.
        ///
        /// Given a baseline, threshold, warn_threshold, and desired status,
        /// returns a current value that will produce that status.
        fn current_for_status(
            baseline: u64,
            threshold: f64,
            warn_threshold: f64,
            status: MetricStatus,
        ) -> u64 {
            let baseline_f = baseline as f64;
            match status {
                // For Pass: regression < warn_threshold
                // regression = (current - baseline) / baseline
                // So current = baseline * (1 + regression)
                // Use regression = 0 (no change) for Pass
                MetricStatus::Pass => baseline,

                // For Warn: warn_threshold <= regression <= threshold
                // Use midpoint between warn_threshold and threshold
                MetricStatus::Warn => {
                    let regression = (warn_threshold + threshold) / 2.0;
                    (baseline_f * (1.0 + regression)).ceil() as u64
                }

                // For Fail: regression > threshold
                // Use threshold + 0.1 to ensure we exceed it
                MetricStatus::Fail => {
                    let regression = threshold + 0.1;
                    (baseline_f * (1.0 + regression)).ceil() as u64
                }

                // For Skip: return baseline (same as Pass for this helper)
                MetricStatus::Skip => baseline,
            }
        }

        proptest! {
            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation
            ///
            /// For any set of metric statuses:
            /// - If any metric has Fail status, verdict SHALL be Fail
            /// - Else if any metric has Warn status, verdict SHALL be Warn
            /// - Else verdict SHALL be Pass
            #[test]
            fn prop_verdict_aggregation_single_metric(
                status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                let baseline_stats = make_stats_with_wall_ms(baseline);
                let current_value = current_for_status(baseline, threshold, warn_threshold, status);
                let current_cv = if status == MetricStatus::Skip { Some(0.5) } else { None };
                let current_stats = make_stats_with_wall_ms_and_cv(current_value, current_cv);

                let mut budget = Budget {
                    noise_threshold: None,
                    noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold,
                    warn_threshold,
                    direction: Direction::Lower,
                };

                if status == MetricStatus::Skip {
                    budget.noise_threshold = Some(0.1);
                    budget.noise_policy = perfgate_types::NoisePolicy::Skip;
                }

                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, budget);

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verify the verdict matches the expected aggregation
                let expected = expected_verdict_status(&[status]);
                prop_assert_eq!(
                    comparison.verdict.status, expected,
                    "Verdict should be {:?} when single metric status is {:?}",
                    expected, status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Multiple Metrics)
            ///
            /// Test with multiple metrics to verify aggregation across all metrics.
            #[test]
            fn prop_verdict_aggregation_multiple_metrics(
                wall_ms_status in metric_status_strategy(),
                max_rss_status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with both wall_ms and max_rss_kb
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(baseline, baseline, baseline),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary::new(baseline, baseline, baseline)),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,                };

                // Compute current values to achieve desired statuses
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, wall_ms_status);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, max_rss_status);

                let wall_cv = if wall_ms_status == MetricStatus::Skip { Some(0.5) } else { None };
                let rss_cv = if max_rss_status == MetricStatus::Skip { Some(0.5) } else { None };

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: wall_ms_current,
                        min: wall_ms_current,
                        max: wall_ms_current,
                        mean: wall_cv.map(|_cv| wall_ms_current as f64),
                        stddev: wall_cv.map(|cv| (wall_ms_current as f64) * cv),
                    },
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary {
                        median: max_rss_current,
                        min: max_rss_current,
                        max: max_rss_current,
                        mean: rss_cv.map(|_cv| max_rss_current as f64),
                        stddev: rss_cv.map(|cv| (max_rss_current as f64) * cv),
                    }),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };

                let mut wall_budget = Budget {
                    noise_threshold: None,
                    noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold,
                    warn_threshold,
                    direction: Direction::Lower,
                };
                if wall_ms_status == MetricStatus::Skip {
                    wall_budget.noise_threshold = Some(0.1);
                    wall_budget.noise_policy = perfgate_types::NoisePolicy::Skip;
                }

                let mut rss_budget = Budget {
                    noise_threshold: None,
                    noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold,
                    warn_threshold,
                    direction: Direction::Lower,
                };
                if max_rss_status == MetricStatus::Skip {
                    rss_budget.noise_threshold = Some(0.1);
                    rss_budget.noise_policy = perfgate_types::NoisePolicy::Skip;
                }

                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, wall_budget);
                budgets.insert(Metric::MaxRssKb, rss_budget);

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verify the verdict matches the expected aggregation
                let expected = expected_verdict_status(&[wall_ms_status, max_rss_status]);
                prop_assert_eq!(
                    comparison.verdict.status, expected,
                    "Verdict should be {:?} when metric statuses are [{:?}, {:?}]",
                    expected, wall_ms_status, max_rss_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Three Metrics)
            ///
            /// Test with all three metric types to verify comprehensive aggregation.
            #[test]
            fn prop_verdict_aggregation_three_metrics(
                wall_ms_status in metric_status_strategy(),
                max_rss_status in metric_status_strategy(),
                throughput_status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let baseline_throughput = 100.0f64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with all three metrics
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(baseline, baseline, baseline),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary::new(baseline, baseline, baseline)),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: Some(F64Summary::new(baseline_throughput, baseline_throughput, baseline_throughput)),                };

                // Compute current values to achieve desired statuses
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, wall_ms_status);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, max_rss_status);

                let wall_cv = if wall_ms_status == MetricStatus::Skip { Some(0.5) } else { None };
                let rss_cv = if max_rss_status == MetricStatus::Skip { Some(0.5) } else { None };
                let throughput_cv = if throughput_status == MetricStatus::Skip { Some(0.5) } else { None };

                // For throughput (higher is better), we need to invert the logic
                // Pass: current >= baseline (no regression)
                // Warn: current = baseline * (1 - midpoint of warn/threshold)
                // Fail: current = baseline * (1 - (threshold + 0.1))
                let throughput_current = match throughput_status {
                    MetricStatus::Pass => baseline_throughput,
                    MetricStatus::Warn => {
                        let regression = (warn_threshold + threshold) / 2.0;
                        baseline_throughput * (1.0 - regression)
                    }
                    MetricStatus::Fail => {
                        let regression = threshold + 0.1;
                        baseline_throughput * (1.0 - regression)
                    }
                    MetricStatus::Skip => baseline_throughput,
                };

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: wall_ms_current,
                        min: wall_ms_current,
                        max: wall_ms_current,
                        mean: wall_cv.map(|_cv| wall_ms_current as f64),
                        stddev: wall_cv.map(|cv| (wall_ms_current as f64) * cv),
                    },
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary {
                        median: max_rss_current,
                        min: max_rss_current,
                        max: max_rss_current,
                        mean: rss_cv.map(|_cv| max_rss_current as f64),
                        stddev: rss_cv.map(|cv| (max_rss_current as f64) * cv),
                    }),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: Some(F64Summary {
                        median: throughput_current,
                        min: throughput_current,
                        max: throughput_current,
                        mean: throughput_cv.map(|_cv| throughput_current),
                        stddev: throughput_cv.map(|cv| (throughput_current) * cv),
                    }),
                };

                let mut wall_budget = Budget {
                    noise_threshold: None,
                    noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold,
                    warn_threshold,
                    direction: Direction::Lower,
                };
                if wall_ms_status == MetricStatus::Skip {
                    wall_budget.noise_threshold = Some(0.1);
                    wall_budget.noise_policy = perfgate_types::NoisePolicy::Skip;
                }

                let mut rss_budget = Budget {
                    noise_threshold: None,
                    noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold,
                    warn_threshold,
                    direction: Direction::Lower,
                };
                if max_rss_status == MetricStatus::Skip {
                    rss_budget.noise_threshold = Some(0.1);
                    rss_budget.noise_policy = perfgate_types::NoisePolicy::Skip;
                }

                let mut throughput_budget = Budget {
                    noise_threshold: None,
                    noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold,
                    warn_threshold,
                    direction: Direction::Higher,
                };
                if throughput_status == MetricStatus::Skip {
                    throughput_budget.noise_threshold = Some(0.1);
                    throughput_budget.noise_policy = perfgate_types::NoisePolicy::Skip;
                }

                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, wall_budget);
                budgets.insert(Metric::MaxRssKb, rss_budget);
                budgets.insert(Metric::ThroughputPerS, throughput_budget);

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verify the verdict matches the expected aggregation
                let expected = expected_verdict_status(&[wall_ms_status, max_rss_status, throughput_status]);
                prop_assert_eq!(
                    comparison.verdict.status, expected,
                    "Verdict should be {:?} when metric statuses are [{:?}, {:?}, {:?}]",
                    expected, wall_ms_status, max_rss_status, throughput_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Fail dominates)
            ///
            /// If any metric has Fail status, the verdict SHALL be Fail,
            /// regardless of other metric statuses.
            #[test]
            fn prop_verdict_fail_dominates(
                other_status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with both wall_ms and max_rss_kb
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(baseline, baseline, baseline),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary::new(baseline, baseline, baseline)),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,                };

                // wall_ms will be Fail, max_rss will be the random status
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, MetricStatus::Fail);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, other_status);

                let current_stats = Stats {
                    wall_ms: U64Summary::new(wall_ms_current, wall_ms_current, wall_ms_current),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary::new(max_rss_current, max_rss_current, max_rss_current)),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                budgets.insert(
                    Metric::MaxRssKb,
                    Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verdict should always be Fail when any metric is Fail
                prop_assert_eq!(
                    comparison.verdict.status, VerdictStatus::Fail,
                    "Verdict should be Fail when any metric has Fail status, regardless of other_status={:?}",
                    other_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Warn without Fail)
            ///
            /// If no metric has Fail status but at least one has Warn status,
            /// the verdict SHALL be Warn.
            #[test]
            fn prop_verdict_warn_without_fail(
                // Generate only Pass or Warn statuses (no Fail)
                other_status in prop_oneof![Just(MetricStatus::Pass), Just(MetricStatus::Warn)],
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with both wall_ms and max_rss_kb
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(baseline, baseline, baseline),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary::new(baseline, baseline, baseline)),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,                };

                // wall_ms will be Warn, max_rss will be Pass or Warn
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, MetricStatus::Warn);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, other_status);

                let current_stats = Stats {
                    wall_ms: U64Summary::new(wall_ms_current, wall_ms_current, wall_ms_current),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(U64Summary::new(max_rss_current, max_rss_current, max_rss_current)),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                budgets.insert(
                    Metric::MaxRssKb,
                    Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verdict should be Warn when at least one metric is Warn and none are Fail
                prop_assert_eq!(
                    comparison.verdict.status, VerdictStatus::Warn,
                    "Verdict should be Warn when at least one metric is Warn and none are Fail, other_status={:?}",
                    other_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (All Pass)
            ///
            /// If all metrics have Pass status, the verdict SHALL be Pass.
            #[test]
            fn prop_verdict_all_pass(
                // Generate 1-3 metrics, all with Pass status
                num_metrics in 1usize..=3,
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let baseline_throughput = 100.0f64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // All metrics will be Pass (current == baseline, no regression)
                let baseline_stats = Stats {
                    wall_ms: U64Summary::new(baseline, baseline, baseline),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: if num_metrics >= 2 {
                        Some(U64Summary::new(baseline, baseline, baseline))
                    } else {
                        None
                    },
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: if num_metrics >= 3 {
                        Some(F64Summary::new(baseline_throughput, baseline_throughput, baseline_throughput))
                    } else {
                        None
                    },
                };

                // Current stats are same as baseline (Pass status)
                let current_stats = baseline_stats.clone();

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                if num_metrics >= 2 {
                    budgets.insert(
                        Metric::MaxRssKb,
                        Budget {
                            noise_threshold: None,
                            noise_policy: perfgate_types::NoisePolicy::Ignore,
                            threshold,
                            warn_threshold,
                            direction: Direction::Lower,
                        },                    );
                }
                if num_metrics >= 3 {
                    budgets.insert(
                        Metric::ThroughputPerS,
                        Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                            threshold,
                            warn_threshold,
                            direction: Direction::Higher,
                        },
                    );
                }

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verdict should be Pass when all metrics are Pass
                prop_assert_eq!(
                    comparison.verdict.status, VerdictStatus::Pass,
                    "Verdict should be Pass when all {} metrics have Pass status",
                    num_metrics
                );

                // Also verify the counts are correct
                prop_assert_eq!(
                    comparison.verdict.counts.pass, num_metrics as u32,
                    "Pass count should equal number of metrics"
                );
                prop_assert_eq!(
                    comparison.verdict.counts.warn, 0,
                    "Warn count should be 0"
                );
                prop_assert_eq!(
                    comparison.verdict.counts.fail, 0,
                    "Fail count should be 0"
                );
            }
        }

        // =====================================================================
        // Property: compare_stats determinism
        // =====================================================================

        proptest! {
            #[test]
            fn prop_compare_stats_determinism(
                baseline_wall in 1u64..10000,
                current_wall in 1u64..10000,
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
            let baseline = Stats {
                wall_ms: U64Summary::new(baseline_wall, baseline_wall, baseline_wall),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            };
            let current = Stats {
                wall_ms: U64Summary::new(current_wall, current_wall, current_wall),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            };
                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold, warn_threshold, direction: Direction::Lower,
                });

                let r1 = compare_stats(&baseline, &current, &budgets).unwrap();
                let r2 = compare_stats(&baseline, &current, &budgets).unwrap();
                prop_assert_eq!(r1, r2, "compare_stats must be deterministic");
            }
        }

        // =====================================================================
        // Property: compare_runs determinism
        // =====================================================================

        fn make_run_receipt_for_prop(name: &str, wall: u64) -> RunReceipt {
            use perfgate_types::{BenchMeta, HostInfo, RUN_SCHEMA_V1, RunMeta, ToolInfo};
            let sample = non_warmup_sample(wall);
            let stats = compute_stats(std::slice::from_ref(&sample), None).unwrap();
            RunReceipt {
                schema: RUN_SCHEMA_V1.to_string(),
                tool: ToolInfo {
                    name: "perfgate".into(),
                    version: "test".into(),
                },
                run: RunMeta {
                    id: format!("run-{name}"),
                    started_at: "2024-01-01T00:00:00Z".into(),
                    ended_at: "2024-01-01T00:00:01Z".into(),
                    host: HostInfo {
                        os: "linux".into(),
                        arch: "x86_64".into(),
                        cpu_count: None,
                        memory_bytes: None,
                        hostname_hash: None,
                    },
                },
                bench: BenchMeta {
                    name: name.into(),
                    cwd: None,
                    command: vec!["echo".into()],
                    repeat: 1,
                    warmup: 0,
                    work_units: None,
                    timeout_ms: None,
                },
                samples: vec![sample],
                stats,
            }
        }

        proptest! {
            #[test]
            fn prop_compare_runs_determinism(
                baseline_wall in 1u64..10000,
                current_wall in 1u64..10000,
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
                let baseline = make_run_receipt_for_prop("base", baseline_wall);
                let current = make_run_receipt_for_prop("cur", current_wall);
                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, Budget {
                        noise_threshold: None,
                        noise_policy: perfgate_types::NoisePolicy::Ignore,
                    threshold, warn_threshold, direction: Direction::Lower,
                });
                let stats_map = BTreeMap::new();

                let c1 = compare_runs(&baseline, &current, &budgets, &stats_map, None).unwrap();
                let c2 = compare_runs(&baseline, &current, &budgets, &stats_map, None).unwrap();
                prop_assert_eq!(c1, c2, "compare_runs must be deterministic");
            }
        }

        // =====================================================================
        // Property: derive_report consistency
        // =====================================================================

        fn make_compare_receipt(
            deltas: BTreeMap<Metric, Delta>,
            budgets: BTreeMap<Metric, Budget>,
            verdict: Verdict,
        ) -> CompareReceipt {
            use perfgate_types::{BenchMeta, COMPARE_SCHEMA_V1, CompareRef, ToolInfo};
            CompareReceipt {
                schema: COMPARE_SCHEMA_V1.to_string(),
                tool: ToolInfo {
                    name: "perfgate".into(),
                    version: "test".into(),
                },
                bench: BenchMeta {
                    name: "bench".into(),
                    cwd: None,
                    command: vec!["echo".into()],
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
                verdict,
            }
        }

        fn arb_metric_status() -> impl Strategy<Value = MetricStatus> {
            prop_oneof![
                Just(MetricStatus::Pass),
                Just(MetricStatus::Warn),
                Just(MetricStatus::Fail),
                Just(MetricStatus::Skip),
            ]
        }

        fn arb_delta(status: MetricStatus) -> Delta {
            Delta {
                baseline: 100.0,
                current: 110.0,
                ratio: 1.1,
                pct: 0.1,
                regression: 0.1,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status,
            }
        }

        proptest! {
            #[test]
            fn prop_derive_report_finding_count(
                wall_status in arb_metric_status(),
                rss_status in arb_metric_status(),
                cpu_status in arb_metric_status(),
            ) {
                let statuses = vec![
                    (Metric::WallMs, wall_status),
                    (Metric::MaxRssKb, rss_status),
                    (Metric::CpuMs, cpu_status),
                ];
                let mut deltas = BTreeMap::new();
                let budgets = BTreeMap::new();
                let mut warn_count = 0u32;
                let mut fail_count = 0u32;
                let mut pass_count = 0u32;
                for (m, s) in &statuses {
                    deltas.insert(*m, arb_delta(*s));
                    match s {
                        MetricStatus::Pass => pass_count += 1,
                        MetricStatus::Warn => warn_count += 1,
                        MetricStatus::Fail => fail_count += 1,
                        MetricStatus::Skip => {}
                    }
                }
                let verdict = Verdict {
                    status: if fail_count > 0 { VerdictStatus::Fail }
                            else if warn_count > 0 { VerdictStatus::Warn }
                            else { VerdictStatus::Pass },
                    counts: VerdictCounts { pass: pass_count, warn: warn_count, fail: fail_count, skip: 0 },
                    reasons: vec![],
                };
                let receipt = make_compare_receipt(deltas, budgets, verdict.clone());
                let report = derive_report(&receipt);

                prop_assert_eq!(
                    report.findings.len() as u32,
                    warn_count + fail_count,
                    "finding_count must equal warn_count + fail_count"
                );
            }

            #[test]
            fn prop_derive_report_verdict_consistency(
                wall_status in arb_metric_status(),
                rss_status in arb_metric_status(),
            ) {
                let statuses = vec![
                    (Metric::WallMs, wall_status),
                    (Metric::MaxRssKb, rss_status),
                ];
                let mut deltas = BTreeMap::new();
                let budgets = BTreeMap::new();
                let mut warn_count = 0u32;
                let mut fail_count = 0u32;
                let mut pass_count = 0u32;
                for (m, s) in &statuses {
                    deltas.insert(*m, arb_delta(*s));
                    match s {
                        MetricStatus::Pass => pass_count += 1,
                        MetricStatus::Warn => warn_count += 1,
                        MetricStatus::Fail => fail_count += 1,
                        MetricStatus::Skip => {}
                    }
                }
                let expected_status = if fail_count > 0 { VerdictStatus::Fail }
                    else if warn_count > 0 { VerdictStatus::Warn }
                    else { VerdictStatus::Pass };
                let verdict = Verdict {
                    status: expected_status,
                    counts: VerdictCounts { pass: pass_count, warn: warn_count, fail: fail_count, skip: 0 },
                    reasons: vec![],
                };
                let receipt = make_compare_receipt(deltas, budgets, verdict);
                let report = derive_report(&receipt);

                prop_assert_eq!(
                    report.verdict, expected_status,
                    "report verdict must match worst finding status"
                );
            }
        }

        // =====================================================================
        // Property: summarize_u64/f64 sample_count & median bounds
        // =====================================================================

        proptest! {
            #[test]
            fn prop_summarize_u64_median_between_min_max(
                values in prop::collection::vec(any::<u64>(), 1..100)
            ) {
                let s = summarize_u64(&values).unwrap();
                prop_assert!(s.min <= s.median, "min <= median");
                prop_assert!(s.median <= s.max, "median <= max");
            }

            #[test]
            fn prop_summarize_f64_median_between_min_max(
                values in prop::collection::vec(finite_f64_strategy(), 1..100)
            ) {
                let s = summarize_f64(&values).unwrap();
                prop_assert!(s.min <= s.median, "min <= median");
                prop_assert!(s.median <= s.max, "median <= max");
            }

            #[test]
            fn prop_compute_stats_sample_count(
                walls in prop::collection::vec(1u64..10000, 1..50)
            ) {
                let samples: Vec<Sample> = walls.iter().map(|&w| non_warmup_sample(w)).collect();
                let stats = compute_stats(&samples, None).unwrap();
                let expected = summarize_u64(&walls).unwrap();
                prop_assert_eq!(stats.wall_ms, expected, "stats.wall_ms must match direct summarize");
            }
        }

        // =====================================================================
        // Property: mean is within [min, max] and variance >= 0
        // =====================================================================

        proptest! {
            #[test]
            fn prop_mean_within_min_max_and_variance_non_negative(
                values in prop::collection::vec(1.0f64..10000.0, 1..100)
            ) {
                let (mean, var) = mean_and_variance(&values).expect("finite values");
                let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

                prop_assert!(
                    mean >= min && mean <= max,
                    "mean ({mean}) must be within [{min}, {max}]"
                );
                prop_assert!(var >= 0.0, "variance ({var}) must be >= 0");
            }
        }

        // =====================================================================
        // Property: comparison symmetry – regression flips to improvement
        // =====================================================================

        proptest! {
            /// If baseline=A, current=B shows a regression (regression > 0),
            /// then swapping (baseline=B, current=A) must show no regression
            /// (regression == 0) for the same direction.
            #[test]
            fn prop_comparison_symmetry(
                a in 1u64..10000,
                b in 1u64..10000,
            ) {
                let budget = Budget::new(1.0, 0.5, Direction::Lower);

                let mk = |median: u64| Stats {
                    wall_ms: U64Summary::new(median, median, median),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };

                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, budget);

                let fwd = compare_stats(&mk(a), &mk(b), &budgets).unwrap();
                let rev = compare_stats(&mk(b), &mk(a), &budgets).unwrap();

                let fwd_reg = fwd.deltas[&Metric::WallMs].regression;
                let rev_reg = rev.deltas[&Metric::WallMs].regression;

                // If one direction regresses, the other must not
                if fwd_reg > 0.0 {
                    prop_assert!(
                        rev_reg == 0.0,
                        "fwd regression={fwd_reg} but rev regression={rev_reg} (expected 0)"
                    );
                }
                if rev_reg > 0.0 {
                    prop_assert!(
                        fwd_reg == 0.0,
                        "rev regression={rev_reg} but fwd regression={fwd_reg} (expected 0)"
                    );
                }
            }
        }

        // =====================================================================
        // Property: budget threshold consistency
        // =====================================================================

        proptest! {
            /// threshold=0.0 means any positive regression must fail.
            #[test]
            fn prop_budget_zero_threshold_fails_any_regression(
                baseline in 1u64..10000,
                delta in 1u64..5000,
            ) {
                let current = baseline + delta; // strictly worse (Direction::Lower)
                let budget = Budget::new(0.0, 0.0, Direction::Lower);
                let mk = |v: u64| Stats {
                    wall_ms: U64Summary::new(v, v, v),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };
                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, budget);

                let cmp = compare_stats(&mk(baseline), &mk(current), &budgets).unwrap();
                let status = cmp.deltas[&Metric::WallMs].status;
                prop_assert_eq!(
                    status,
                    MetricStatus::Fail,
                    "threshold=0 must fail any regression (baseline={}, current={})",
                    baseline, current
                );
            }

            /// threshold=1.0 allows up to 100% regression;
            /// a regression <= 100% must not fail.
            #[test]
            fn prop_budget_full_threshold_allows_up_to_100pct(
                baseline in 1u64..10000,
                factor in 0u64..=100, // 0..100% regression
            ) {
                // current = baseline * (1 + factor/100)
                let current = baseline + (baseline * factor) / 100;
                let budget = Budget::new(1.0, 0.5, Direction::Lower);
                let mk = |v: u64| Stats {
                    wall_ms: U64Summary::new(v, v, v),
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                };
                let mut budgets = BTreeMap::new();
                budgets.insert(Metric::WallMs, budget);

                let cmp = compare_stats(&mk(baseline), &mk(current), &budgets).unwrap();
                let status = cmp.deltas[&Metric::WallMs].status;
                prop_assert!(
                    status != MetricStatus::Fail,
                    "threshold=1.0 must not fail <=100% regression \
                     (baseline={baseline}, current={current}, factor={factor}%)"
                );
            }
        }
    }

    #[test]
    fn compute_stats_excludes_warmup() {
        let samples = vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: true,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 200,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ];

        let stats = compute_stats(&samples, None).unwrap();
        let expected = U64Summary {
            median: 200,
            min: 200,
            max: 200,
            mean: Some(200.0),
            stddev: Some(0.0),
        };
        assert_eq!(stats.wall_ms, expected);
    }

    // =========================================================================
    // CPU Time (cpu_ms) Tests
    // =========================================================================

    /// Test that compute_stats correctly computes cpu_ms summary from samples.
    #[test]
    fn compute_stats_computes_cpu_ms_summary() {
        let samples = vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(50),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 110,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(60),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 105,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(55),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ];

        let stats = compute_stats(&samples, None).unwrap();

        // cpu_ms should be present with correct statistics
        assert!(stats.cpu_ms.is_some(), "cpu_ms stats should be present");
        let cpu_stats = stats.cpu_ms.unwrap();
        assert_eq!(cpu_stats.min, 50, "cpu_ms min should be 50");
        assert_eq!(cpu_stats.max, 60, "cpu_ms max should be 60");
        assert_eq!(cpu_stats.median, 55, "cpu_ms median should be 55");
    }

    /// Test that compute_stats returns None for cpu_ms when samples don't have it.
    #[test]
    fn compute_stats_cpu_ms_none_when_samples_missing_cpu() {
        let samples = vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(1024),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 110,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(1028),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ];

        let stats = compute_stats(&samples, None).unwrap();

        // cpu_ms should be None since samples don't have cpu_ms
        assert!(
            stats.cpu_ms.is_none(),
            "cpu_ms stats should be None when samples lack cpu_ms"
        );
        // max_rss_kb should still be computed
        assert!(
            stats.max_rss_kb.is_some(),
            "max_rss_kb should still be present"
        );
    }

    /// Test that compute_stats excludes warmup samples from cpu_ms calculation.
    #[test]
    fn compute_stats_cpu_ms_excludes_warmup() {
        let samples = vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: true, // warmup - should be excluded
                timed_out: false,
                cpu_ms: Some(1000), // High value that would skew results
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(50),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(60),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ];

        let stats = compute_stats(&samples, None).unwrap();

        let cpu_stats = stats.cpu_ms.expect("cpu_ms should be present");
        // Warmup sample with cpu_ms=1000 should be excluded
        assert_eq!(
            cpu_stats.min, 50,
            "cpu_ms min should be 50 (excluding warmup)"
        );
        assert_eq!(
            cpu_stats.max, 60,
            "cpu_ms max should be 60 (excluding warmup)"
        );
    }

    /// Test that compare_stats correctly compares cpu_ms values.
    #[test]
    fn compare_stats_cpu_ms_regression_detection() {
        let baseline = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(50, 50, 50)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        // Current has 100% increase in cpu_ms (50 -> 100)
        let current = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(100, 100, 100)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::CpuMs, Budget::new(0.20, 0.10, Direction::Lower));

        let comparison = compare_stats(&baseline, &current, &budgets).unwrap();

        // Should have cpu_ms delta
        let cpu_delta = comparison
            .deltas
            .get(&Metric::CpuMs)
            .expect("cpu_ms delta should exist");

        // 100% regression should fail the 20% threshold
        assert_eq!(
            cpu_delta.status,
            MetricStatus::Fail,
            "100% cpu_ms regression should fail 20% threshold"
        );
        assert!(
            (cpu_delta.regression - 1.0).abs() < 0.001,
            "regression should be ~1.0 (100%)"
        );
        assert_eq!(cpu_delta.baseline, 50.0, "baseline should be 50");
        assert_eq!(cpu_delta.current, 100.0, "current should be 100");
    }

    /// Test that compare_stats passes when cpu_ms improvement (decrease).
    #[test]
    fn compare_stats_cpu_ms_improvement_passes() {
        let baseline = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(100, 100, 100)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        // Current has 50% decrease in cpu_ms (100 -> 50) - improvement!
        let current = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(50, 50, 50)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::CpuMs, Budget::new(0.20, 0.10, Direction::Lower));

        let comparison = compare_stats(&baseline, &current, &budgets).unwrap();

        let cpu_delta = comparison
            .deltas
            .get(&Metric::CpuMs)
            .expect("cpu_ms delta should exist");

        // Improvement should pass (regression = 0 since current < baseline for Lower direction)
        assert_eq!(
            cpu_delta.status,
            MetricStatus::Pass,
            "cpu_ms improvement should pass"
        );
        assert_eq!(
            cpu_delta.regression, 0.0,
            "regression should be 0 for improvement"
        );
    }

    /// Test that compare_stats skips cpu_ms when only baseline has it.
    #[test]
    fn compare_stats_skips_cpu_ms_when_only_baseline_has_it() {
        let baseline = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(50, 50, 50)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let current = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: None, // No cpu_ms in current
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::CpuMs, Budget::new(0.20, 0.10, Direction::Lower));

        let comparison = compare_stats(&baseline, &current, &budgets).unwrap();

        // cpu_ms delta should NOT exist (skipped because current lacks it)
        assert!(
            !comparison.deltas.contains_key(&Metric::CpuMs),
            "cpu_ms delta should be skipped when current lacks cpu_ms"
        );
    }

    /// Test that compare_stats skips cpu_ms when only current has it.
    #[test]
    fn compare_stats_skips_cpu_ms_when_only_current_has_it() {
        let baseline = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: None, // No cpu_ms in baseline
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let current = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(50, 50, 50)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::CpuMs, Budget::new(0.20, 0.10, Direction::Lower));

        let comparison = compare_stats(&baseline, &current, &budgets).unwrap();

        // cpu_ms delta should NOT exist (skipped because baseline lacks it)
        assert!(
            !comparison.deltas.contains_key(&Metric::CpuMs),
            "cpu_ms delta should be skipped when baseline lacks cpu_ms"
        );
    }

    /// Test that compare_stats warns on cpu_ms when within warn threshold.
    #[test]
    fn compare_stats_cpu_ms_warns_within_threshold() {
        let baseline = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(100, 100, 100)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        // Current has 15% increase in cpu_ms (100 -> 115)
        let current = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
            cpu_ms: Some(U64Summary::new(115, 115, 115)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::CpuMs, Budget::new(0.20, 0.10, Direction::Lower));

        let comparison = compare_stats(&baseline, &current, &budgets).unwrap();

        let cpu_delta = comparison
            .deltas
            .get(&Metric::CpuMs)
            .expect("cpu_ms delta should exist");

        // 15% regression should warn (between 10% and 20%)
        assert_eq!(
            cpu_delta.status,
            MetricStatus::Warn,
            "15% cpu_ms regression should warn (10% < 15% < 20%)"
        );
    }

    #[test]
    fn compare_lower_is_worse_regression_is_positive_pct() {
        let baseline = Stats {
            wall_ms: U64Summary::new(1000, 1000, 1000),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let current = Stats {
            wall_ms: U64Summary::new(1100, 1100, 1100),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        };
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::WallMs, Budget::new(0.20, 0.18, Direction::Lower));

        let c = compare_stats(&baseline, &current, &budgets).unwrap();
        let d = c.deltas.get(&Metric::WallMs).unwrap();
        assert!(d.pct > 0.0);
        assert_eq!(d.status, MetricStatus::Pass);
    }

    #[test]
    fn compare_higher_is_better_regression_is_negative_pct() {
        let baseline = Stats {
            wall_ms: U64Summary::new(1000, 1000, 1000),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: Some(F64Summary::new(110.0, 110.0, 110.0)),
        };
        let current = Stats {
            wall_ms: U64Summary::new(1000, 1000, 1000),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: Some(F64Summary::new(100.0, 100.0, 100.0)),
        };
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::ThroughputPerS,
            Budget::new(0.15, 0.135, Direction::Higher),
        );

        let c = compare_stats(&baseline, &current, &budgets).unwrap();
        let d = c.deltas.get(&Metric::ThroughputPerS).unwrap();
        assert!(d.pct < 0.0);
        assert_eq!(d.status, MetricStatus::Pass);
    }

    // =========================================================================
    // Unit Tests for Domain Error Conditions
    // **Validates: Requirements 11.1, 11.2**
    // =========================================================================

    mod error_condition_tests {
        use super::*;

        // ---------------------------------------------------------------------
        // DomainError::NoSamples Tests
        // ---------------------------------------------------------------------

        /// Test that summarize_u64 returns DomainError::NoSamples for empty input.
        /// **Validates: Requirements 11.1**
        #[test]
        fn summarize_u64_empty_input_returns_no_samples_error() {
            let result = summarize_u64(&[]);

            assert!(
                result.is_err(),
                "summarize_u64 should return error for empty input"
            );
            match result {
                Err(StatsError::NoSamples) => { /* expected */ }
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that summarize_f64 returns DomainError::NoSamples for empty input.
        /// **Validates: Requirements 11.1**
        #[test]
        fn summarize_f64_empty_input_returns_no_samples_error() {
            let result = summarize_f64(&[]);

            assert!(
                result.is_err(),
                "summarize_f64 should return error for empty input"
            );
            match result {
                Err(StatsError::NoSamples) => { /* expected */ }
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that compute_stats returns DomainError::NoSamples for empty samples.
        /// **Validates: Requirements 11.1**
        #[test]
        fn compute_stats_empty_samples_returns_no_samples_error() {
            let samples: Vec<Sample> = vec![];
            let result = compute_stats(&samples, None);

            assert!(
                result.is_err(),
                "compute_stats should return error for empty samples"
            );
            match result {
                Err(DomainError::NoSamples) => { /* expected */ }
                Err(other) => panic!("expected NoSamples error, got: {:?}", other),
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that compute_stats returns DomainError::NoSamples when all samples are warmup.
        /// **Validates: Requirements 11.1**
        #[test]
        fn compute_stats_all_warmup_samples_returns_no_samples_error() {
            // Create samples where all are marked as warmup
            let samples = vec![
                Sample {
                    wall_ms: 100,
                    exit_code: 0,
                    warmup: true,
                    timed_out: false,
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(1024),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    stdout: None,
                    stderr: None,
                },
                Sample {
                    wall_ms: 200,
                    exit_code: 0,
                    warmup: true,
                    timed_out: false,
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(2048),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    stdout: None,
                    stderr: None,
                },
                Sample {
                    wall_ms: 150,
                    exit_code: 0,
                    warmup: true,
                    timed_out: false,
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(1536),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    stdout: None,
                    stderr: None,
                },
            ];

            let result = compute_stats(&samples, None);

            assert!(
                result.is_err(),
                "compute_stats should return error when all samples are warmup"
            );
            match result {
                Err(DomainError::NoSamples) => { /* expected */ }
                Err(other) => panic!("expected NoSamples error, got: {:?}", other),
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that compute_stats with work_units also returns NoSamples for all-warmup samples.
        /// **Validates: Requirements 11.1**
        #[test]
        fn compute_stats_all_warmup_with_work_units_returns_no_samples_error() {
            let samples = vec![Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: true,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }];

            // Even with work_units specified, should still fail
            let result = compute_stats(&samples, Some(1000));

            assert!(
                result.is_err(),
                "compute_stats should return error when all samples are warmup, even with work_units"
            );
            match result {
                Err(DomainError::NoSamples) => { /* expected */ }
                Err(other) => panic!("expected NoSamples error, got: {:?}", other),
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        // ---------------------------------------------------------------------
        // DomainError::InvalidBaseline Tests
        // ---------------------------------------------------------------------

        /// Test that compare_stats returns DomainError::InvalidBaseline when baseline value is 0.
        /// **Validates: Requirements 11.2**
        #[test]
        fn compare_stats_zero_baseline_returns_invalid_baseline_error() {
            // Create baseline stats with wall_ms median of 0
            let baseline = Stats {
                wall_ms: U64Summary::new(0, 0, 0),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            };

            let current = Stats {
                wall_ms: U64Summary::new(100, 100, 100),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            };

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));

            let result = compare_stats(&baseline, &current, &budgets);

            assert!(
                result.is_err(),
                "compare_stats should return error when baseline value is 0"
            );
            match result {
                Err(DomainError::InvalidBaseline(metric)) => {
                    assert_eq!(
                        metric,
                        Metric::WallMs,
                        "error should indicate WallMs metric"
                    );
                }
                Err(other) => panic!("expected InvalidBaseline error, got: {:?}", other),
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that compare_stats returns InvalidBaseline for zero throughput baseline.
        /// **Validates: Requirements 11.2**
        #[test]
        fn compare_stats_zero_throughput_baseline_returns_invalid_baseline_error() {
            let baseline = Stats {
                wall_ms: U64Summary::new(1000, 1000, 1000),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: Some(F64Summary::new(0.0, 0.0, 0.0)),
            };

            let current = Stats {
                wall_ms: U64Summary::new(1000, 1000, 1000),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: Some(F64Summary::new(100.0, 100.0, 100.0)),
            };

            let mut budgets = BTreeMap::new();
            budgets.insert(
                Metric::ThroughputPerS,
                Budget::new(0.20, 0.10, Direction::Higher),
            );

            let result = compare_stats(&baseline, &current, &budgets);

            assert!(
                result.is_err(),
                "compare_stats should return error when throughput baseline is 0"
            );
            match result {
                Err(DomainError::InvalidBaseline(metric)) => {
                    assert_eq!(
                        metric,
                        Metric::ThroughputPerS,
                        "error should indicate ThroughputPerS metric"
                    );
                }
                Err(other) => panic!("expected InvalidBaseline error, got: {:?}", other),
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that compare_stats returns InvalidBaseline for zero max_rss_kb baseline.
        /// **Validates: Requirements 11.2**
        #[test]
        fn compare_stats_zero_max_rss_baseline_returns_invalid_baseline_error() {
            let baseline = Stats {
                wall_ms: U64Summary::new(1000, 1000, 1000),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(U64Summary::new(0, 0, 0)),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            };

            let current = Stats {
                wall_ms: U64Summary::new(1000, 1000, 1000),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(U64Summary::new(1024, 1024, 1024)),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            };

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::MaxRssKb, Budget::new(0.20, 0.10, Direction::Lower));

            let result = compare_stats(&baseline, &current, &budgets);

            assert!(
                result.is_err(),
                "compare_stats should return error when max_rss_kb baseline is 0"
            );
            match result {
                Err(DomainError::InvalidBaseline(metric)) => {
                    assert_eq!(
                        metric,
                        Metric::MaxRssKb,
                        "error should indicate MaxRssKb metric"
                    );
                }
                Err(other) => panic!("expected InvalidBaseline error, got: {:?}", other),
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that compare_stats returns InvalidBaseline for negative throughput baseline.
        /// Note: While negative throughput is unusual, the check is for <= 0.
        /// **Validates: Requirements 11.2**
        #[test]
        fn compare_stats_negative_throughput_baseline_returns_invalid_baseline_error() {
            let baseline = Stats {
                wall_ms: U64Summary::new(1000, 1000, 1000),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: Some(F64Summary::new(-10.0, -10.0, -10.0)),
            };

            let current = Stats {
                wall_ms: U64Summary::new(1000, 1000, 1000),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: Some(F64Summary::new(100.0, 100.0, 100.0)),
            };

            let mut budgets = BTreeMap::new();
            budgets.insert(
                Metric::ThroughputPerS,
                Budget::new(0.20, 0.10, Direction::Higher),
            );

            let result = compare_stats(&baseline, &current, &budgets);

            assert!(
                result.is_err(),
                "compare_stats should return error when throughput baseline is negative"
            );
            match result {
                Err(DomainError::InvalidBaseline(metric)) => {
                    assert_eq!(
                        metric,
                        Metric::ThroughputPerS,
                        "error should indicate ThroughputPerS metric"
                    );
                }
                Err(other) => panic!("expected InvalidBaseline error, got: {:?}", other),
                Ok(_) => panic!("expected error, got Ok"),
            }
        }

        /// Test that DomainError::NoSamples has the expected error message.
        /// **Validates: Requirements 11.1**
        #[test]
        fn no_samples_error_has_descriptive_message() {
            let error = DomainError::NoSamples;
            let message = format!("{}", error);
            assert_eq!(message, "no samples to summarize");
        }

        /// Test that DomainError::InvalidBaseline has the expected error message.
        /// **Validates: Requirements 11.2**
        #[test]
        fn invalid_baseline_error_has_descriptive_message() {
            let error = DomainError::InvalidBaseline(Metric::WallMs);
            let message = format!("{}", error);
            assert_eq!(message, "baseline value for WallMs must be > 0");

            let error2 = DomainError::InvalidBaseline(Metric::ThroughputPerS);
            let message2 = format!("{}", error2);
            assert_eq!(message2, "baseline value for ThroughputPerS must be > 0");

            let error3 = DomainError::InvalidBaseline(Metric::MaxRssKb);
            let message3 = format!("{}", error3);
            assert_eq!(message3, "baseline value for MaxRssKb must be > 0");
        }

        /// Test that DomainError::InvalidAlpha has the expected error message.
        #[test]
        fn invalid_alpha_error_has_descriptive_message() {
            let error = DomainError::InvalidAlpha(1.5);
            let message = format!("{}", error);
            assert_eq!(
                message,
                "significance alpha must be between 0.0 and 1.0, got 1.5"
            );
        }

        /// Test that SignificancePolicy::new accepts valid alpha values.
        #[test]
        fn significance_policy_new_accepts_valid_alpha() {
            for alpha in [0.0, 0.05, 0.5, 1.0] {
                let policy = SignificancePolicy::new(alpha, 8, false);
                assert!(policy.is_ok(), "alpha={alpha} should be valid");
                let p = policy.unwrap();
                assert!((p.alpha - alpha).abs() < f64::EPSILON);
            }
        }

        /// Test that SignificancePolicy::new rejects out-of-range alpha values.
        #[test]
        fn significance_policy_new_rejects_invalid_alpha() {
            for alpha in [-0.1, 1.1, 2.0] {
                let result = SignificancePolicy::new(alpha, 8, false);
                match result {
                    Err(DomainError::InvalidAlpha(v)) => {
                        assert!((v - alpha).abs() < f64::EPSILON);
                    }
                    other => panic!("expected InvalidAlpha for alpha={alpha}, got: {other:?}"),
                }
            }
        }
    }

    // =========================================================================
    // derive_report Tests
    // =========================================================================

    mod derive_report_tests {
        use super::*;
        use perfgate_types::{
            BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareReceipt, CompareRef, Delta, Direction,
            Metric, MetricStatus, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
        };

        /// Helper to create a minimal CompareReceipt for testing.
        fn make_receipt(
            deltas: BTreeMap<Metric, Delta>,
            budgets: BTreeMap<Metric, Budget>,
            verdict_status: VerdictStatus,
            counts: VerdictCounts,
        ) -> CompareReceipt {
            CompareReceipt {
                schema: COMPARE_SCHEMA_V1.to_string(),
                tool: ToolInfo {
                    name: "perfgate".to_string(),
                    version: "0.1.0".to_string(),
                },
                bench: BenchMeta {
                    name: "test_bench".to_string(),
                    cwd: None,
                    command: vec!["echo".to_string(), "hello".to_string()],
                    repeat: 5,
                    warmup: 1,
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
                    status: verdict_status,
                    counts,
                    reasons: vec![],
                },
            }
        }

        /// Helper to create a Delta with given values.
        fn make_delta(baseline: f64, current: f64, status: MetricStatus) -> Delta {
            let ratio = current / baseline;
            let pct = (current - baseline) / baseline;
            let regression = pct.max(0.0);
            Delta {
                baseline,
                current,
                ratio,
                pct,
                regression,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status,
            }
        }

        /// Helper to create a Budget with given threshold.
        fn make_budget(threshold: f64) -> Budget {
            Budget::new(threshold, threshold * 0.9, Direction::Lower)
        }

        /// Test: Empty deltas produces no findings.
        #[test]
        fn test_empty_deltas_no_findings() {
            let receipt = make_receipt(
                BTreeMap::new(),
                BTreeMap::new(),
                VerdictStatus::Pass,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );

            let report = derive_report(&receipt);

            assert!(report.findings.is_empty());
            assert_eq!(report.verdict, VerdictStatus::Pass);
        }

        /// Test: All pass status deltas produce no findings.
        #[test]
        fn test_all_pass_no_findings() {
            let mut deltas = BTreeMap::new();
            deltas.insert(Metric::WallMs, make_delta(100.0, 105.0, MetricStatus::Pass));
            deltas.insert(
                Metric::MaxRssKb,
                make_delta(1000.0, 1050.0, MetricStatus::Pass),
            );

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, make_budget(0.2));
            budgets.insert(Metric::MaxRssKb, make_budget(0.2));

            let receipt = make_receipt(
                deltas,
                budgets,
                VerdictStatus::Pass,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );

            let report = derive_report(&receipt);

            assert!(report.findings.is_empty());
            assert_eq!(report.verdict, VerdictStatus::Pass);
        }

        /// Test: Mix of pass/warn/fail produces correct finding count and codes.
        #[test]
        fn test_mixed_status_correct_findings() {
            let mut deltas = BTreeMap::new();
            deltas.insert(Metric::WallMs, make_delta(100.0, 105.0, MetricStatus::Pass));
            deltas.insert(
                Metric::MaxRssKb,
                make_delta(1000.0, 1150.0, MetricStatus::Warn),
            );
            deltas.insert(
                Metric::ThroughputPerS,
                make_delta(500.0, 350.0, MetricStatus::Fail),
            );

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, make_budget(0.2));
            budgets.insert(Metric::MaxRssKb, make_budget(0.2));
            budgets.insert(Metric::ThroughputPerS, make_budget(0.2));

            let receipt = make_receipt(
                deltas,
                budgets,
                VerdictStatus::Fail,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );

            let report = derive_report(&receipt);

            // Should have 2 findings (1 warn + 1 fail, not the pass)
            assert_eq!(report.findings.len(), 2);

            // Verify finding codes
            let codes: Vec<&str> = report.findings.iter().map(|f| f.code.as_str()).collect();
            assert!(codes.contains(&"metric_warn"));
            assert!(codes.contains(&"metric_fail"));

            // Verify all findings have check_id = "perf.budget"
            for finding in &report.findings {
                assert_eq!(finding.check_id, "perf.budget");
            }

            // Verify verdict matches
            assert_eq!(report.verdict, VerdictStatus::Fail);
        }

        /// Test: Finding count equals warn + fail count.
        #[test]
        fn test_finding_count_equals_warn_plus_fail() {
            let mut deltas = BTreeMap::new();
            deltas.insert(Metric::WallMs, make_delta(100.0, 125.0, MetricStatus::Warn));
            deltas.insert(
                Metric::MaxRssKb,
                make_delta(1000.0, 1300.0, MetricStatus::Fail),
            );
            deltas.insert(
                Metric::ThroughputPerS,
                make_delta(500.0, 300.0, MetricStatus::Fail),
            );

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, make_budget(0.2));
            budgets.insert(Metric::MaxRssKb, make_budget(0.2));
            budgets.insert(Metric::ThroughputPerS, make_budget(0.2));

            let receipt = make_receipt(
                deltas,
                budgets,
                VerdictStatus::Fail,
                VerdictCounts {
                    pass: 0,
                    warn: 1,
                    fail: 2,
                    skip: 0,
                },
            );

            let report = derive_report(&receipt);

            // Invariant: finding count = warn + fail
            let expected_count = receipt.verdict.counts.warn + receipt.verdict.counts.fail;
            assert_eq!(report.findings.len(), expected_count as usize);
        }

        /// Test: Report verdict matches compare verdict.
        #[test]
        fn test_verdict_matches() {
            // Test with Warn verdict
            let mut deltas_warn = BTreeMap::new();
            deltas_warn.insert(Metric::WallMs, make_delta(100.0, 115.0, MetricStatus::Warn));

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, make_budget(0.2));

            let receipt_warn = make_receipt(
                deltas_warn,
                budgets.clone(),
                VerdictStatus::Warn,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );

            let report_warn = derive_report(&receipt_warn);
            assert_eq!(report_warn.verdict, VerdictStatus::Warn);

            // Test with Fail verdict
            let mut deltas_fail = BTreeMap::new();
            deltas_fail.insert(Metric::WallMs, make_delta(100.0, 130.0, MetricStatus::Fail));

            let receipt_fail = make_receipt(
                deltas_fail,
                budgets,
                VerdictStatus::Fail,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );

            let report_fail = derive_report(&receipt_fail);
            assert_eq!(report_fail.verdict, VerdictStatus::Fail);
        }

        /// Test: Findings are ordered deterministically by metric name.
        #[test]
        fn test_deterministic_ordering() {
            // Insert in reverse order to verify ordering is by metric name
            let mut deltas = BTreeMap::new();
            deltas.insert(
                Metric::ThroughputPerS,
                make_delta(500.0, 300.0, MetricStatus::Fail),
            );
            deltas.insert(Metric::WallMs, make_delta(100.0, 130.0, MetricStatus::Fail));
            deltas.insert(
                Metric::MaxRssKb,
                make_delta(1000.0, 1300.0, MetricStatus::Warn),
            );

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, make_budget(0.2));
            budgets.insert(Metric::MaxRssKb, make_budget(0.2));
            budgets.insert(Metric::ThroughputPerS, make_budget(0.2));

            let receipt = make_receipt(
                deltas,
                budgets,
                VerdictStatus::Fail,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );

            let report = derive_report(&receipt);

            // BTreeMap orders by Metric enum order (WallMs < MaxRssKb < ThroughputPerS based on derive order)
            // Verify the ordering is deterministic by checking metric names
            let metric_names: Vec<&str> = report
                .findings
                .iter()
                .map(|f| f.data.metric_name.as_str())
                .collect();

            // Run twice to ensure deterministic
            let report2 = derive_report(&receipt);
            let metric_names2: Vec<&str> = report2
                .findings
                .iter()
                .map(|f| f.data.metric_name.as_str())
                .collect();

            assert_eq!(metric_names, metric_names2);
        }

        /// Test: Finding data contains correct values.
        #[test]
        fn test_finding_data_values() {
            let mut deltas = BTreeMap::new();
            deltas.insert(Metric::WallMs, make_delta(100.0, 125.0, MetricStatus::Fail));

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, make_budget(0.2));

            let mut receipt = make_receipt(
                deltas,
                budgets,
                VerdictStatus::Fail,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );
            receipt.bench.name = "my_benchmark".to_string();

            let report = derive_report(&receipt);

            assert_eq!(report.findings.len(), 1);
            let finding = &report.findings[0];

            assert_eq!(finding.code, "metric_fail");
            assert_eq!(finding.check_id, "perf.budget");
            assert_eq!(finding.data.metric_name, "wall_ms");
            assert_eq!(finding.data.bench_name, "my_benchmark");
            assert!((finding.data.baseline - 100.0).abs() < f64::EPSILON);
            assert!((finding.data.current - 125.0).abs() < f64::EPSILON);
            assert!((finding.data.regression_pct - 0.25).abs() < f64::EPSILON);
            assert!((finding.data.threshold - 0.2).abs() < f64::EPSILON);
        }

        /// Test: Warn finding has correct code.
        #[test]
        fn test_warn_finding_code() {
            let mut deltas = BTreeMap::new();
            deltas.insert(Metric::WallMs, make_delta(100.0, 115.0, MetricStatus::Warn));

            let mut budgets = BTreeMap::new();
            budgets.insert(Metric::WallMs, make_budget(0.2));

            let receipt = make_receipt(
                deltas,
                budgets,
                VerdictStatus::Warn,
                VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
            );

            let report = derive_report(&receipt);

            assert_eq!(report.findings.len(), 1);
            assert_eq!(report.findings[0].code, "metric_warn");
        }

        /// Test: metric_to_string helper function.
        #[test]
        fn test_metric_to_string() {
            assert_eq!(metric_to_string(Metric::WallMs), "wall_ms");
            assert_eq!(metric_to_string(Metric::MaxRssKb), "max_rss_kb");
            assert_eq!(metric_to_string(Metric::ThroughputPerS), "throughput_per_s");
        }
    }
}
