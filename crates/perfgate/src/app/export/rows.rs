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
