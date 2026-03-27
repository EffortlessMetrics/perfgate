//! Baseline CRUD handlers.

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::auth::{AuthContext, Scope, check_scope};
use crate::error::StoreError;
use crate::metrics;
use crate::models::{
    ApiError, AuditAction, AuditEvent, AuditResourceType, BaselineRecord, BaselineRecordExt,
    BaselineSource, DeleteBaselineResponse, ListBaselinesQuery, PromoteBaselineRequest,
    PromoteBaselineResponse, UploadBaselineRequest, UploadBaselineResponse, generate_ulid,
};
use crate::server::AppState;

/// Upload a new baseline.
pub async fn upload_baseline(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Json(request): Json<UploadBaselineRequest>,
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

    let version = request
        .version
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string());

    let record = <BaselineRecord as BaselineRecordExt>::new(
        project.clone(),
        request.benchmark.clone(),
        version.clone(),
        request.receipt.clone(),
        request.git_ref.clone(),
        request.git_sha.clone(),
        request.metadata.clone(),
        request.tags.clone(),
        BaselineSource::Upload,
    );

    match state.store.create(&record).await {
        Ok(_) => {
            metrics::record_baseline_upload(&project);
            info!(project = %project, benchmark = %request.benchmark, version = %version, "Baseline uploaded");

            emit_audit(
                &state.audit,
                &auth_ctx,
                AuditAction::Create,
                AuditResourceType::Baseline,
                &record.id,
                &project,
                serde_json::json!({
                    "benchmark": request.benchmark,
                    "version": version,
                }),
            )
            .await;

            let response = UploadBaselineResponse {
                id: record.id.clone(),
                benchmark: request.benchmark.clone(),
                version,
                created_at: record.created_at,
                etag: record.etag(),
            };
            Ok((
                StatusCode::CREATED,
                [(header::ETAG, record.etag())],
                Json(response),
            ))
        }
        Err(StoreError::AlreadyExists(_)) => Err((
            StatusCode::CONFLICT,
            Json(ApiError::already_exists(&format!(
                "{}/{}",
                request.benchmark, version
            ))),
        )),
        Err(e) => {
            error!(error = %e, "Failed to upload baseline");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
    }
}

pub async fn get_latest_baseline(
    Path((project, benchmark)): Path<(String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, Some(&benchmark), Scope::Read)?;

    match state.store.get_latest(&project, &benchmark).await {
        Ok(Some(record)) => {
            metrics::record_baseline_download(&project);
            Ok((
                StatusCode::OK,
                [(header::ETAG, record.etag())],
                Json(record),
            ))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(&format!(
                "Baseline {}/latest not found",
                benchmark
            ))),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )),
    }
}

pub async fn get_baseline(
    Path((project, benchmark, version)): Path<(String, String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, Some(&benchmark), Scope::Read)?;

    match state.store.get(&project, &benchmark, &version).await {
        Ok(Some(record)) => {
            metrics::record_baseline_download(&project);
            Ok((
                StatusCode::OK,
                [(header::ETAG, record.etag())],
                Json(record),
            ))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(&format!(
                "Baseline {}/{} not found",
                benchmark, version
            ))),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )),
    }
}

pub async fn list_baselines(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Query(query): Query<ListBaselinesQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, None, Scope::Read)?;

    match state.store.list(&project, &query).await {
        Ok(response) => {
            metrics::record_storage_list();
            Ok(Json(response).into_response())
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )),
    }
}

pub async fn delete_baseline(
    Path((project, benchmark, version)): Path<(String, String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, Some(&benchmark), Scope::Delete)?;

    match state.store.delete(&project, &benchmark, &version).await {
        Ok(true) => {
            metrics::record_storage_delete();
            let resource_id = format!("{}/{}/{}", project, benchmark, version);

            emit_audit(
                &state.audit,
                &auth_ctx,
                AuditAction::Delete,
                AuditResourceType::Baseline,
                &resource_id,
                &project,
                serde_json::json!({
                    "benchmark": benchmark,
                    "version": version,
                }),
            )
            .await;

            Ok(Json(DeleteBaselineResponse {
                deleted: true,
                id: resource_id,
                benchmark,
                version,
                deleted_at: chrono::Utc::now(),
            })
            .into_response())
        }
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError::not_found(&format!(
                "Baseline {}/{} not found",
                benchmark, version
            ))),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )),
    }
}

pub async fn promote_baseline(
    Path((project, benchmark)): Path<(String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(state): State<AppState>,
    Json(request): Json<PromoteBaselineRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), &project, Some(&benchmark), Scope::Promote)?;

    let source = match state
        .store
        .get(&project, &benchmark, &request.from_version)
        .await
    {
        Ok(Some(record)) => record,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(&format!(
                    "Source {}/{} not found",
                    benchmark, request.from_version
                ))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ));
        }
    };

    if state
        .store
        .get(&project, &benchmark, &request.to_version)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            )
        })?
        .is_some()
    {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError::already_exists(&format!(
                "Target {}/{} already exists",
                benchmark, request.to_version
            ))),
        ));
    }

    let promoted = <BaselineRecord as BaselineRecordExt>::new(
        project.clone(),
        benchmark.clone(),
        request.to_version.clone(),
        source.receipt,
        request.git_ref.or(source.git_ref),
        request.git_sha.or(source.git_sha),
        source.metadata,
        request.tags,
        BaselineSource::Promote,
    );

    match state.store.create(&promoted).await {
        Ok(_) => {
            metrics::record_storage_promote();

            emit_audit(
                &state.audit,
                &auth_ctx,
                AuditAction::Promote,
                AuditResourceType::Baseline,
                &promoted.id,
                &project,
                serde_json::json!({
                    "benchmark": benchmark,
                    "from_version": request.from_version,
                    "to_version": request.to_version,
                }),
            )
            .await;

            Ok((
                StatusCode::CREATED,
                Json(PromoteBaselineResponse {
                    id: promoted.id.clone(),
                    benchmark: benchmark.clone(),
                    version: request.to_version.clone(),
                    promoted_from: request.from_version.clone(),
                    promoted_at: promoted.created_at,
                    created_at: promoted.created_at,
                }),
            ))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )),
    }
}

/// Emit an audit event, logging a warning if storage fails.
async fn emit_audit(
    audit: &Arc<dyn crate::storage::AuditStore>,
    auth_ctx: &AuthContext,
    action: AuditAction,
    resource_type: AuditResourceType,
    resource_id: &str,
    project: &str,
    metadata: serde_json::Value,
) {
    let event = AuditEvent {
        id: generate_ulid(),
        timestamp: chrono::Utc::now(),
        actor: auth_ctx.api_key.id.clone(),
        action,
        resource_type,
        resource_id: resource_id.to_string(),
        project: project.to_string(),
        metadata,
    };

    if let Err(e) = audit.log_event(&event).await {
        warn!(error = %e, "Failed to log audit event");
    }
}
