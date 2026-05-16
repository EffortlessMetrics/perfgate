use perfgate_types::{CompareReceipt, Metric, MetricStatus, RunReceipt};

/// Row structure for RunReceipt export.
///
/// # Examples
///
/// ```
/// use perfgate::app::export::RunExportRow;
///
/// let row = RunExportRow {
///     bench_name: "my-bench".into(),
///     wall_ms_median: 42,
///     wall_ms_min: 40,
///     wall_ms_max: 44,
///     binary_bytes_median: None,
///     cpu_ms_median: Some(20),
///     ctx_switches_median: None,
///     max_rss_kb_median: None,
///     energy_uj_median: None,
///     page_faults_median: None,
///     io_read_bytes_median: None,
///     io_write_bytes_median: None,
///     network_packets_median: None,
///     throughput_median: None,
///     sample_count: 5,
///     timestamp: "2024-01-01T00:00:00Z".into(),
/// };
/// assert_eq!(row.bench_name, "my-bench");
/// assert_eq!(row.sample_count, 5);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunExportRow {
    pub bench_name: String,
    pub wall_ms_median: u64,
    pub wall_ms_min: u64,
    pub wall_ms_max: u64,
    pub binary_bytes_median: Option<u64>,
    pub cpu_ms_median: Option<u64>,
    pub ctx_switches_median: Option<u64>,
    pub energy_uj_median: Option<u64>,
    pub max_rss_kb_median: Option<u64>,
    pub page_faults_median: Option<u64>,
    pub io_read_bytes_median: Option<u64>,
    pub io_write_bytes_median: Option<u64>,
    pub network_packets_median: Option<u64>,
    pub throughput_median: Option<f64>,
    pub sample_count: usize,
    pub timestamp: String,
}

/// Row structure for CompareReceipt export.
///
/// # Examples
///
/// ```
/// use perfgate::app::export::CompareExportRow;
///
/// let row = CompareExportRow {
///     bench_name: "my-bench".to_string(),
///     metric: "wall_ms".to_string(),
///     baseline_value: 100.0,
///     current_value: 110.0,
///     regression_pct: 10.0,
///     status: "pass".to_string(),
///     threshold: 20.0,
///     warn_threshold: Some(18.0),
///     cv: None,
///     noise_threshold: None,
/// };
/// assert_eq!(row.metric, "wall_ms");
/// assert_eq!(row.status, "pass");
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CompareExportRow {
    pub bench_name: String,
    pub metric: String,
    pub baseline_value: f64,
    pub current_value: f64,
    pub regression_pct: f64,
    pub status: String,
    pub threshold: f64,
    pub warn_threshold: Option<f64>,
    pub cv: Option<f64>,
    pub noise_threshold: Option<f64>,
}

/// Convert RunReceipt to an exportable row.
pub(super) fn run_to_row(receipt: &RunReceipt) -> RunExportRow {
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
pub(super) fn compare_to_rows(receipt: &CompareReceipt) -> Vec<CompareExportRow> {
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
                regression_pct: delta.regression * 100.0,
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

/// Convert Metric enum to snake_case string.
fn metric_to_string(metric: Metric) -> String {
    metric.as_str().to_string()
}

/// Convert MetricStatus enum to lowercase string.
fn status_to_string(status: MetricStatus) -> String {
    match status {
        MetricStatus::Pass => "pass".to_string(),
        MetricStatus::Warn => "warn".to_string(),
        MetricStatus::Fail => "fail".to_string(),
        MetricStatus::Skip => "skip".to_string(),
    }
}
