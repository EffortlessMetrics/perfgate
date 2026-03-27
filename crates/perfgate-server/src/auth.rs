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
pub use perfgate_auth::{ApiKey, JwtClaims, Role, Scope, validate_key_format};
use perfgate_error::AuthError;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

use crate::models::ApiError;
use crate::oidc::OidcProvider;
use crate::storage::KeyStore;

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

/// Authentication state shared by middleware.
#[derive(Clone)]
pub struct AuthState {
    /// In-memory API key store (for CLI-provided keys).
    pub key_store: Arc<ApiKeyStore>,

    /// Persistent key store (database-backed).
    pub persistent_key_store: Option<Arc<dyn KeyStore>>,

    /// Optional JWT validation settings.
    pub jwt: Option<JwtConfig>,

    /// Optional OIDC provider.
    pub oidc: Option<OidcProvider>,
}

impl AuthState {
    /// Creates auth state from a key store and optional JWT/OIDC configuration.
    pub fn new(
        key_store: Arc<ApiKeyStore>,
        jwt: Option<JwtConfig>,
        oidc: Option<OidcProvider>,
    ) -> Self {
        Self {
            key_store,
            persistent_key_store: None,
            jwt,
            oidc,
        }
    }

    /// Adds a persistent key store for database-backed key validation.
    pub fn with_persistent_key_store(mut self, store: Arc<dyn KeyStore>) -> Self {
        self.persistent_key_store = Some(store);
        self
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
    auth_state: &AuthState,
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

    // Try the in-memory store first (CLI-provided keys)
    if let Some(api_key) = auth_state.key_store.get_key(api_key_str).await {
        if api_key.is_expired() {
            warn!(key_id = %api_key.id, "API key expired");
            return Err(unauthorized("API key has expired"));
        }
        return Ok(AuthContext {
            api_key,
            source_ip: source_ip(headers),
        });
    }

    // Try the persistent key store (database-backed keys)
    if let Some(persistent) = &auth_state.persistent_key_store
        && let Ok(Some(record)) = persistent.validate_key(api_key_str).await
    {
        let mut api_key = ApiKey::new(
            record.id.clone(),
            record.description.clone(),
            record.project.clone(),
            record.role,
        );
        // Apply benchmark pattern as regex
        api_key.benchmark_regex = record.pattern.clone();
        api_key.expires_at = record.expires_at;
        api_key.created_at = record.created_at;

        return Ok(AuthContext {
            api_key,
            source_ip: source_ip(headers),
        });
    }

    warn!(
        key_prefix = &api_key_str[..10.min(api_key_str.len())],
        "Invalid API key"
    );
    Err(unauthorized("Invalid API key"))
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

async fn authenticate_jwt(
    auth_state: &AuthState,
    token: &str,
    headers: &HeaderMap,
) -> Result<AuthContext, (StatusCode, Json<ApiError>)> {
    // Try static JWT config if available
    if let Some(config) = &auth_state.jwt {
        match validate_jwt(token, config) {
            Ok(claims) => {
                return Ok(AuthContext {
                    api_key: api_key_from_jwt_claims(&claims),
                    source_ip: source_ip(headers),
                });
            }
            Err(e) => {
                // If we don't have an OIDC provider, fail here.
                // Otherwise, fall through to OIDC.
                if auth_state.oidc.is_none() {
                    match &e {
                        AuthError::ExpiredToken => warn!("Expired JWT token"),
                        AuthError::InvalidToken(_) => warn!("Invalid JWT token"),
                        _ => {}
                    }
                    return Err(unauthorized(&e.to_string()));
                }
            }
        }
    }

    // Try OIDC provider if available
    if let Some(oidc) = &auth_state.oidc {
        match oidc.validate_token(token).await {
            Ok(api_key) => {
                return Ok(AuthContext {
                    api_key,
                    source_ip: source_ip(headers),
                });
            }
            Err(e) => {
                match &e {
                    AuthError::ExpiredToken => warn!("Expired OIDC token"),
                    AuthError::InvalidToken(msg) => warn!("Invalid OIDC token: {}", msg),
                    _ => {}
                }
                return Err(unauthorized(&e.to_string()));
            }
        }
    }

    warn!("JWT token received but no JWT or OIDC authentication is configured");
    Err(unauthorized("JWT/OIDC authentication is not configured"))
}

fn api_key_from_jwt_claims(claims: &JwtClaims) -> ApiKey {
    ApiKey {
        id: format!("jwt:{}", claims.sub),
        name: format!("JWT {}", claims.sub),
        project_id: claims.project_id.clone(),
        scopes: claims.scopes.clone(),
        role: Role::from_scopes(&claims.scopes),
        benchmark_regex: None,
        expires_at: Some(
            chrono::DateTime::<chrono::Utc>::from_timestamp(claims.exp as i64, 0)
                .unwrap_or_else(chrono::Utc::now),
        ),
        created_at: claims
            .iat
            .and_then(|iat| chrono::DateTime::<chrono::Utc>::from_timestamp(iat as i64, 0))
            .unwrap_or_else(chrono::Utc::now),
        last_used_at: None,
    }
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
            authenticate_api_key(&auth_state, &api_key, request.headers()).await?
        }
        Some(Credentials::Jwt(token)) => {
            authenticate_jwt(&auth_state, &token, request.headers()).await?
        }
        None => {
            warn!("Missing authentication header");
            return Err(unauthorized("Missing authentication header"));
        }
    };

    request.extensions_mut().insert(auth_ctx);

    Ok(next.run(request).await)
}

/// Checks if the current auth context has the required scope, project access, and benchmark access.
/// Returns an error response if the scope is not present, project mismatch, or benchmark restricted.
pub fn check_scope(
    auth_ctx: Option<&AuthContext>,
    project_id: &str,
    benchmark: Option<&str>,
    scope: Scope,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let ctx = match auth_ctx {
        Some(ctx) => ctx,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError::unauthorized("Authentication required")),
            ));
        }
    };

    // 1. Check Scope
    if !ctx.api_key.has_scope(scope) {
        warn!(
            key_id = %ctx.api_key.id,
            required_scope = %scope,
            actual_role = %ctx.api_key.role,
            "Insufficient permissions: scope mismatch"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::forbidden(&format!(
                "Requires '{}' permission",
                scope
            ))),
        ));
    }

    // 2. Check Project Isolation
    // Global admins (those with Scope::Admin) can access any project.
    // Otherwise, the key's project_id must match the requested project_id.
    if !ctx.api_key.has_scope(Scope::Admin) && ctx.api_key.project_id != project_id {
        warn!(
            key_id = %ctx.api_key.id,
            key_project = %ctx.api_key.project_id,
            requested_project = %project_id,
            "Insufficient permissions: project isolation violation"
        );
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError::forbidden(&format!(
                "Key is restricted to project '{}'",
                ctx.api_key.project_id
            ))),
        ));
    }

    // 3. Check Benchmark Restriction
    // If the key has a benchmark_regex, all accessed benchmarks must match it.
    if let (Some(regex_str), Some(bench)) = (&ctx.api_key.benchmark_regex, benchmark) {
        let regex = regex::Regex::new(regex_str).map_err(|e| {
            warn!(key_id = %ctx.api_key.id, regex = %regex_str, error = %e, "Invalid benchmark regex in API key");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError::internal_error("Invalid security configuration")),
            )
        })?;

        if !regex.is_match(bench) {
            warn!(
                key_id = %ctx.api_key.id,
                benchmark = %bench,
                regex = %regex_str,
                "Insufficient permissions: benchmark restriction violation"
            );
            return Err((
                StatusCode::FORBIDDEN,
                Json(ApiError::forbidden(&format!(
                    "Key is restricted to benchmarks matching '{}'",
                    regex_str
                ))),
            ));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Extension, Router, routing::get};
    use jsonwebtoken::{Header, encode};
    use perfgate_auth::generate_api_key;
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

        let response = auth_test_router(AuthState::new(store, None, None))
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
            None,
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

        let response = auth_test_router(AuthState::new(Arc::new(ApiKeyStore::new()), None, None))
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
    fn test_hash_api_key() {
        let key = "pg_live_test123456789012345678901234567890";
        let hash1 = hash_api_key(key);
        let hash2 = hash_api_key(key);

        assert_eq!(hash1, hash2);

        let different_hash = hash_api_key("pg_live_different1234567890123456789012");
        assert_ne!(hash1, different_hash);
    }

    #[test]
    fn test_check_scope_project_isolation() {
        let key = ApiKey::new(
            "k1".to_string(),
            "n1".to_string(),
            "project-a".to_string(),
            Role::Contributor,
        );
        let ctx = AuthContext {
            api_key: key,
            source_ip: None,
        };

        // Same project, correct scope -> OK
        assert!(check_scope(Some(&ctx), "project-a", None, Scope::Write).is_ok());
        assert!(check_scope(Some(&ctx), "project-a", None, Scope::Read).is_ok());

        // Same project, wrong scope -> Forbidden
        let res = check_scope(Some(&ctx), "project-a", None, Scope::Delete);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, StatusCode::FORBIDDEN);

        // Different project -> Forbidden
        let res = check_scope(Some(&ctx), "project-b", None, Scope::Read);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, StatusCode::FORBIDDEN);
    }

    #[test]
    fn test_check_scope_global_admin() {
        let key = ApiKey::new(
            "k1".to_string(),
            "admin".to_string(),
            "any-project".to_string(),
            Role::Admin,
        );
        let ctx = AuthContext {
            api_key: key,
            source_ip: None,
        };

        // Global admin can access ANY project
        assert!(check_scope(Some(&ctx), "project-a", None, Scope::Read).is_ok());
        assert!(check_scope(Some(&ctx), "project-b", None, Scope::Delete).is_ok());
        assert!(check_scope(Some(&ctx), "other", None, Scope::Admin).is_ok());
    }

    #[test]
    fn test_check_scope_benchmark_restriction() {
        let mut key = ApiKey::new(
            "k1".to_string(),
            "n1".to_string(),
            "project-a".to_string(),
            Role::Contributor,
        );
        key.benchmark_regex = Some("^web-.*$".to_string());

        let ctx = AuthContext {
            api_key: key,
            source_ip: None,
        };

        // Matches regex -> OK
        assert!(check_scope(Some(&ctx), "project-a", Some("web-auth"), Scope::Read).is_ok());
        assert!(check_scope(Some(&ctx), "project-a", Some("web-api"), Scope::Write).is_ok());

        // Does not match regex -> Forbidden
        let res = check_scope(Some(&ctx), "project-a", Some("worker-job"), Scope::Read);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, StatusCode::FORBIDDEN);

        // No benchmark name provided (e.g. list operation) -> OK (scoping only applies to explicit access)
        assert!(check_scope(Some(&ctx), "project-a", None, Scope::Read).is_ok());
    }
}
