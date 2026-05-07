//! API key management handlers (admin-only).

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::auth::{AuthContext, Role, Scope};
use crate::models::{
    ApiError, AuditAction, AuditEvent, AuditResourceType, CreateKeyRequest, CreateKeyResponse,
    KeyEntry, ListKeysResponse, RevokeKeyResponse,
};
use crate::server::AppState;
use crate::storage::{KeyRecord, KeyStore, hash_key, key_prefix};

/// POST /api/v1/keys — create a new API key (admin-only).
///
/// Returns the plaintext key exactly once in the response.
pub async fn create_key(
    Extension(auth_ctx): Extension<AuthContext>,
    Extension(store): Extension<Arc<dyn KeyStore>>,
    State(state): State<AppState>,
    Json(request): Json<CreateKeyRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Admin scope check — use a wildcard project since key management is global.
    check_admin(&auth_ctx)?;

    // Validate description is not empty
    if request.description.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError::validation("description must not be empty")),
        ));
    }

    // Generate a new plaintext key
    let plaintext = perfgate_types::baseline_service::auth::generate_api_key(false);
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();

    let record = KeyRecord {
        id: id.clone(),
        key_hash: hash_key(&plaintext),
        key_prefix: key_prefix(&plaintext),
        role: request.role,
        project: request.project.clone(),
        pattern: request.pattern.clone(),
        description: request.description.clone(),
        created_at: now,
        expires_at: request.expires_at,
        revoked_at: None,
    };

    store.create_key(&record).await.map_err(|e| {
        error!(error = %e, "Failed to create API key");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )
    })?;

    info!(
        actor = %auth_ctx.api_key.id,
        key_id = %id,
        role = %record.role,
        project = %record.project,
        "API key created"
    );

    emit_key_audit(
        &state,
        &auth_ctx,
        AuditAction::Create,
        &id,
        &record.project,
        Some(record.role),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(CreateKeyResponse {
            id,
            key: plaintext,
            description: request.description,
            role: request.role,
            project: request.project,
            pattern: request.pattern,
            created_at: now,
            expires_at: request.expires_at,
        }),
    ))
}

/// GET /api/v1/keys — list all API keys (admin-only, redacted).
pub async fn list_keys(
    Extension(auth_ctx): Extension<AuthContext>,
    Extension(store): Extension<Arc<dyn KeyStore>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_admin(&auth_ctx)?;

    let records = store.list_keys().await.map_err(|e| {
        error!(error = %e, "Failed to list API keys");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )
    })?;

    let keys: Vec<KeyEntry> = records
        .into_iter()
        .map(|r| KeyEntry {
            id: r.id,
            key_prefix: r.key_prefix,
            description: r.description,
            role: r.role,
            project: r.project,
            pattern: r.pattern,
            created_at: r.created_at,
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
        })
        .collect();

    Ok(Json(ListKeysResponse { keys }))
}

/// DELETE /api/v1/keys/{id} — revoke an API key (admin-only).
pub async fn revoke_key(
    Path(id): Path<String>,
    Extension(auth_ctx): Extension<AuthContext>,
    Extension(store): Extension<Arc<dyn KeyStore>>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    check_admin(&auth_ctx)?;

    let key_for_audit = match store.list_keys().await {
        Ok(records) => records.into_iter().find(|record| record.id == id),
        Err(e) => {
            warn!(error = %e, key_id = %id, "Failed to load key metadata for audit");
            None
        }
    };

    let revoked_at = store.revoke_key(&id).await.map_err(|e| {
        error!(error = %e, key_id = %id, "Failed to revoke API key");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError::internal_error(&e.to_string())),
        )
    })?;

    match revoked_at {
        Some(ts) => {
            info!(actor = %auth_ctx.api_key.id, key_id = %id, "API key revoked");
            let audit_project = key_for_audit
                .as_ref()
                .map(|record| record.project.as_str())
                .unwrap_or(&auth_ctx.api_key.project_id);
            let audit_role = key_for_audit.as_ref().map(|record| record.role);
            emit_key_audit(
                &state,
                &auth_ctx,
                AuditAction::Delete,
                &id,
                audit_project,
                audit_role,
            )
            .await;
            Ok(Json(RevokeKeyResponse { id, revoked_at: ts }))
        }
        None => {
            warn!(key_id = %id, "Attempted to revoke non-existent key");
            Err((
                StatusCode::NOT_FOUND,
                Json(ApiError::not_found(&format!("Key {} not found", id))),
            ))
        }
    }
}

/// Helper: verify the caller has admin scope.
fn check_admin(auth_ctx: &AuthContext) -> Result<(), (StatusCode, Json<ApiError>)> {
    // For key management, we use a special project identifier.
    // Admins can manage keys regardless of their project scoping.
    if !auth_ctx.api_key.has_scope(Scope::Admin) {
        warn!(
            key_id = %auth_ctx.api_key.id,
            "Non-admin attempted key management"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::forbidden("Requires 'admin' permission")),
        ));
    }
    Ok(())
}

async fn emit_key_audit(
    state: &AppState,
    auth_ctx: &AuthContext,
    action: AuditAction,
    key_id: &str,
    project: &str,
    role: Option<Role>,
) {
    let metadata = serde_json::json!({
        "source": "api_key",
        "request_id": null,
        "source_ip": auth_ctx.source_ip.clone(),
        "role": role.map(|r| r.to_string()),
    });
    let event = AuditEvent {
        id: crate::models::generate_ulid(),
        timestamp: chrono::Utc::now(),
        actor: auth_ctx.api_key.id.clone(),
        action,
        resource_type: AuditResourceType::Key,
        resource_id: key_id.to_string(),
        project: project.to_string(),
        metadata,
    };

    if let Err(e) = state.audit.log_event(&event).await {
        warn!(error = %e, key_id = %key_id, "Failed to log key audit event");
    }
}
