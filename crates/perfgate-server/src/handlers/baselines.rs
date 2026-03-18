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
use crate::models::{
    ApiError, BaselineRecord, BaselineRecordExt, BaselineSource, DeleteBaselineResponse, ListBaselinesQuery,
    PromoteBaselineRequest, PromoteBaselineResponse, UploadBaselineRequest, UploadBaselineResponse,
};
use crate::storage::BaselineStore;

/// Upload a new baseline.
pub async fn upload_baseline(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
    Json(request): Json<UploadBaselineRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Write)?;

    if let Err(e) = perfgate_validation::validate_bench_name(&request.benchmark) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation(&format!("Invalid benchmark name: {}", e))),
        ));
    }

    let version = request.version.clone().unwrap_or_else(|| chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string());

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

    match store.create(&record).await {
        Ok(_) => {
            info!(project = %project, benchmark = %request.benchmark, version = %version, "Baseline uploaded");
            let response = UploadBaselineResponse {
                id: record.id.clone(),
                benchmark: request.benchmark.clone(),
                version,
                created_at: record.created_at,
                etag: record.etag(),
            };
            Ok((StatusCode::CREATED, [(header::ETAG, record.etag())], Json(response)))
        }
        Err(StoreError::AlreadyExists(_)) => {
            Err((StatusCode::CONFLICT, Json(ApiError::already_exists(&format!("{}/{}", request.benchmark, version)))))
        }
        Err(e) => {
            error!(error = %e, "Failed to upload baseline");
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::internal_error(&e.to_string()))))
        }
    }
}

pub async fn get_latest_baseline(
    Path((project, benchmark)): Path<(String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Read)?;

    match store.get_latest(&project, &benchmark).await {
        Ok(Some(record)) => Ok((StatusCode::OK, [(header::ETAG, record.etag())], Json(record))),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(ApiError::not_found(&format!("Baseline {}/latest not found", benchmark))))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::internal_error(&e.to_string())))),
    }
}

pub async fn get_baseline(
    Path((project, benchmark, version)): Path<(String, String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Read)?;

    match store.get(&project, &benchmark, &version).await {
        Ok(Some(record)) => Ok((StatusCode::OK, [(header::ETAG, record.etag())], Json(record))),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(ApiError::not_found(&format!("Baseline {}/{} not found", benchmark, version))))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::internal_error(&e.to_string())))),
    }
}

pub async fn list_baselines(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
    Query(query): Query<ListBaselinesQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Read)?;

    match store.list(&project, &query).await {
        Ok(response) => Ok(Json(response).into_response()),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::internal_error(&e.to_string())))),
    }
}

pub async fn delete_baseline(
    Path((project, benchmark, version)): Path<(String, String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Delete)?;

    match store.delete(&project, &benchmark, &version).await {
        Ok(true) => Ok(Json(DeleteBaselineResponse {
            deleted: true,
            id: format!("{}/{}/{}", project, benchmark, version),
            benchmark,
            version,
            deleted_at: chrono::Utc::now(),
        })
        .into_response()),
        Ok(false) => Err((StatusCode::NOT_FOUND, Json(ApiError::not_found(&format!("Baseline {}/{} not found", benchmark, version))))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::internal_error(&e.to_string())))),
    }
}

pub async fn promote_baseline(
    Path((project, benchmark)): Path<(String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
    Json(request): Json<PromoteBaselineRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Promote)?;

    let source = match store.get(&project, &benchmark, &request.from_version).await {
        Ok(Some(record)) => record,
        Ok(None) => return Err((StatusCode::NOT_FOUND, Json(ApiError::not_found(&format!("Source {}/{} not found", benchmark, request.from_version))))),
        Err(e) => return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::internal_error(&e.to_string())))),
    };

    if store.get(&project, &benchmark, &request.to_version).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )
    })?.is_some() {
        return Err((StatusCode::CONFLICT, Json(ApiError::already_exists(&format!("Target {}/{} already exists", benchmark, request.to_version)))));
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

    match store.create(&promoted).await {
        Ok(_) => Ok((StatusCode::CREATED, Json(PromoteBaselineResponse {
            id: promoted.id.clone(),
            benchmark: benchmark.clone(),
            version: request.to_version.clone(),
            promoted_from: request.from_version.clone(),
            promoted_at: promoted.created_at,
            created_at: promoted.created_at,
        }))),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError::internal_error(&e.to_string())))),
    }
}
