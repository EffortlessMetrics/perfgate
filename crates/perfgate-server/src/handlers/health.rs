//! Health check handlers.

use axum::{Json, extract::State};
use tracing::warn;

use crate::error::StoreError;
use crate::metrics;
use crate::models::{HealthResponse, StorageHealth as ModelStorageHealth};
use crate::server::AppState;
use crate::storage::StorageHealth;

/// Health check endpoint.
///
/// Returns server health status, storage backend health, and connection pool
/// metrics (when using a pooled backend such as PostgreSQL).
pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let mut storage_detail = None;
    let storage_health = match state.store.health_check().await {
        Ok(health) => health,
        Err(error) => {
            warn!(
                backend = state.store.backend_type(),
                error = %error,
                "Storage health check failed"
            );
            metrics::record_storage_error("health_check");
            storage_detail = Some(storage_health_detail(&error).to_string());
            StorageHealth::Unhealthy
        }
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
            detail: storage_detail,
        },
        pool,
    })
}

fn storage_health_detail(error: &StoreError) -> &'static str {
    match error {
        StoreError::ConnectionError(_) => "connection_error",
        StoreError::QueryError(_) => "query_error",
        StoreError::IoError(_) => "io_error",
        StoreError::SqliteError(_) => "sqlite_error",
        StoreError::LockError(_) => "lock_error",
        StoreError::SerializationError(_) => "serialization_error",
        StoreError::AlreadyExists(_) | StoreError::NotFound(_) | StoreError::Other(_) => {
            "storage_error"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        AuditEvent, BaselineRecord, BaselineVersion, DecisionRecord, ListAuditEventsQuery,
        ListAuditEventsResponse, ListBaselinesQuery, ListBaselinesResponse, ListDecisionsQuery,
        ListDecisionsResponse, ListVerdictsQuery, ListVerdictsResponse, PaginationInfo,
        VerdictRecord,
    };
    use crate::storage::{AuditStore, BaselineStore};
    use async_trait::async_trait;
    use std::sync::Arc;

    #[derive(Debug)]
    struct FailingStore;

    fn storage_failure() -> StoreError {
        StoreError::QueryError(
            "could not reach database at postgres://user:secret@example.invalid/perfgate"
                .to_string(),
        )
    }

    #[async_trait]
    impl BaselineStore for FailingStore {
        async fn create(&self, _record: &BaselineRecord) -> Result<(), StoreError> {
            Err(storage_failure())
        }

        async fn get(
            &self,
            _project: &str,
            _benchmark: &str,
            _version: &str,
        ) -> Result<Option<BaselineRecord>, StoreError> {
            Err(storage_failure())
        }

        async fn get_latest(
            &self,
            _project: &str,
            _benchmark: &str,
        ) -> Result<Option<BaselineRecord>, StoreError> {
            Err(storage_failure())
        }

        async fn list(
            &self,
            _project: &str,
            _query: &ListBaselinesQuery,
        ) -> Result<ListBaselinesResponse, StoreError> {
            Err(storage_failure())
        }

        async fn update(&self, _record: &BaselineRecord) -> Result<(), StoreError> {
            Err(storage_failure())
        }

        async fn delete(
            &self,
            _project: &str,
            _benchmark: &str,
            _version: &str,
        ) -> Result<bool, StoreError> {
            Err(storage_failure())
        }

        async fn hard_delete(
            &self,
            _project: &str,
            _benchmark: &str,
            _version: &str,
        ) -> Result<bool, StoreError> {
            Err(storage_failure())
        }

        async fn list_versions(
            &self,
            _project: &str,
            _benchmark: &str,
        ) -> Result<Vec<BaselineVersion>, StoreError> {
            Err(storage_failure())
        }

        async fn health_check(&self) -> Result<StorageHealth, StoreError> {
            Err(storage_failure())
        }

        fn backend_type(&self) -> &'static str {
            "failing"
        }

        async fn create_verdict(&self, _record: &VerdictRecord) -> Result<(), StoreError> {
            Err(storage_failure())
        }

        async fn list_verdicts(
            &self,
            _project: &str,
            _query: &ListVerdictsQuery,
        ) -> Result<ListVerdictsResponse, StoreError> {
            Err(storage_failure())
        }

        async fn create_decision(&self, _record: &DecisionRecord) -> Result<(), StoreError> {
            Err(storage_failure())
        }

        async fn latest_decision(
            &self,
            _project: &str,
        ) -> Result<Option<DecisionRecord>, StoreError> {
            Err(storage_failure())
        }

        async fn list_decisions(
            &self,
            _project: &str,
            _query: &ListDecisionsQuery,
        ) -> Result<ListDecisionsResponse, StoreError> {
            Err(storage_failure())
        }

        async fn prune_decisions(
            &self,
            _project: &str,
            _older_than: chrono::DateTime<chrono::Utc>,
            _dry_run: bool,
        ) -> Result<perfgate_types::baseline_service::PruneDecisionsResponse, StoreError> {
            Err(storage_failure())
        }
    }

    #[async_trait]
    impl AuditStore for FailingStore {
        async fn log_event(&self, _event: &AuditEvent) -> Result<(), StoreError> {
            Err(storage_failure())
        }

        async fn list_events(
            &self,
            _query: &ListAuditEventsQuery,
        ) -> Result<ListAuditEventsResponse, StoreError> {
            Ok(ListAuditEventsResponse {
                events: Vec::new(),
                pagination: PaginationInfo {
                    total: 0,
                    offset: 0,
                    limit: 100,
                    has_more: false,
                },
            })
        }
    }

    fn failing_state() -> AppState {
        let store = Arc::new(FailingStore);
        AppState {
            store: store.clone(),
            audit: store,
        }
    }

    #[tokio::test]
    async fn health_check_includes_sanitized_storage_detail_on_error() {
        let Json(response) = health_check(State(failing_state())).await;

        assert_eq!(response.status, "degraded");
        assert_eq!(response.storage.backend, "failing");
        assert_eq!(response.storage.status, "unhealthy");
        assert_eq!(response.storage.detail.as_deref(), Some("query_error"));
        let serialized = serde_json::to_string(&response).expect("serialize health response");
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("example.invalid"));
    }

    #[test]
    fn storage_health_detail_uses_stable_non_secret_codes() {
        assert_eq!(
            storage_health_detail(&StoreError::ConnectionError(
                "postgres://user:secret@example.invalid/perfgate".to_string()
            )),
            "connection_error"
        );
        assert_eq!(
            storage_health_detail(&StoreError::LockError("poisoned".to_string())),
            "lock_error"
        );
        assert_eq!(
            storage_health_detail(&StoreError::Other("anything".to_string())),
            "storage_error"
        );
    }
}
