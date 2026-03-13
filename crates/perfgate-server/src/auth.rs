//! Authentication and authorization middleware.
//!
//! This module provides API key and JWT token validation for the baseline service.

use axum::{
    Json,
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::IntoResponse,
};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, errors::ErrorKind};
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

    /// Infers the closest built-in role from a set of scopes.
    pub fn from_scopes(scopes: &[Scope]) -> Self {
        if scopes.contains(&Scope::Admin) || scopes.contains(&Scope::Delete) {
            Self::Admin
        } else if scopes.contains(&Scope::Promote) {
            Self::Promoter
        } else if scopes.contains(&Scope::Write) {
            Self::Contributor
        } else {
            Self::Viewer
        }
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

/// JWT validation settings.
#[derive(Clone)]
pub struct JwtConfig {
    secret: Vec<u8>,
    issuer: Option<String>,
    audience: Option<String>,
}

impl JwtConfig {
    /// Creates an HS256 JWT configuration from raw secret bytes.
    pub fn hs256(secret: impl Into<Vec<u8>>) -> Self {
        Self {
            secret: secret.into(),
            issuer: None,
            audience: None,
        }
    }

    /// Sets the expected issuer claim.
    pub fn issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Sets the expected audience claim.
    pub fn audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    /// Returns the configured secret bytes.
    pub fn secret_bytes(&self) -> &[u8] {
        &self.secret
    }

    fn validation(&self) -> Validation {
        let mut validation = Validation::new(Algorithm::HS256);
        if let Some(issuer) = &self.issuer {
            validation.set_issuer(&[issuer.as_str()]);
        }
        if let Some(audience) = &self.audience {
            validation.set_audience(&[audience.as_str()]);
        }
        validation
    }
}

impl std::fmt::Debug for JwtConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtConfig")
            .field("secret", &"<redacted>")
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .finish()
    }
}

/// JWT claims accepted by the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JwtClaims {
    /// Subject identifier.
    pub sub: String,

    /// Project this token belongs to.
    pub project_id: String,

    /// Granted scopes.
    pub scopes: Vec<Scope>,

    /// Expiration timestamp (seconds since Unix epoch).
    pub exp: u64,

    /// Issued-at timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iat: Option<u64>,

    /// Optional issuer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,

    /// Optional audience.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
}

/// Authentication state shared by middleware.
#[derive(Clone, Debug)]
pub struct AuthState {
    /// In-memory API key store.
    pub key_store: Arc<ApiKeyStore>,

    /// Optional JWT validation settings.
    pub jwt: Option<JwtConfig>,
}

impl AuthState {
    /// Creates auth state from a key store and optional JWT configuration.
    pub fn new(key_store: Arc<ApiKeyStore>, jwt: Option<JwtConfig>) -> Self {
        Self { key_store, jwt }
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

    /// Creates an auth context facade from validated JWT claims.
    fn from_jwt_claims(claims: &JwtClaims) -> Self {
        Self {
            id: format!("jwt:{}", claims.sub),
            name: format!("JWT {}", claims.sub),
            project_id: claims.project_id.clone(),
            scopes: claims.scopes.clone(),
            role: Role::from_scopes(&claims.scopes),
            expires_at: format_timestamp(claims.exp),
            created_at: claims
                .iat
                .and_then(format_timestamp)
                .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            last_used_at: None,
        }
    }

    /// Checks if the key has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self
            .expires_at
            .as_ref()
            .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
        {
            return exp.timestamp() < chrono::Utc::now().timestamp();
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

enum Credentials {
    ApiKey(String),
    Jwt(String),
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

fn format_timestamp(timestamp: u64) -> Option<String> {
    chrono::DateTime::<chrono::Utc>::from_timestamp(timestamp as i64, 0).map(|dt| dt.to_rfc3339())
}

fn extract_credentials(headers: &HeaderMap) -> Option<Credentials> {
    let auth_header = headers.get(header::AUTHORIZATION)?.to_str().ok()?;

    if let Some(key) = auth_header.strip_prefix("Bearer ") {
        return Some(Credentials::ApiKey(key.to_string()));
    }

    if let Some(token) = auth_header.strip_prefix("Token ") {
        return Some(Credentials::Jwt(token.to_string()));
    }

    None
}

fn source_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
}

fn unauthorized(message: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiError::unauthorized(message)),
    )
}

async fn authenticate_api_key(
    key_store: &ApiKeyStore,
    api_key_str: &str,
    headers: &HeaderMap,
) -> Result<AuthContext, (StatusCode, Json<ApiError>)> {
    validate_key_format(api_key_str).map_err(|_| {
        warn!(
            key_prefix = &api_key_str[..10.min(api_key_str.len())],
            "Invalid API key format"
        );
        unauthorized("Invalid API key format")
    })?;

    let api_key = key_store.get_key(api_key_str).await.ok_or_else(|| {
        warn!(
            key_prefix = &api_key_str[..10.min(api_key_str.len())],
            "Invalid API key"
        );
        unauthorized("Invalid API key")
    })?;

    if api_key.is_expired() {
        warn!(key_id = %api_key.id, "API key expired");
        return Err(unauthorized("API key has expired"));
    }

    Ok(AuthContext {
        api_key,
        source_ip: source_ip(headers),
    })
}

fn validate_jwt(token: &str, config: &JwtConfig) -> Result<JwtClaims, AuthError> {
    let validation = config.validation();

    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(config.secret_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|error| match error.kind() {
        ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
        _ => AuthError::InvalidToken(error.to_string()),
    })
}

fn authenticate_jwt(
    config: Option<&JwtConfig>,
    token: &str,
    headers: &HeaderMap,
) -> Result<AuthContext, (StatusCode, Json<ApiError>)> {
    let config = config.ok_or_else(|| {
        warn!("JWT token received but JWT authentication is not configured");
        unauthorized("JWT token authentication is not configured")
    })?;

    let claims = validate_jwt(token, config).map_err(|error| {
        match &error {
            AuthError::ExpiredToken => warn!("Expired JWT token"),
            AuthError::InvalidToken(_) => warn!("Invalid JWT token"),
            _ => {}
        }
        unauthorized(&error.to_string())
    })?;

    Ok(AuthContext {
        api_key: ApiKey::from_jwt_claims(&claims),
        source_ip: source_ip(headers),
    })
}

/// Authentication middleware.
pub async fn auth_middleware(
    State(auth_state): State<AuthState>,
    mut request: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, Json<ApiError>)> {
    // Skip auth for health endpoint
    if request.uri().path() == "/health" {
        return Ok(next.run(request).await);
    }

    let auth_ctx = match extract_credentials(request.headers()) {
        Some(Credentials::ApiKey(api_key)) => {
            authenticate_api_key(&auth_state.key_store, &api_key, request.headers()).await?
        }
        Some(Credentials::Jwt(token)) => {
            authenticate_jwt(auth_state.jwt.as_ref(), &token, request.headers())?
        }
        None => {
            warn!("Missing authentication header");
            return Err(unauthorized("Missing authentication header"));
        }
    };

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
    use axum::{Extension, Router, routing::get};
    use jsonwebtoken::{Header, encode};
    use tower::ServiceExt;
    use uselesskey::{Factory, HmacFactoryExt, HmacSpec, Seed};
    use uselesskey_jsonwebtoken::JwtKeyExt;

    fn test_jwt_config() -> JwtConfig {
        let seed = Seed::from_env_value("perfgate-server-auth-tests").unwrap();
        let factory = Factory::deterministic(seed);
        let fixture = factory.hmac("jwt-auth", HmacSpec::hs256());
        JwtConfig::hs256(fixture.secret_bytes())
            .issuer("perfgate-tests")
            .audience("perfgate")
    }

    fn create_test_claims(scopes: Vec<Scope>, exp: u64) -> JwtClaims {
        JwtClaims {
            sub: "ci-bot".to_string(),
            project_id: "project-1".to_string(),
            scopes,
            exp,
            iat: Some(chrono::Utc::now().timestamp() as u64),
            iss: Some("perfgate-tests".to_string()),
            aud: Some("perfgate".to_string()),
        }
    }

    fn create_test_token(claims: &JwtClaims) -> String {
        let seed = Seed::from_env_value("perfgate-server-auth-tests").unwrap();
        let factory = Factory::deterministic(seed);
        let fixture = factory.hmac("jwt-auth", HmacSpec::hs256());
        encode(&Header::default(), claims, &fixture.encoding_key()).unwrap()
    }

    fn auth_test_router(auth_state: AuthState) -> Router {
        Router::new()
            .route(
                "/protected",
                get(|Extension(auth_ctx): Extension<AuthContext>| async move {
                    auth_ctx.api_key.id
                }),
            )
            .layer(axum::middleware::from_fn_with_state(
                auth_state,
                auth_middleware,
            ))
    }

    #[test]
    fn test_validate_key_format() {
        assert!(validate_key_format("pg_live_abcdefghijklmnopqrstuvwxyz123456").is_ok());
        assert!(validate_key_format("pg_test_abcdefghijklmnopqrstuvwxyz123456").is_ok());
        assert!(validate_key_format("invalid_abcdefghijklmnopqrstuvwxyz123456").is_err());
        assert!(validate_key_format("pg_live_short").is_err());
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
    fn test_role_from_scopes() {
        assert_eq!(Role::from_scopes(&[Scope::Read]), Role::Viewer);
        assert_eq!(
            Role::from_scopes(&[Scope::Read, Scope::Write]),
            Role::Contributor
        );
        assert_eq!(
            Role::from_scopes(&[Scope::Read, Scope::Write, Scope::Promote]),
            Role::Promoter
        );
        assert_eq!(Role::from_scopes(&[Scope::Delete]), Role::Admin);
    }

    #[test]
    fn test_validate_jwt_success() {
        let config = test_jwt_config();
        let claims = create_test_claims(
            vec![Scope::Read, Scope::Write],
            (chrono::Utc::now() + chrono::Duration::minutes(5)).timestamp() as u64,
        );
        let token = create_test_token(&claims);

        let decoded = validate_jwt(&token, &config).unwrap();

        assert_eq!(decoded.sub, "ci-bot");
        assert_eq!(decoded.project_id, "project-1");
        assert_eq!(decoded.scopes, vec![Scope::Read, Scope::Write]);
    }

    #[test]
    fn test_validate_jwt_expired() {
        let config = test_jwt_config();
        let claims = create_test_claims(
            vec![Scope::Read],
            (chrono::Utc::now() - chrono::Duration::minutes(5)).timestamp() as u64,
        );
        let token = create_test_token(&claims);

        let err = validate_jwt(&token, &config).unwrap_err();
        assert!(matches!(err, AuthError::ExpiredToken));
    }

    #[test]
    fn test_api_key_expiration() {
        let mut key = ApiKey::new(
            "key-1".to_string(),
            "Test Key".to_string(),
            "project-1".to_string(),
            Role::Viewer,
        );

        assert!(!key.is_expired());

        key.expires_at = Some("2020-01-01T00:00:00Z".to_string());
        assert!(key.is_expired());

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

        store.add_key(key.clone(), &raw_key).await;

        let retrieved = store.get_key(&raw_key).await;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "key-1");
        assert_eq!(retrieved.role, Role::Contributor);

        let keys = store.list_keys().await;
        assert_eq!(keys.len(), 1);

        let removed = store.remove_key(&raw_key).await;
        assert!(removed);

        let retrieved = store.get_key(&raw_key).await;
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_auth_middleware_accepts_api_key() {
        let store = Arc::new(ApiKeyStore::new());
        let key = "pg_test_abcdefghijklmnopqrstuvwxyz123456";
        store
            .add_key(
                ApiKey::new(
                    "api-key-1".to_string(),
                    "API Key".to_string(),
                    "project-1".to_string(),
                    Role::Viewer,
                ),
                key,
            )
            .await;

        let response = auth_test_router(AuthState::new(store, None))
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header(header::AUTHORIZATION, format!("Bearer {}", key))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_accepts_jwt_token() {
        let claims = create_test_claims(
            vec![Scope::Read, Scope::Promote],
            (chrono::Utc::now() + chrono::Duration::minutes(5)).timestamp() as u64,
        );
        let token = create_test_token(&claims);

        let response = auth_test_router(AuthState::new(
            Arc::new(ApiKeyStore::new()),
            Some(test_jwt_config()),
        ))
        .oneshot(
            Request::builder()
                .uri("/protected")
                .header(header::AUTHORIZATION, format!("Token {}", token))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_rejects_jwt_when_unconfigured() {
        let claims = create_test_claims(
            vec![Scope::Read],
            (chrono::Utc::now() + chrono::Duration::minutes(5)).timestamp() as u64,
        );
        let token = create_test_token(&claims);

        let response = auth_test_router(AuthState::new(Arc::new(ApiKeyStore::new()), None))
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header(header::AUTHORIZATION, format!("Token {}", token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_generate_api_key() {
        let live_key = generate_api_key(false);
        assert!(live_key.starts_with(API_KEY_PREFIX_LIVE));
        assert!(live_key.len() >= 40);

        let test_key = generate_api_key(true);
        assert!(test_key.starts_with(API_KEY_PREFIX_TEST));
        assert!(test_key.len() >= 40);
    }

    #[test]
    fn test_hash_api_key() {
        let key = "pg_live_test123456789012345678901234567890";
        let hash1 = hash_api_key(key);
        let hash2 = hash_api_key(key);

        assert_eq!(hash1, hash2);

        let different_hash = hash_api_key("pg_live_different1234567890123456789012");
        assert_ne!(hash1, different_hash);
    }
}
