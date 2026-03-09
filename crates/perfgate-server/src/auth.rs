//! Authentication and authorization middleware.
//!
//! This module provides API key and JWT token validation for the baseline service.

use axum::{
    Extension, Json,
    extract::Request,
    http::{StatusCode, header},
    middleware::Next,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

use crate::error::AuthError;
use crate::models::ApiError;

/// API key prefix for live keys.
pub const API_KEY_PREFIX_LIVE: &str = "pg_live_";

/// API key prefix for test keys.
pub const API_KEY_PREFIX_TEST: &str = "pg_test_";

/// Permission scope for API operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Read-only access
    Read,
    /// Write/upload access
    Write,
    /// Promote baselines
    Promote,
    /// Delete baselines
    Delete,
    /// Admin operations
    Admin,
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Scope::Read => write!(f, "read"),
            Scope::Write => write!(f, "write"),
            Scope::Promote => write!(f, "promote"),
            Scope::Delete => write!(f, "delete"),
            Scope::Admin => write!(f, "admin"),
        }
    }
}

/// Role-based access control.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Read-only access
    Viewer,
    /// Can upload and read baselines
    Contributor,
    /// Can promote baselines to production
    Promoter,
    /// Full access including delete
    Admin,
}

impl Role {
    /// Returns the scopes allowed for this role.
    pub fn allowed_scopes(&self) -> Vec<Scope> {
        match self {
            Role::Viewer => vec![Scope::Read],
            Role::Contributor => vec![Scope::Read, Scope::Write],
            Role::Promoter => vec![Scope::Read, Scope::Write, Scope::Promote],
            Role::Admin => vec![
                Scope::Read,
                Scope::Write,
                Scope::Promote,
                Scope::Delete,
                Scope::Admin,
            ],
        }
    }

    /// Checks if this role has a specific scope.
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.allowed_scopes().contains(&scope)
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Viewer => write!(f, "viewer"),
            Role::Contributor => write!(f, "contributor"),
            Role::Promoter => write!(f, "promoter"),
            Role::Admin => write!(f, "admin"),
        }
    }
}

/// Represents an authenticated API key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique key identifier
    pub id: String,

    /// Key name/description
    pub name: String,

    /// Project this key belongs to
    pub project_id: String,

    /// Granted scopes
    pub scopes: Vec<Scope>,

    /// Role (for easier permission checks)
    pub role: Role,

    /// Expiration timestamp (RFC 3339)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,

    /// Creation timestamp
    pub created_at: String,

    /// Last usage timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
}

impl ApiKey {
    /// Creates a new API key with the given role.
    pub fn new(id: String, name: String, project_id: String, role: Role) -> Self {
        Self {
            id,
            name,
            project_id,
            scopes: role.allowed_scopes(),
            role,
            expires_at: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_used_at: None,
        }
    }

    /// Checks if the key has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(ref expires_at) = self.expires_at {
            if let Ok(exp) = chrono::DateTime::parse_from_rfc3339(expires_at) {
                return exp.timestamp() < chrono::Utc::now().timestamp();
            }
        }
        false
    }

    /// Checks if the key has a specific scope.
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }
}

/// Authenticated user context extracted from requests.
#[derive(Debug, Clone)]
pub struct AuthContext {
    /// API key information
    pub api_key: ApiKey,

    /// Source IP address
    pub source_ip: Option<String>,
}

/// In-memory API key store for development and testing.
#[derive(Debug, Default)]
pub struct ApiKeyStore {
    /// Keys indexed by key hash
    keys: Arc<RwLock<HashMap<String, ApiKey>>>,
}

impl ApiKeyStore {
    /// Creates a new empty key store.
    pub fn new() -> Self {
        Self {
            keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Adds an API key to the store.
    pub async fn add_key(&self, key: ApiKey, raw_key: &str) {
        let hash = hash_api_key(raw_key);
        let mut keys = self.keys.write().await;
        keys.insert(hash, key);
    }

    /// Looks up an API key by its hash.
    pub async fn get_key(&self, raw_key: &str) -> Option<ApiKey> {
        let hash = hash_api_key(raw_key);
        let keys = self.keys.read().await;
        keys.get(&hash).cloned()
    }

    /// Removes an API key from the store.
    pub async fn remove_key(&self, raw_key: &str) -> bool {
        let hash = hash_api_key(raw_key);
        let mut keys = self.keys.write().await;
        keys.remove(&hash).is_some()
    }

    /// Lists all API keys (without sensitive data).
    pub async fn list_keys(&self) -> Vec<ApiKey> {
        let keys = self.keys.read().await;
        keys.values().cloned().collect()
    }
}

/// Hashes an API key for storage.
fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Validates API key format.
pub fn validate_key_format(key: &str) -> Result<(), AuthError> {
    if key.starts_with(API_KEY_PREFIX_LIVE) || key.starts_with(API_KEY_PREFIX_TEST) {
        let remainder = key
            .strip_prefix(API_KEY_PREFIX_LIVE)
            .or_else(|| key.strip_prefix(API_KEY_PREFIX_TEST))
            .unwrap();

        // Check that the remainder is at least 32 characters
        if remainder.len() >= 32 && remainder.chars().all(|c| c.is_alphanumeric()) {
            return Ok(());
        }
    }

    Err(AuthError::InvalidKeyFormat)
}

/// Extracts the API key from the Authorization header.
fn extract_api_key(headers: &axum::http::HeaderMap) -> Option<String> {
    let auth_header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;

    // Support "Bearer <key>" format
    if let Some(key) = auth_header.strip_prefix("Bearer ") {
        return Some(key.to_string());
    }

    // Support "Token <key>" format
    if let Some(key) = auth_header.strip_prefix("Token ") {
        return Some(key.to_string());
    }

    None
}

/// Authentication middleware.
pub async fn auth_middleware(
    Extension(key_store): Extension<Arc<ApiKeyStore>>,
    mut request: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Skip auth for health endpoint
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    // Extract API key
    let api_key_str = extract_api_key(request.headers()).ok_or_else(|| {
        warn!("Missing authentication header");
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::unauthorized("Missing authentication header")),
        )
    })?;

    // Validate format
    validate_key_format(&api_key_str).map_err(|_| {
        warn!(
            key_prefix = &api_key_str[..10.min(api_key_str.len())],
            "Invalid API key format"
        );
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::unauthorized("Invalid API key format")),
        )
    })?;

    // Look up key
    let api_key = key_store.get_key(&api_key_str).await.ok_or_else(|| {
        warn!(
            key_prefix = &api_key_str[..10.min(api_key_str.len())],
            "Invalid API key"
        );
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiError::unauthorized("Invalid API key")),
        )
    })?;

    // Check expiration
    if api_key.is_expired() {
        warn!(key_id = %api_key.id, "API key expired");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::unauthorized("API key has expired")),
        ));
    }

    // Create auth context
    let auth_ctx = AuthContext {
        api_key,
        source_ip: request
            .headers()
            .get("X-Forwarded-For")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string()),
    };

    // Add auth context to request extensions
    request.extensions_mut().insert(auth_ctx);

    Ok(next.run(request).await)
}

/// Checks if the current auth context has the required scope.
/// Returns an error response if the scope is not present.
pub fn check_scope(
    auth_ctx: Option<&AuthContext>,
    scope: Scope,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    match auth_ctx {
        Some(ctx) if ctx.api_key.has_scope(scope) => Ok(()),
        Some(ctx) => {
            warn!(
                key_id = %ctx.api_key.id,
                required_scope = %scope,
                actual_role = %ctx.api_key.role,
                "Insufficient permissions"
            );
            Err((
                StatusCode::FORBIDDEN,
                Json(ApiError::forbidden(&format!(
                    "Requires '{}' permission",
                    scope
                ))),
            ))
        }
        None => Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError::unauthorized("Authentication required")),
        )),
    }
}

/// Creates a new API key string.
pub fn generate_api_key(test: bool) -> String {
    let prefix = if test {
        API_KEY_PREFIX_TEST
    } else {
        API_KEY_PREFIX_LIVE
    };
    let random: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(32)
        .collect();
    format!("{}{}", prefix, random)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_key_format() {
        // Valid live key
        assert!(validate_key_format("pg_live_abcdefghijklmnopqrstuvwxyz123456").is_ok());

        // Valid test key
        assert!(validate_key_format("pg_test_abcdefghijklmnopqrstuvwxyz123456").is_ok());

        // Invalid prefix
        assert!(validate_key_format("invalid_abcdefghijklmnopqrstuvwxyz123456").is_err());

        // Too short
        assert!(validate_key_format("pg_live_short").is_err());

        // Invalid characters
        assert!(validate_key_format("pg_live_abcdefghijklmnopqrstuvwxyz12345!@").is_err());
    }

    #[test]
    fn test_role_scopes() {
        let viewer = Role::Viewer;
        assert!(viewer.has_scope(Scope::Read));
        assert!(!viewer.has_scope(Scope::Write));

        let contributor = Role::Contributor;
        assert!(contributor.has_scope(Scope::Read));
        assert!(contributor.has_scope(Scope::Write));
        assert!(!contributor.has_scope(Scope::Promote));

        let promoter = Role::Promoter;
        assert!(promoter.has_scope(Scope::Promote));
        assert!(!promoter.has_scope(Scope::Delete));

        let admin = Role::Admin;
        assert!(admin.has_scope(Scope::Delete));
        assert!(admin.has_scope(Scope::Admin));
    }

    #[test]
    fn test_api_key_expiration() {
        let mut key = ApiKey::new(
            "key-1".to_string(),
            "Test Key".to_string(),
            "project-1".to_string(),
            Role::Viewer,
        );

        // No expiration
        assert!(!key.is_expired());

        // Expired in the past
        key.expires_at = Some("2020-01-01T00:00:00Z".to_string());
        assert!(key.is_expired());

        // Expires in the future
        key.expires_at = Some("2099-01-01T00:00:00Z".to_string());
        assert!(!key.is_expired());
    }

    #[tokio::test]
    async fn test_api_key_store() {
        let store = ApiKeyStore::new();
        let raw_key = generate_api_key(false);
        let key = ApiKey::new(
            "key-1".to_string(),
            "Test Key".to_string(),
            "project-1".to_string(),
            Role::Contributor,
        );

        // Add key
        store.add_key(key.clone(), &raw_key).await;

        // Retrieve key
        let retrieved = store.get_key(&raw_key).await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "key-1");
        assert_eq!(retrieved.role, Role::Contributor);

        // List keys
        let keys = store.list_keys().await;
        assert_eq!(keys.len(), 1);

        // Remove key
        let removed = store.remove_key(&raw_key).await;
        assert!(removed);

        // Key no longer exists
        let retrieved = store.get_key(&raw_key).await;
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_generate_api_key() {
        let live_key = generate_api_key(false);
        assert!(live_key.starts_with(API_KEY_PREFIX_LIVE));
        // pg_live_ (8 chars) + 32 random chars = 40 chars
        assert!(live_key.len() >= 40);

        let test_key = generate_api_key(true);
        assert!(test_key.starts_with(API_KEY_PREFIX_TEST));
        // pg_test_ (8 chars) + 32 random chars = 40 chars
        assert!(test_key.len() >= 40);
    }

    #[test]
    fn test_hash_api_key() {
        let key = "pg_live_test123456789012345678901234567890";
        let hash1 = hash_api_key(key);
        let hash2 = hash_api_key(key);

        // Same input produces same hash
        assert_eq!(hash1, hash2);

        // Different input produces different hash
        let different_hash = hash_api_key("pg_live_different1234567890123456789012");
        assert_ne!(hash1, different_hash);
    }
}
