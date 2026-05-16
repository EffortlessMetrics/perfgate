use perfgate_types::{CompareReceipt, RunReceipt};

use super::mapping::{metric_to_string, status_to_string};
use super::{CompareExportRow, RunExportRow};

pub(crate) fn run_to_row(receipt: &RunReceipt) -> RunExportRow {
    let sample_count = receipt.samples.iter().filter(|s| !s.warmup).count();

    RunExportRow {
        bench_name: receipt.bench.name.clone(),
        wall_ms_median: receipt.stats.wall_ms.median,
        wall_ms_min: receipt.stats.wall_ms.min,
        wall_ms_max: receipt.stats.wall_ms.max,
        binary_bytes_median: receipt.stats.binary_bytes.as_ref().map(|s| s.median),
        cpu_ms_median: receipt.stats.cpu_ms.as_ref().map(|s| s.median),
        ctx_switches_median: receipt.stats.ctx_switches.as_ref().map(|s| s.median),
        energy_uj_median: receipt.stats.energy_uj.as_ref().map(|s| s.median),
        max_rss_kb_median: receipt.stats.max_rss_kb.as_ref().map(|s| s.median),
        page_faults_median: receipt.stats.page_faults.as_ref().map(|s| s.median),
        io_read_bytes_median: receipt.stats.io_read_bytes.as_ref().map(|s| s.median),
        io_write_bytes_median: receipt.stats.io_write_bytes.as_ref().map(|s| s.median),
        network_packets_median: receipt.stats.network_packets.as_ref().map(|s| s.median),
        throughput_median: receipt.stats.throughput_per_s.as_ref().map(|s| s.median),
        sample_count,
        timestamp: receipt.run.started_at.clone(),
    }
}

/// Convert CompareReceipt to exportable rows (one per metric, sorted by metric name).
pub(crate) fn compare_to_rows(receipt: &CompareReceipt) -> Vec<CompareExportRow> {
    let mut rows: Vec<CompareExportRow> = receipt
        .deltas
        .iter()
        .map(|(metric, delta)| {
            let budget = receipt.budgets.get(metric);
            let threshold = budget.map(|b| b.threshold).unwrap_or(0.0);
            let warn_threshold = budget.map(|b| b.warn_threshold);

            CompareExportRow {
                bench_name: receipt.bench.name.clone(),
                metric: metric_to_string(*metric),
                baseline_value: delta.baseline,
                current_value: delta.current,
                regression_pct: delta.pct * 100.0,
                status: status_to_string(delta.status),
                threshold: threshold * 100.0,
                warn_threshold: warn_threshold.map(|t| t * 100.0),
                cv: delta.cv.map(|cv| cv * 100.0),
                noise_threshold: delta.noise_threshold.map(|t| t * 100.0),
            }
        })
        .collect();

    rows.sort_by(|a, b| a.metric.cmp(&b.metric));
    rows
}
