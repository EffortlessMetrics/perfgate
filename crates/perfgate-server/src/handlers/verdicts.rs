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

const HIGH_FLAKINESS_CV_THRESHOLD: f64 = 0.30;
const FLAKINESS_HISTORY_LIMIT: u32 = 20;

fn score_flakiness_history(cv_history: &[f64]) -> Option<f64> {
    let filtered: Vec<f64> = cv_history
        .iter()
        .copied()
        .filter(|cv| cv.is_finite() && *cv >= 0.0)
        .collect();
    if filtered.is_empty() {
        return None;
    }

    let noisy_ratio = filtered
        .iter()
        .filter(|cv| **cv > HIGH_FLAKINESS_CV_THRESHOLD)
        .count() as f64
        / filtered.len() as f64;
    let mean_severity = filtered
        .iter()
        .map(|cv| (cv / HIGH_FLAKINESS_CV_THRESHOLD).min(2.0) / 2.0)
        .sum::<f64>()
        / filtered.len() as f64;

    Some((noisy_ratio * 0.7 + mean_severity * 0.3).min(1.0))
}

async fn compute_flakiness_score(
    state: &AppState,
    project: &str,
    benchmark: &str,
    current_wall_ms_cv: Option<f64>,
) -> Option<f64> {
    let mut cv_history = Vec::new();
    if let Some(cv) = current_wall_ms_cv.filter(|cv| cv.is_finite() && *cv >= 0.0) {
        cv_history.push(cv);
    }

    let query = ListVerdictsQuery::new()
        .with_benchmark(benchmark.to_string())
        .with_limit(FLAKINESS_HISTORY_LIMIT.saturating_sub(1));
    match state.store.list_verdicts(project, &query).await {
        Ok(response) => {
            cv_history.extend(
                response
                    .verdicts
                    .into_iter()
                    .filter_map(|record| record.wall_ms_cv),
            );
        }
        Err(e) => {
            warn!(
                error = %e,
                project = %project,
                benchmark = %benchmark,
                "Failed to load verdict history for flakiness scoring"
            );
        }
    }

    score_flakiness_history(&cv_history)
}

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

    let flakiness_score =
        compute_flakiness_score(&state, &project, &request.benchmark, request.wall_ms_cv).await;
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
        wall_ms_cv: request.wall_ms_cv,
        flakiness_score,
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
