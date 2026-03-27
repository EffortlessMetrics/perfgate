//! Trend analysis handlers.
//!
//! Provides an endpoint to analyze metric trends for a benchmark
//! and predict budget threshold breaches.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::error;

use crate::auth::{AuthContext, Scope, check_scope};
use crate::models::ApiError;
use crate::server::AppState;
use perfgate_domain::{TrendConfig, metric_value};
use perfgate_stats::trend::{TrendAnalysis, analyze_trend, spark_chart};
use perfgate_types::Metric;

/// Query parameters for trend analysis.
#[derive(Debug, Clone, Deserialize)]
pub struct TrendQuery {
    /// Metric to analyze (e.g., wall_ms, cpu_ms, max_rss_kb).
    pub metric: String,
    /// Number of recent baselines to use for trend analysis.
    #[serde(default = "default_window")]
    pub window: u32,
    /// Budget threshold as a fraction (e.g., 0.20 for 20% regression allowed).
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    /// Number of runs within which a breach is considered "critical".
    #[serde(default = "default_critical_window")]
    pub critical_window: u32,
}

fn default_window() -> u32 {
    30
}

fn default_threshold() -> f64 {
    0.20
}

fn default_critical_window() -> u32 {
    10
}

/// Response for trend analysis.
#[derive(Debug, Clone, Serialize)]
pub struct TrendResponse {
    pub project: String,
    pub benchmark: String,
    pub analysis: Option<TrendAnalysis>,
    pub values: Vec<f64>,
    pub spark: String,
    pub data_points: usize,
}

/// Analyze metric trends for a benchmark.
///
/// `GET /api/v1/projects/{project}/baselines/{benchmark}/trend?metric=wall_ms&window=30`
pub async fn get_trend(
    Path((project, benchmark)): Path<(String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Query(query): Query<TrendQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, Some(&benchmark), Scope::Read)?;
    let store = &state.store;

    let metric = Metric::parse_key(&query.metric).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation(&format!(
                "Unknown metric: {}",
                query.metric
            ))),
        )
    })?;

    // Fetch recent baselines for this benchmark
    let list_query = crate::models::ListBaselinesQuery {
        benchmark: Some(benchmark.clone()),
        include_receipt: true,
        limit: query.window,
        ..Default::default()
    };

    let baselines = match store.list(&project, &list_query).await {
        Ok(response) => response.baselines,
        Err(e) => {
            error!(error = %e, "Failed to list baselines for trend analysis");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ));
        }
    };

    // Extract metric values from baseline receipts (sorted chronologically)
    let mut entries: Vec<(chrono::DateTime<chrono::Utc>, f64)> = baselines
        .iter()
        .filter_map(|b| {
            let receipt = b.receipt.as_ref()?;
            let value = metric_value(&receipt.stats, metric)?;
            Some((b.created_at, value))
        })
        .collect();

    // Sort by creation time (oldest first)
    entries.sort_by_key(|(ts, _)| *ts);

    let values: Vec<f64> = entries.iter().map(|(_, v)| *v).collect();
    let data_points = values.len();
    let spark = spark_chart(&values);

    let direction = metric.default_direction();
    let lower_is_better = direction == perfgate_types::Direction::Lower;

    let analysis = if values.len() >= 2 {
        let baseline_value = values[0];
        let absolute_threshold = if lower_is_better {
            baseline_value * (1.0 + query.threshold)
        } else {
            baseline_value * (1.0 - query.threshold)
        };

        let config = TrendConfig {
            critical_window: query.critical_window,
            ..TrendConfig::default()
        };

        analyze_trend(
            &values,
            metric.as_str(),
            absolute_threshold,
            lower_is_better,
            &config,
        )
    } else {
        None
    };

    Ok(Json(TrendResponse {
        project,
        benchmark,
        analysis,
        values,
        spark,
        data_points,
    }))
}
