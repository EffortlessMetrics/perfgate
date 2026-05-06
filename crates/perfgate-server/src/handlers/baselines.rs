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

    if let Err(e) = perfgate_types::validation::validate_bench_name(&request.benchmark) {
        metrics::record_upload_failure(&project, "validation");
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
        Err(StoreError::AlreadyExists(_)) => {
            metrics::record_upload_failure(&project, "conflict");
            Err((
                StatusCode::CONFLICT,
                Json(ApiError::already_exists(&format!(
                    "{}/{}",
                    request.benchmark, version
                ))),
            ))
        }
        Err(e) => {
            metrics::record_upload_failure(&project, "storage");
            metrics::record_storage_error("upload_baseline");
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
        Err(e) => {
            metrics::record_storage_error("get_latest_baseline");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
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
        Err(e) => {
            metrics::record_storage_error("get_baseline");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
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
            metrics::record_baselines_total(&project, response.pagination.total);
            Ok(Json(response).into_response())
        }
        Err(e) => {
            metrics::record_storage_error("list_baselines");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
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
        Err(e) => {
            metrics::record_storage_error("delete_baseline");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
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
            metrics::record_storage_error("promote_source_lookup");
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
            metrics::record_storage_error("promote_target_lookup");
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
        Err(e) => {
            metrics::record_storage_error("promote_baseline");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error(&e.to_string())),
            ))
        }
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
        metrics::record_storage_error("audit_log");
        warn!(error = %e, "Failed to log audit event");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{ApiKey, Role};
    use crate::models::{
        BaselineVersion, ListAuditEventsQuery, ListAuditEventsResponse, ListBaselinesResponse,
        ListVerdictsQuery, ListVerdictsResponse, PaginationInfo, VerdictRecord,
    };
    use crate::storage::{AuditStore, BaselineStore, StorageHealth};
    use async_trait::async_trait;
    use perfgate_types::{
        BenchMeta, HostInfo, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
    };

    #[derive(Debug)]
    struct FailingStore;

    fn storage_failure() -> StoreError {
        StoreError::Other("injected storage failure".to_string())
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

    fn app_state() -> AppState {
        let store = Arc::new(FailingStore);
        AppState {
            store: store.clone(),
            audit: store,
        }
    }

    fn auth_ctx() -> AuthContext {
        AuthContext {
            api_key: ApiKey::new(
                "admin-key".to_string(),
                "Admin".to_string(),
                "project".to_string(),
                Role::Admin,
            ),
            source_ip: None,
        }
    }

    fn upload_request() -> UploadBaselineRequest {
        UploadBaselineRequest {
            benchmark: "bench".to_string(),
            version: Some("v1".to_string()),
            git_ref: Some("main".to_string()),
            git_sha: Some("abc123".to_string()),
            receipt: RunReceipt {
                schema: "perfgate.run.v1".to_string(),
                tool: ToolInfo {
                    name: "perfgate".to_string(),
                    version: "test".to_string(),
                },
                run: RunMeta {
                    id: "run-1".to_string(),
                    started_at: "2024-01-01T00:00:00Z".to_string(),
                    ended_at: "2024-01-01T00:00:01Z".to_string(),
                    host: HostInfo {
                        os: "linux".to_string(),
                        arch: "x86_64".to_string(),
                        hostname_hash: None,
                        cpu_count: None,
                        memory_bytes: None,
                    },
                },
                bench: BenchMeta {
                    name: "bench".to_string(),
                    command: vec!["true".to_string()],
                    repeat: 1,
                    warmup: 0,
                    timeout_ms: None,
                    cwd: None,
                    work_units: None,
                },
                samples: vec![Sample {
                    wall_ms: 1,
                    exit_code: 0,
                    warmup: false,
                    timed_out: false,
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    binary_bytes: None,
                    stdout: None,
                    stderr: None,
                }],
                stats: Stats {
                    wall_ms: U64Summary::new(1, 1, 1),
                    max_rss_kb: None,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    cpu_ms: None,
                    page_faults: None,
                    ctx_switches: None,
                    binary_bytes: None,
                    throughput_per_s: None,
                },
            },
            metadata: std::collections::BTreeMap::new(),
            tags: Vec::new(),
            normalize: false,
        }
    }

    fn assert_internal_error<T>(result: Result<T, (StatusCode, Json<ApiError>)>) {
        match result {
            Ok(_) => panic!("handler should return an internal server error"),
            Err((status, _)) => assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    #[tokio::test]
    async fn storage_failures_return_500_from_baseline_handlers() {
        let state = app_state();
        let auth = auth_ctx();

        assert_internal_error(
            upload_baseline(
                Path("project".to_string()),
                Extension(auth.clone()),
                State(state.clone()),
                Json(upload_request()),
            )
            .await,
        );

        assert_internal_error(
            get_latest_baseline(
                Path(("project".to_string(), "bench".to_string())),
                Extension(auth.clone()),
                State(state.clone()),
            )
            .await,
        );

        assert_internal_error(
            get_baseline(
                Path(("project".to_string(), "bench".to_string(), "v1".to_string())),
                Extension(auth.clone()),
                State(state.clone()),
            )
            .await,
        );

        assert_internal_error(
            list_baselines(
                Path("project".to_string()),
                Extension(auth.clone()),
                State(state.clone()),
                Query(ListBaselinesQuery::new()),
            )
            .await,
        );

        assert_internal_error(
            delete_baseline(
                Path(("project".to_string(), "bench".to_string(), "v1".to_string())),
                Extension(auth.clone()),
                State(state.clone()),
            )
            .await,
        );

        assert_internal_error(
            promote_baseline(
                Path(("project".to_string(), "bench".to_string())),
                Extension(auth),
                State(state),
                Json(PromoteBaselineRequest {
                    from_version: "v1".to_string(),
                    to_version: "v2".to_string(),
                    git_ref: None,
                    git_sha: None,
                    tags: Vec::new(),
                    normalize: false,
                }),
            )
            .await,
        );
    }
}
