//! Verdict history handlers.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use tracing::{error, info, warn};

use crate::auth::{AuthContext, Scope, check_scope};
use crate::models::{
    ApiError, AuditAction, AuditEvent, AuditResourceType, ListVerdictsQuery, SubmitVerdictRequest,
    VERDICT_SCHEMA_V1, VerdictRecord, generate_ulid,
};
use crate::server::AppState;

/// Submit a new benchmark verdict.
pub async fn submit_verdict(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Json(request): Json<SubmitVerdictRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(
        Some(&auth_ctx),
        &project,
        Some(&request.benchmark),
        Scope::Write,
    )?;

    if let Err(e) = perfgate_validation::validate_bench_name(&request.benchmark) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation(&format!(
                "Invalid benchmark name: {}",
                e
            ))),
        ));
    }

    let record = VerdictRecord {
        schema: VERDICT_SCHEMA_V1.to_string(),
        id: generate_ulid(),
        project: project.clone(),
        benchmark: request.benchmark.clone(),
        run_id: request.run_id.clone(),
        status: request.status,
        counts: request.counts,
        reasons: request.reasons,
        git_ref: request.git_ref,
        git_sha: request.git_sha,
        created_at: chrono::Utc::now(),
    };

    match state.store.create_verdict(&record).await {
        Ok(_) => {
            info!(
                project = %project,
                benchmark = %record.benchmark,
                status = ?record.status,
                "Verdict submitted"
            );

            let audit_event = AuditEvent {
                id: generate_ulid(),
                timestamp: chrono::Utc::now(),
                actor: auth_ctx.api_key.id.clone(),
                action: AuditAction::Create,
                resource_type: AuditResourceType::Verdict,
                resource_id: record.id.clone(),
                project: project.clone(),
                metadata: serde_json::json!({
                    "benchmark": record.benchmark,
                    "status": format!("{:?}", record.status).to_lowercase(),
                }),
            };
            if let Err(e) = state.audit.log_event(&audit_event).await {
                warn!(error = %e, "Failed to log audit event");
            }

            Ok((StatusCode::CREATED, Json(record)))
        }
        Err(e) => {
            error!(error = %e, "Failed to submit verdict");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

/// List verdicts for a project.
pub async fn list_verdicts(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Query(query): Query<ListVerdictsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, None, Scope::Read)?;

    match state.store.list_verdicts(&project, &query).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            error!(error = %e, "Failed to list verdicts");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}
