use perfgate_types::{Metric, MetricStatus};

/// Convert Metric enum to snake_case string.
pub(crate) fn metric_to_string(metric: Metric) -> String {
    metric.as_str().to_string()
}

/// Convert MetricStatus enum to lowercase string.
pub(crate) fn status_to_string(status: MetricStatus) -> String {
    match status {
        MetricStatus::Pass => "pass".to_string(),
        MetricStatus::Warn => "warn".to_string(),
        MetricStatus::Fail => "fail".to_string(),
        MetricStatus::Skip => "skip".to_string(),
    }
}
