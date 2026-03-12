//! Baseline CRUD handlers.

//!
//! This module implements the REST API endpoints for baseline management.
//!
//! # Endpoints
//!
//! - `POST /projects/{project}/baselines` - Upload baseline
//! - `GET /projects/{project}/baselines/{benchmark}/latest` - Get latest
//! - `GET /projects/{project}/baselines/{benchmark}/versions/{version}` - Get by version
//! - `GET /projects/{project}/baselines` - List with filtering
//! - `DELETE /projects/{project}/baselines/{benchmark}/versions/{version}` - Delete
//! - `POST /projects/{project}/baselines/{benchmark}/promote` - Promote

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
    ApiError, BaselineRecord, BaselineSource, DeleteBaselineResponse, ListBaselinesQuery,
    PromoteBaselineRequest, PromoteBaselineResponse, UploadBaselineRequest, UploadBaselineResponse,
};
use crate::storage::BaselineStore;

/// Upload a new baseline.
///
/// POST /projects/{project}/baselines
pub async fn upload_baseline(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
    Json(request): Json<UploadBaselineRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Write)?;

    // Validate benchmark name
    if let Err(e) = perfgate_validation::validate_bench_name(&request.benchmark) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation(&format!(
                "Invalid benchmark name: {}",
                e
            ))),
        ));
    }

    // Generate version if not provided
    let version = request
        .version
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string());

    // Create the record
    let record = BaselineRecord::new(
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

    // Store it
    match store.create(&record).await {
        Ok(_) => {
            info!(
                project = %project,
                benchmark = %request.benchmark,
                version = %version,
                "Baseline uploaded successfully"
            );

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
        Err(StoreError::AlreadyExists(_)) => {
            warn!(
                project = %project,
                benchmark = %request.benchmark,
                version = %version,
                "Baseline already exists"
            );
            Err((
                StatusCode::CONFLICT,
                Json(ApiError::already_exists(
                    "Baseline",
                    &format!("{}/{}", request.benchmark, version),
                )),
            ))
        }
        Err(e) => {
            error!(
                project = %project,
                benchmark = %request.benchmark,
                version = %version,
                error = %e,
                "Failed to upload baseline"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ))
        }
    }
}

/// Get the latest baseline for a benchmark.
///
/// GET /projects/{project}/baselines/{benchmark}/latest
pub async fn get_latest_baseline(
    Path((project, benchmark)): Path<(String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Read)?;

    match store.get_latest(&project, &benchmark).await {
        Ok(Some(record)) => Ok((
            StatusCode::OK,
            [(header::ETAG, record.etag())],
            Json(record),
        )),
        Ok(None) => {
            warn!(
                project = %project,
                benchmark = %benchmark,
                "Baseline not found"
            );
            Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(
                    "Baseline",
                    &format!("{}/latest", benchmark),
                )),
            ))
        }
        Err(e) => {
            error!(
                project = %project,
                benchmark = %benchmark,
                error = %e,
                "Failed to get baseline"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ))
        }
    }
}

/// Get a specific baseline version.
///
/// GET /projects/{project}/baselines/{benchmark}/versions/{version}
pub async fn get_baseline(
    Path((project, benchmark, version)): Path<(String, String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Read)?;

    match store.get(&project, &benchmark, &version).await {
        Ok(Some(record)) => Ok((
            StatusCode::OK,
            [(header::ETAG, record.etag())],
            Json(record),
        )),
        Ok(None) => {
            warn!(
                project = %project,
                benchmark = %benchmark,
                version = %version,
                "Baseline not found"
            );
            Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(
                    "Baseline",
                    &format!("{}/{}", benchmark, version),
                )),
            ))
        }
        Err(e) => {
            error!(
                project = %project,
                benchmark = %benchmark,
                version = %version,
                error = %e,
                "Failed to get baseline"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ))
        }
    }
}

/// List baselines with optional filtering.
///
/// GET /projects/{project}/baselines
pub async fn list_baselines(
    Path(project): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
    Query(query): Query<ListBaselinesQuery>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Read)?;

    match store.list(&project, &query).await {
        Ok(response) => Ok(Json(response).into_response()),
        Err(e) => {
            error!(
                project = %project,
                error = %e,
                "Failed to list baselines"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ))
        }
    }
}

/// Delete a baseline (soft delete).
///
/// DELETE /projects/{project}/baselines/{benchmark}/versions/{version}
pub async fn delete_baseline(
    Path((project, benchmark, version)): Path<(String, String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Delete)?;

    match store.delete(&project, &benchmark, &version).await {
        Ok(true) => {
            info!(
                project = %project,
                benchmark = %benchmark,
                version = %version,
                "Baseline deleted"
            );
            Ok(Json(DeleteBaselineResponse {
                deleted: true,
                id: format!("{}/{}/{}", project, benchmark, version),
            })
            .into_response())
        }
        Ok(false) => {
            warn!(
                project = %project,
                benchmark = %benchmark,
                version = %version,
                "Baseline not found or already deleted"
            );
            Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(
                    "Baseline",
                    &format!("{}/{}", benchmark, version),
                )),
            ))
        }
        Err(e) => {
            error!(
                project = %project,
                benchmark = %benchmark,
                version = %version,
                error = %e,
                "Failed to delete baseline"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ))
        }
    }
}

/// Promote a baseline version.
///
/// POST /projects/{project}/baselines/{benchmark}/promote
pub async fn promote_baseline(
    Path((project, benchmark)): Path<(String, String)>,
    Extension(auth_ctx): Extension<AuthContext>,
    State(store): State<Arc<dyn BaselineStore>>,
    Json(request): Json<PromoteBaselineRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_scope(Some(&auth_ctx), Scope::Promote)?;

    // Get the source version
    let source = match store.get(&project, &benchmark, &request.from_version).await {
        Ok(Some(record)) => record,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(
                    "Source baseline",
                    &format!("{}/{}", benchmark, request.from_version),
                )),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ));
        }
    };

    // Check if target version already exists
    match store.get(&project, &benchmark, &request.to_version).await {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiError::already_exists(
                    "Target version",
                    &format!("{}/{}", benchmark, request.to_version),
                )),
            ));
        }
        Ok(None) => {}
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ));
        }
    }

    // Create the promoted version
    let promoted = BaselineRecord::new(
        project.clone(),
        benchmark.clone(),
        request.to_version.clone(),
        source.receipt.clone(),
        request.git_ref.or(source.git_ref),
        request.git_sha.or(source.git_sha),
        source.metadata.clone(),
        source.tags.clone(),
        BaselineSource::Promote,
    );

    match store.create(&promoted).await {
        Ok(_) => {
            info!(
                project = %project,
                benchmark = %benchmark,
                from_version = %request.from_version,
                to_version = %request.to_version,
                "Baseline promoted"
            );

            Ok((
                StatusCode::CREATED,
                Json(PromoteBaselineResponse {
                    id: promoted.id.clone(),
                    benchmark: benchmark.clone(),
                    version: request.to_version.clone(),
                    promoted_from: request.from_version.clone(),
                    created_at: promoted.created_at,
                }),
            ))
        }
        Err(e) => {
            error!(
                project = %project,
                benchmark = %benchmark,
                from_version = %request.from_version,
                to_version = %request.to_version,
                error = %e,
                "Failed to promote baseline"
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal(&e.to_string())),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ApiKey, Role};
    use crate::storage::InMemoryStore;
    use axum::body::Body;
    use axum::http::{Method, Request};
    use perfgate_types::{BenchMeta, HostInfo, RunMeta, RunReceipt, Stats, ToolInfo, U64Summary};
    use std::collections::BTreeMap;
    use tower::ServiceExt;

    fn test_auth_context(role: Role) -> AuthContext {
        AuthContext {
            api_key: ApiKey::new(
                "test-auth".to_string(),
                "Test Auth".to_string(),
                "project".to_string(),
                role,
            ),
            source_ip: None,
        }
    }

    fn create_test_receipt(name: &str) -> RunReceipt {
        RunReceipt {
            schema: "perfgate.run.v1".to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.3.0".to_string(),
            },
            run: RunMeta {
                id: "test-run-id".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: "2026-01-01T00:01:00Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(8),
                    memory_bytes: Some(16 * 1024 * 1024 * 1024),
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: name.to_string(),
                cwd: None,
                command: vec!["./bench.sh".to_string()],
                repeat: 5,
                warmup: 1,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![],
            stats: Stats {
                wall_ms: U64Summary {
                    median: 100,
                    min: 90,
                    max: 110,
                },
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    fn create_test_request() -> UploadBaselineRequest {
        UploadBaselineRequest {
            benchmark: "my-bench".to_string(),
            version: Some("v1.0.0".to_string()),
            git_ref: Some("refs/heads/main".to_string()),
            git_sha: Some("abc123".to_string()),
            receipt: create_test_receipt("my-bench"),
            metadata: BTreeMap::new(),
            tags: vec!["test".to_string()],
            normalize: false,
        }
    }

    #[tokio::test]
    async fn test_upload_baseline() {
        let store = Arc::new(InMemoryStore::new()) as Arc<dyn BaselineStore>;

        let app = axum::Router::new()
            .route(
                "/projects/{project}/baselines",
                axum::routing::post(upload_baseline),
            )
            .layer(axum::Extension(test_auth_context(Role::Contributor)))
            .with_state(store);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/projects/my-project/baselines")
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::to_string(&create_test_request()).unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_get_latest_baseline() {
        let store = Arc::new(InMemoryStore::new()) as Arc<dyn BaselineStore>;

        // First upload a baseline
        let request = create_test_request();
        let app = axum::Router::new()
            .route(
                "/projects/{project}/baselines",
                axum::routing::post(upload_baseline),
            )
            .layer(axum::Extension(test_auth_context(Role::Contributor)))
            .with_state(store.clone());

        app.oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/projects/my-project/baselines")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

        // Now get latest
        let app = axum::Router::new()
            .route(
                "/projects/{project}/baselines/{benchmark}/latest",
                axum::routing::get(get_latest_baseline),
            )
            .layer(axum::Extension(test_auth_context(Role::Viewer)))
            .with_state(store);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/projects/my-project/baselines/my-bench/latest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_baseline() {
        let store = Arc::new(InMemoryStore::new()) as Arc<dyn BaselineStore>;

        // First upload a baseline
        let request = create_test_request();
        let app = axum::Router::new()
            .route(
                "/projects/{project}/baselines",
                axum::routing::post(upload_baseline),
            )
            .layer(axum::Extension(test_auth_context(Role::Contributor)))
            .with_state(store.clone());

        app.oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/projects/my-project/baselines")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

        // Now delete
        let app = axum::Router::new()
            .route(
                "/projects/{project}/baselines/{benchmark}/versions/{version}",
                axum::routing::delete(delete_baseline),
            )
            .layer(axum::Extension(test_auth_context(Role::Admin)))
            .with_state(store);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/projects/my-project/baselines/my-bench/versions/v1.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_delete_baseline_requires_delete_scope() {
        let store = Arc::new(InMemoryStore::new()) as Arc<dyn BaselineStore>;

        let request = create_test_request();
        let upload_app = axum::Router::new()
            .route(
                "/projects/{project}/baselines",
                axum::routing::post(upload_baseline),
            )
            .layer(axum::Extension(test_auth_context(Role::Contributor)))
            .with_state(store.clone());

        upload_app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/projects/my-project/baselines")
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::to_string(&request).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let delete_app = axum::Router::new()
            .route(
                "/projects/{project}/baselines/{benchmark}/versions/{version}",
                axum::routing::delete(delete_baseline),
            )
            .layer(axum::Extension(test_auth_context(Role::Viewer)))
            .with_state(store);

        let response = delete_app
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri("/projects/my-project/baselines/my-bench/versions/v1.0.0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
