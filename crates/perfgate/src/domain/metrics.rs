use perfgate_types::{Metric, MetricStatistic, RunReceipt, Stats};

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
