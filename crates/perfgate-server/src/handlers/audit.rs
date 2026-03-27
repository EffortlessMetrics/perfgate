//! Audit log query handler.

use axum::{
    Extension, Json,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use tracing::error;

use crate::auth::{AuthContext, Scope, check_scope};
use crate::models::{ApiError, ListAuditEventsQuery};
use crate::server::AppState;

/// List audit events with filtering.
///
/// Requires `Admin` scope. Supports filtering by project, action, resource_type,
/// actor, since, and until.
pub async fn list_audit_events(
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Query(query): Query<ListAuditEventsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Audit log access requires admin role. We use the actor's own project for the
    // scope check, since audit events span projects.
    check_scope(
        Some(&auth_ctx),
        &auth_ctx.api_key.project_id,
        None,
        Scope::Admin,
    )?;

    match state.audit.list_events(&query).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!(error = %e, "Failed to list audit events");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}
