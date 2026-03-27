//! Health check handlers.

use axum::{Json, extract::State};

use crate::models::{HealthResponse, StorageHealth as ModelStorageHealth};
use crate::server::AppState;
use crate::storage::StorageHealth;

/// Health check endpoint.
///
/// Returns server health status, storage backend health, and connection pool
/// metrics (when using a pooled backend such as PostgreSQL).
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let storage_health = match state.store.health_check().await {
        Ok(health) => health,
        Err(_) => StorageHealth::Unhealthy,
    };

    let status_str = storage_health.as_str();

    // Collect pool metrics if the backend exposes them.
    let pool = state.store.pool_metrics();

    Json(HealthResponse {
        status: if storage_health == StorageHealth::Healthy {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
        version: env!("CARGO_PKG_VERSION").to_string(),
        storage: ModelStorageHealth {
            backend: state.store.backend_type().to_string(),
            status: status_str.to_string(),
        },
        pool,
    })
}
