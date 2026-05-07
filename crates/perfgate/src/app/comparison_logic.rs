//! Logic for building budgets and metric statistics for comparisons.

use perfgate_types::{Budget, Metric, MetricStatistic, RunReceipt};
use std::collections::BTreeMap;

/// Build budgets for comparing two receipts.
#[allow(clippy::too_many_arguments)]
pub fn build_budgets(
    baseline: &RunReceipt,
    current: &RunReceipt,
    global_threshold: f64,
    global_warn_factor: f64,
    global_noise_threshold: Option<f64>,
    global_noise_policy: Option<perfgate_types::NoisePolicy>,
    metric_thresholds: Vec<(String, f64)>,
    noise_thresholds: Vec<(String, f64)>,
    direction_overrides: Vec<(String, String)>,
) -> anyhow::Result<BTreeMap<Metric, Budget>> {
    // Determine candidate metrics: those present in both baseline+current.
    let mut candidates = Vec::new();
    candidates.push(Metric::WallMs);
    if baseline.stats.binary_bytes.is_some() && current.stats.binary_bytes.is_some() {
        candidates.push(Metric::BinaryBytes);
    }
    if baseline.stats.cpu_ms.is_some() && current.stats.cpu_ms.is_some() {
        candidates.push(Metric::CpuMs);
    }
    if baseline.stats.ctx_switches.is_some() && current.stats.ctx_switches.is_some() {
        candidates.push(Metric::CtxSwitches);
    }
    if baseline.stats.max_rss_kb.is_some() && current.stats.max_rss_kb.is_some() {
        candidates.push(Metric::MaxRssKb);
    }
    if baseline.stats.page_faults.is_some() && current.stats.page_faults.is_some() {
        candidates.push(Metric::PageFaults);
    }
    if baseline.stats.throughput_per_s.is_some() && current.stats.throughput_per_s.is_some() {
        candidates.push(Metric::ThroughputPerS);
    }

    let mut thresholds: BTreeMap<String, f64> = metric_thresholds.into_iter().collect();
    let mut noise_limits: BTreeMap<String, f64> = noise_thresholds.into_iter().collect();
    let mut dirs: BTreeMap<String, String> = direction_overrides.into_iter().collect();

    let mut budgets = BTreeMap::new();

    for metric in candidates {
        let key = metric.as_str();
        let threshold = thresholds.remove(key).unwrap_or(global_threshold);
        let warn_threshold = threshold * global_warn_factor;
        let noise_threshold = noise_limits.remove(key).or(global_noise_threshold);
        let noise_policy = global_noise_policy.unwrap_or(perfgate_types::NoisePolicy::Warn);

        let dir = match dirs.remove(key).as_deref() {
            Some("lower") => perfgate_types::Direction::Lower,
            Some("higher") => perfgate_types::Direction::Higher,
            Some(other) => {
                anyhow::bail!("invalid direction for {key}: {other} (expected lower|higher)")
            }
            None => metric.default_direction(),
        };

        budgets.insert(
            metric,
            Budget {
                threshold,
                warn_threshold,
                noise_threshold,
                noise_policy,
                direction: dir,
            },
        );
    }

    Ok(budgets)
}

/// Build metric statistics overrides for a comparison.
pub fn build_metric_statistics(
    budgets: &BTreeMap<Metric, Budget>,
    overrides: Vec<(String, String)>,
) -> anyhow::Result<BTreeMap<Metric, MetricStatistic>> {
    let mut statistics = BTreeMap::new();

    for (key, value) in overrides {
        let metric = Metric::parse_key(&key)
            .ok_or_else(|| anyhow::anyhow!("unknown metric for --metric-stat: {}", key))?;
        if !budgets.contains_key(&metric) {
            anyhow::bail!(
                "metric-stat override for {} is not applicable (metric not present in both receipts)",
                key
            );
        }

        let statistic = match value.to_lowercase().as_str() {
            "median" => MetricStatistic::Median,
            "p95" => MetricStatistic::P95,
            _ => {
                anyhow::bail!(
                    "invalid statistic for {}: {} (expected median|p95)",
                    key,
                    value
                )
            }
        };

        statistics.insert(metric, statistic);
    }

    Ok(statistics)
}

/// Map verdict counts to a string verdict.
pub fn verdict_from_counts(pass_count: u32, warn_count: u32, fail_count: u32) -> &'static str {
    if fail_count > 0 {
        "fail"
    } else if warn_count > 0 {
        "warn"
    } else if pass_count > 0 {
        "pass"
    } else {
        "skip"
    }
}
