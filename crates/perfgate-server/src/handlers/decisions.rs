//! Performance decision ledger handlers.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use tracing::{error, info, warn};

use crate::auth::{AuthContext, Scope, check_scope};
use crate::metrics;
use crate::models::{
    ApiError, AuditAction, AuditEvent, AuditResourceType, DECISION_RECORD_SCHEMA_V1,
    DecisionRecord, ListDecisionsQuery, PruneDecisionsRequest, UploadDecisionRequest,
    generate_ulid,
};
use crate::server::AppState;
use perfgate_types::{DECISION_INDEX_SCHEMA_V1, SCENARIO_SCHEMA_V1, TRADEOFF_SCHEMA_V1};

/// Upload a structured performance decision receipt to the server ledger.
pub async fn upload_decision(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Json(request): Json<UploadDecisionRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, None, Scope::Write)?;
    validate_decision_upload(&request)?;

    let scenario_name = request.tradeoff.scenario.clone().or_else(|| {
        request
            .scenario
            .as_ref()
            .map(|receipt| receipt.scenario.name.clone())
    });
    let accepted_rules: Vec<String> = request
        .tradeoff
        .rules
        .iter()
        .filter(|rule| rule.accepted)
        .map(|rule| rule.name.clone())
        .collect();

    let record = DecisionRecord {
        schema: DECISION_RECORD_SCHEMA_V1.to_string(),
        id: generate_ulid(),
        project: project.clone(),
        scenario: scenario_name,
        status: request.tradeoff.decision.status,
        verdict: request.tradeoff.verdict.status,
        accepted_rules,
        review_required: request.tradeoff.decision.review_required,
        review_reasons: request.tradeoff.decision.review_reasons.clone(),
        git_ref: request.git_ref,
        git_sha: request.git_sha,
        scenario_receipt: request.scenario,
        tradeoff_receipt: request.tradeoff,
        artifact_index: request.artifact_index,
        created_at: chrono::Utc::now(),
    };

    match state.store.create_decision(&record).await {
        Ok(_) => {
            info!(
                project = %project,
                status = %record.status.as_str(),
                verdict = %record.verdict.as_str(),
                decision_id = %record.id,
                "Performance decision uploaded"
            );

            let audit_event = AuditEvent {
                id: generate_ulid(),
                timestamp: chrono::Utc::now(),
                actor: auth_ctx.api_key.id.clone(),
                action: AuditAction::Create,
                resource_type: AuditResourceType::Decision,
                resource_id: record.id.clone(),
                project: project.clone(),
                metadata: serde_json::json!({
                    "scenario": record.scenario.clone(),
                    "status": record.status.as_str(),
                    "verdict": record.verdict.as_str(),
                    "review_required": record.review_required,
                    "accepted_rules": record.accepted_rules.clone(),
                }),
            };
            if let Err(e) = state.audit.log_event(&audit_event).await {
                metrics::record_storage_error("audit_log");
                warn!(error = %e, "Failed to log decision audit event");
            }

            Ok((StatusCode::CREATED, Json(record)))
        }
        Err(e) => {
            metrics::record_storage_error("upload_decision");
            error!(error = %e, "Failed to upload performance decision");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

/// List stored performance decisions for a project.
pub async fn list_decisions(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Query(query): Query<ListDecisionsQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, None, Scope::Read)?;

    match state.store.list_decisions(&project, &query).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => {
            metrics::record_storage_error("list_decisions");
            error!(error = %e, "Failed to list performance decisions");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

/// Return the latest stored performance decision for a project.
pub async fn latest_decision(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, None, Scope::Read)?;

    match state.store.latest_decision(&project).await {
        Ok(Some(record)) => Ok(Json(record)),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(&format!(
                "Decision record not found for project {}",
                project
            ))),
        )),
        Err(e) => {
            metrics::record_storage_error("latest_decision");
            error!(error = %e, "Failed to get latest performance decision");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

/// Prune old performance decisions from the server ledger.
pub async fn prune_decisions(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Json(request): Json<PruneDecisionsRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, None, Scope::Delete)?;

    match state
        .store
        .prune_decisions(&project, request.older_than, request.dry_run)
        .await
    {
        Ok(response) => {
            if !request.dry_run {
                let audit_event = AuditEvent {
                    id: generate_ulid(),
                    timestamp: chrono::Utc::now(),
                    actor: auth_ctx.api_key.id.clone(),
                    action: AuditAction::Delete,
                    resource_type: AuditResourceType::Decision,
                    resource_id: project.clone(),
                    project: project.clone(),
                    metadata: serde_json::json!({
                        "older_than": request.older_than.to_rfc3339(),
                        "matched": response.matched,
                        "deleted": response.deleted,
                        "decision_ids": response.decision_ids.clone(),
                    }),
                };
                if let Err(e) = state.audit.log_event(&audit_event).await {
                    metrics::record_storage_error("audit_log");
                    warn!(error = %e, "Failed to log decision prune audit event");
                }
            }

            info!(
                project = %project,
                older_than = %request.older_than.to_rfc3339(),
                dry_run = request.dry_run,
                matched = response.matched,
                deleted = response.deleted,
                "Performance decisions pruned"
            );

            Ok(Json(response))
        }
        Err(e) => {
            metrics::record_storage_error("prune_decisions");
            error!(error = %e, "Failed to prune performance decisions");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

fn validate_decision_upload(
    request: &UploadDecisionRequest,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    validate_schema("tradeoff", &request.tradeoff.schema, TRADEOFF_SCHEMA_V1)?;
    if let Some(scenario) = &request.scenario {
        validate_schema("scenario", &scenario.schema, SCENARIO_SCHEMA_V1)?;
    }
    if let Some(index) = &request.artifact_index {
        validate_schema("artifact_index", &index.schema, DECISION_INDEX_SCHEMA_V1)?;
    }
    Ok(())
}

fn validate_schema(
    label: &str,
    actual: &str,
    expected: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if actual == expected {
        return Ok(());
    }
    Err((
        StatusCode::BAD_REQUEST,
        Json(ApiError::validation(&format!(
            "{} schema must be {}; got {}",
            label, expected, actual
        ))),
    ))
}
