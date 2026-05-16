use super::DomainError;
use super::stats::{percentile, summarize_f64, summarize_u64};
use perfgate_types::{Metric, MetricStatistic, RunReceipt, Stats};

/// Compute perfgate stats from samples.
///
/// Warmup samples (`sample.warmup == true`) are excluded.
///
/// # Examples
///
/// ```
/// use perfgate::domain::compute_stats;
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
#[must_use = "pure computation; call site should use the returned Stats"]
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

pub(crate) fn metric_cv(stats: &Stats, metric: Metric) -> Option<f64> {
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
pub(crate) fn metric_to_string(metric: Metric) -> String {
    metric.as_str().to_string()
}

#[must_use = "pure computation; call site should use the returned value"]
pub fn metric_value(stats: &Stats, metric: Metric) -> Option<f64> {
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

pub(crate) fn metric_value_from_run(
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

pub(crate) fn metric_series_from_run(run: &RunReceipt, metric: Metric) -> Vec<f64> {
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
