//! Prometheus metrics middleware and endpoint.
//!
//! This module provides:
//! - A `/metrics` endpoint returning Prometheus text exposition format
//! - A middleware layer that records request duration and status code
//! - Custom counters for storage operations

use std::time::Instant;

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Metric name constants.
pub mod names {
    /// Histogram: request duration in seconds, labeled by method, path, status.
    pub const HTTP_REQUEST_DURATION_SECONDS: &str = "http_request_duration_seconds";
    /// Counter: total HTTP requests, labeled by method, path, status.
    pub const HTTP_REQUESTS_TOTAL: &str = "http_requests_total";
    /// Gauge: number of in-flight requests.
    pub const HTTP_REQUESTS_IN_FLIGHT: &str = "http_requests_in_flight";
    /// Counter: total baseline uploads.
    pub const BASELINE_UPLOADS_TOTAL: &str = "perfgate_baseline_uploads_total";
    /// Counter: total baseline downloads.
    pub const BASELINE_DOWNLOADS_TOTAL: &str = "perfgate_baseline_downloads_total";
    /// Counter: total storage operations, labeled by operation.
    pub const STORAGE_OPERATIONS_TOTAL: &str = "perfgate_storage_operations_total";
}

/// Installs the Prometheus recorder and returns a handle for rendering metrics.
///
/// Must be called once at server startup before any metrics are recorded.
pub fn setup_metrics_recorder() -> PrometheusHandle {
    PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install Prometheus recorder")
}

/// Axum handler that renders collected metrics in Prometheus text exposition format.
pub async fn metrics_handler(
    axum::extract::State(handle): axum::extract::State<PrometheusHandle>,
) -> impl IntoResponse {
    let body = handle.render();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(body))
        .unwrap()
}

/// Normalizes a URI path for use as a metrics label.
///
/// Replaces dynamic path segments with placeholders to avoid high-cardinality labels.
fn normalize_path(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    let mut normalized = Vec::with_capacity(segments.len());

    let mut i = 0;
    while i < segments.len() {
        let seg = segments[i];
        match seg {
            "projects" => {
                normalized.push("projects");
                // Next segment is the project name -> replace with placeholder
                if i + 1 < segments.len() {
                    normalized.push(":project");
                    i += 1;
                }
            }
            "baselines" => {
                normalized.push("baselines");
                // Next segment is benchmark name -> replace with placeholder
                if i + 1 < segments.len() && !segments[i + 1].is_empty() {
                    let next = segments[i + 1];
                    // Check if it's a known sub-path or a dynamic segment
                    if next != "latest" && next != "versions" && next != "promote" {
                        normalized.push(":benchmark");
                        i += 1;
                    }
                }
            }
            "versions" => {
                normalized.push("versions");
                // Next segment is the version -> replace with placeholder
                if i + 1 < segments.len() {
                    normalized.push(":version");
                    i += 1;
                }
            }
            "verdicts" => {
                normalized.push("verdicts");
            }
            _ => {
                normalized.push(seg);
            }
        }
        i += 1;
    }

    normalized.join("/")
}

/// Middleware that records HTTP request metrics.
///
/// Records:
/// - `http_request_duration_seconds` histogram
/// - `http_requests_total` counter
/// - `http_requests_in_flight` gauge
pub async fn metrics_middleware(request: Request, next: Next) -> Response {
    let method = request.method().to_string();
    let path = normalize_path(request.uri().path());

    gauge!(names::HTTP_REQUESTS_IN_FLIGHT).increment(1);
    let start = Instant::now();

    let response = next.run(request).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let labels = [
        ("method", method.clone()),
        ("path", path.clone()),
        ("status", status.clone()),
    ];

    histogram!(names::HTTP_REQUEST_DURATION_SECONDS, &labels).record(duration);
    counter!(names::HTTP_REQUESTS_TOTAL, &labels).increment(1);
    gauge!(names::HTTP_REQUESTS_IN_FLIGHT).decrement(1);

    response
}

/// Records a successful baseline upload.
pub fn record_baseline_upload(project: &str) {
    counter!(names::BASELINE_UPLOADS_TOTAL, "project" => project.to_string()).increment(1);
    counter!(names::STORAGE_OPERATIONS_TOTAL, "operation" => "upload").increment(1);
}

/// Records a successful baseline download (get/get_latest).
pub fn record_baseline_download(project: &str) {
    counter!(names::BASELINE_DOWNLOADS_TOTAL, "project" => project.to_string()).increment(1);
    counter!(names::STORAGE_OPERATIONS_TOTAL, "operation" => "download").increment(1);
}

/// Records a storage list operation.
pub fn record_storage_list() {
    counter!(names::STORAGE_OPERATIONS_TOTAL, "operation" => "list").increment(1);
}

/// Records a storage delete operation.
pub fn record_storage_delete() {
    counter!(names::STORAGE_OPERATIONS_TOTAL, "operation" => "delete").increment(1);
}

/// Records a storage promote operation.
pub fn record_storage_promote() {
    counter!(names::STORAGE_OPERATIONS_TOTAL, "operation" => "promote").increment(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_health() {
        assert_eq!(normalize_path("/health"), "/health");
    }

    #[test]
    fn test_normalize_path_baselines_upload() {
        assert_eq!(
            normalize_path("/api/v1/projects/my-proj/baselines"),
            "/api/v1/projects/:project/baselines"
        );
    }

    #[test]
    fn test_normalize_path_latest() {
        assert_eq!(
            normalize_path("/api/v1/projects/my-proj/baselines/my-bench/latest"),
            "/api/v1/projects/:project/baselines/:benchmark/latest"
        );
    }

    #[test]
    fn test_normalize_path_version() {
        assert_eq!(
            normalize_path("/api/v1/projects/my-proj/baselines/my-bench/versions/v1"),
            "/api/v1/projects/:project/baselines/:benchmark/versions/:version"
        );
    }

    #[test]
    fn test_normalize_path_promote() {
        assert_eq!(
            normalize_path("/api/v1/projects/my-proj/baselines/my-bench/promote"),
            "/api/v1/projects/:project/baselines/:benchmark/promote"
        );
    }

    #[test]
    fn test_normalize_path_verdicts() {
        assert_eq!(
            normalize_path("/api/v1/projects/my-proj/verdicts"),
            "/api/v1/projects/:project/verdicts"
        );
    }

    #[test]
    fn test_normalize_path_metrics() {
        assert_eq!(normalize_path("/metrics"), "/metrics");
    }

    #[test]
    fn test_normalize_path_root() {
        assert_eq!(normalize_path("/"), "/");
    }
}
