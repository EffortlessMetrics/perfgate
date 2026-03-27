//! Admin handlers for server management operations.

use axum::{Extension, Json, extract::Query, http::StatusCode, response::IntoResponse};
use serde::Deserialize;
use std::sync::Arc;
use tracing::{error, info};

use crate::auth::{AuthContext, Scope, check_scope};
use crate::cleanup::run_cleanup;
use crate::models::ApiError;
use crate::storage::ArtifactStore;

/// Query parameters for the admin cleanup endpoint.
#[derive(Debug, Deserialize)]
pub struct CleanupQuery {
    /// Remove objects older than this many days.
    pub older_than_days: Option<u64>,
}

/// `DELETE /api/v1/admin/cleanup?older_than_days=N`
///
/// Triggers an immediate cleanup of expired artifacts. Requires admin scope.
/// If `older_than_days` is not specified, defaults to the server's
/// configured retention period. Returns 400 if no retention period is
/// available.
pub async fn admin_cleanup(
    Extension(auth_ctx): Extension<AuthContext>,
    Extension(artifact_store): Extension<Option<Arc<dyn ArtifactStore>>>,
    Extension(default_retention): Extension<DefaultRetentionDays>,
    Query(query): Query<CleanupQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Admin-only: use a synthetic "admin" project check.
    // The `Scope::Admin` check ensures only admin-role keys can call this.
    check_scope(
        Some(&auth_ctx),
        &auth_ctx.api_key.project_id,
        None,
        Scope::Admin,
    )?;

    let store = match artifact_store {
        Some(s) => s,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(
                    "No artifact store configured; cleanup requires S3/GCS/Azure object storage",
                )),
            ));
        }
    };

    let days = query.older_than_days.or(Some(default_retention.0));
    let days = match days {
        Some(d) if d > 0 => d,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError::bad_request(
                    "older_than_days must be > 0, or --retention-days must be configured",
                )),
            ));
        }
    };

    info!(
        older_than_days = days,
        triggered_by = %auth_ctx.api_key.id,
        "Admin cleanup triggered"
    );

    match run_cleanup(store.as_ref(), days).await {
        Ok(result) => {
            info!(
                deleted = result.deleted,
                scanned = result.scanned,
                "Admin cleanup completed"
            );
            Ok((StatusCode::OK, Json(result)))
        }
        Err(e) => {
            error!(error = %e, "Admin cleanup failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e)),
            ))
        }
    }
}

/// Newtype wrapper for the server's default retention days, used as an
/// Axum extension so the handler can fall back to it.
#[derive(Debug, Clone, Copy)]
pub struct DefaultRetentionDays(pub u64);
