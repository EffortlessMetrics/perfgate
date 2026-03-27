//! Fleet-wide dependency regression detection handlers.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use tracing::{error, info};

use crate::models::{
    ApiError, DependencyImpactQuery, ListFleetAlertsQuery, RecordDependencyEventRequest,
};
use crate::storage::FleetStore;

/// Record dependency change events with their performance impact.
pub async fn record_dependency_event(
    State(store): State<Arc<dyn FleetStore>>,
    Json(request): Json<RecordDependencyEventRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    if request.dependency_changes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation("dependency_changes must not be empty")),
        ));
    }

    match store.record_dependency_events(&request).await {
        Ok(response) => {
            info!(
                project = %request.project,
                benchmark = %request.benchmark,
                deps = request.dependency_changes.len(),
                delta_pct = request.delta_pct,
                "Dependency events recorded"
            );
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(e) => {
            error!(error = %e, "Failed to record dependency events");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

/// List fleet-wide alerts for correlated dependency regressions.
pub async fn list_fleet_alerts(
    State(store): State<Arc<dyn FleetStore>>,
    Query(query): Query<ListFleetAlertsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    match store.list_fleet_alerts(&query).await {
        Ok(response) => Ok(Json(response).into_response()),
        Err(e) => {
            error!(error = %e, "Failed to list fleet alerts");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

/// Get the impact of a specific dependency across projects.
pub async fn dependency_impact(
    Path(dep_name): Path<String>,
    State(store): State<Arc<dyn FleetStore>>,
    Query(query): Query<DependencyImpactQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    match store.dependency_impact(&dep_name, &query).await {
        Ok(response) => Ok(Json(response).into_response()),
        Err(e) => {
            error!(error = %e, dep = %dep_name, "Failed to get dependency impact");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}
