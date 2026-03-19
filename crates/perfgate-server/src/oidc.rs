//! OIDC authentication support for perfgate-server.

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use perfgate_auth::{ApiKey, Role};
use perfgate_error::AuthError;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// OIDC configuration.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// URL to fetch JWKS from (e.g. `https://token.actions.githubusercontent.com/.well-known/jwks.json`)
    pub jwks_url: String,

    /// Expected issuer (e.g. `https://token.actions.githubusercontent.com`)
    pub issuer: String,

    /// Expected audience
    pub audience: String,

    /// Mapping from GitHub repository to project ID and Role.
    /// E.g. "org/repo" -> ("project-id", Role::Contributor)
    pub repo_mappings: HashMap<String, (String, Role)>,
}

/// A provider that periodically fetches JWKS and validates tokens.
#[derive(Clone)]
pub struct OidcProvider {
    config: OidcConfig,
    jwks: Arc<RwLock<Option<JwkSet>>>,
    client: Client,
}

/// GitHub Actions OIDC claims.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubClaims {
    iss: String,
    aud: String,
    sub: String,
    repository: String,
    exp: u64,
    iat: Option<u64>,
}

impl OidcProvider {
    /// Creates a new OIDC provider and immediately attempts to fetch the JWKS.
    pub async fn new(config: OidcConfig) -> Result<Self, AuthError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| AuthError::InvalidToken(format!("Failed to build HTTP client: {}", e)))?;

        let provider = Self {
            config,
            jwks: Arc::new(RwLock::new(None)),
            client,
        };

        // Initial fetch
        if let Err(e) = provider.refresh_jwks().await {
            warn!("Failed initial JWKS fetch from {}: {}", provider.config.jwks_url, e);
        }

        Ok(provider)
    }

    /// Refreshes the JWKS from the configured URL.
    pub async fn refresh_jwks(&self) -> Result<(), AuthError> {
        debug!("Fetching JWKS from {}", self.config.jwks_url);
        let res = self
            .client
            .get(&self.config.jwks_url)
            .send()
            .await
            .map_err(|e| AuthError::InvalidToken(format!("JWKS fetch error: {}", e)))?;

        if !res.status().is_success() {
            return Err(AuthError::InvalidToken(format!(
                "JWKS endpoint returned status {}",
                res.status()
            )));
        }

        let jwks: JwkSet = res
            .json()
            .await
            .map_err(|e| AuthError::InvalidToken(format!("Failed to parse JWKS: {}", e)))?;

        info!("Successfully loaded JWKS ({} keys)", jwks.keys.len());
        let mut cache = self.jwks.write().await;
        *cache = Some(jwks);

        Ok(())
    }

    /// Validates an OIDC token and returns the corresponding mapped ApiKey.
    pub async fn validate_token(&self, token: &str) -> Result<ApiKey, AuthError> {
        let header = decode_header(token).map_err(|e| AuthError::InvalidToken(e.to_string()))?;
        
        let kid = header
            .kid
            .ok_or_else(|| AuthError::InvalidToken("Missing 'kid' in token header".to_string()))?;

        // Extract decoding key
        let decoding_key = {
            let cache = self.jwks.read().await;
            let jwks = cache
                .as_ref()
                .ok_or_else(|| AuthError::InvalidToken("JWKS not loaded yet".to_string()))?;

            let jwk = jwks
                .find(&kid)
                .ok_or_else(|| AuthError::InvalidToken(format!("Key '{}' not found in JWKS", kid)))?;

            match &jwk.algorithm {
                jsonwebtoken::jwk::AlgorithmParameters::RSA(rsa) => {
                    DecodingKey::from_rsa_components(&rsa.n, &rsa.e)
                        .map_err(|e| AuthError::InvalidToken(format!("Invalid RSA key: {}", e)))?
                }
                _ => {
                    return Err(AuthError::InvalidToken(
                        "Unsupported key algorithm (expected RSA)".to_string(),
                    ));
                }
            }
        };

        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_audience(&[&self.config.audience]);

        let token_data = decode::<GithubClaims>(token, &decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
                _ => AuthError::InvalidToken(e.to_string()),
            })?;

        let claims = token_data.claims;

        // Map repository to project and role
        let (project_id, role) = self
            .config
            .repo_mappings
            .get(&claims.repository)
            .ok_or_else(|| {
                AuthError::InvalidToken(format!(
                    "Repository '{}' is not authorized",
                    claims.repository
                ))
            })?;

        let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(claims.exp as i64, 0)
            .unwrap_or_else(chrono::Utc::now);
            
        let created_at = claims
            .iat
            .and_then(|iat| chrono::DateTime::<chrono::Utc>::from_timestamp(iat as i64, 0))
            .unwrap_or_else(chrono::Utc::now);

        Ok(ApiKey {
            id: format!("oidc:{}", claims.sub),
            name: format!("GitHub Actions ({})", claims.repository),
            project_id: project_id.clone(),
            scopes: role.allowed_scopes(),
            role: *role,
            benchmark_regex: None,
            expires_at: Some(expires_at),
            created_at,
            last_used_at: None,
        })
    }
}
