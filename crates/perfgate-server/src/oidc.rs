//! OIDC authentication support for perfgate-server.
//!
//! Supports multiple OIDC providers:
//! - **GitHub Actions**: Maps `repository` claim to project/role.
//! - **GitLab CI**: Maps `project_path` claim to project/role.
//! - **Custom providers**: Configurable issuer, JWKS URL, and claim field.

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use perfgate_auth::{ApiKey, Role};
use perfgate_error::AuthError;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Provider types
// ---------------------------------------------------------------------------

/// Identifies which OIDC provider issued the token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OidcProviderType {
    /// GitHub Actions (`https://token.actions.githubusercontent.com`)
    GitHub,
    /// GitLab CI (`https://gitlab.com` or a self-managed instance)
    GitLab,
    /// A custom OIDC provider with a user-specified claim field.
    Custom {
        /// The JWT claim field that identifies the subject for mapping
        /// (e.g. `"project_path"`, `"repository"`, `"sub"`).
        claim_field: String,
    },
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// OIDC configuration for a single provider instance.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// URL to fetch JWKS from
    /// (e.g. `https://token.actions.githubusercontent.com/.well-known/jwks`)
    pub jwks_url: String,

    /// Expected issuer
    /// (e.g. `https://token.actions.githubusercontent.com`)
    pub issuer: String,

    /// Expected audience
    pub audience: String,

    /// Mapping from the identity claim value to (project_id, Role).
    ///
    /// For GitHub the key is `org/repo`, for GitLab it is `group/project`,
    /// for custom providers it is whatever the configured claim field yields.
    pub repo_mappings: HashMap<String, (String, Role)>,

    /// The provider type driving claim extraction.
    pub provider_type: OidcProviderType,
}

impl OidcConfig {
    /// Creates a GitHub Actions OIDC configuration.
    pub fn github(audience: impl Into<String>) -> Self {
        Self {
            jwks_url: "https://token.actions.githubusercontent.com/.well-known/jwks".to_string(),
            issuer: "https://token.actions.githubusercontent.com".to_string(),
            audience: audience.into(),
            repo_mappings: HashMap::new(),
            provider_type: OidcProviderType::GitHub,
        }
    }

    /// Creates a GitLab CI OIDC configuration for `gitlab.com`.
    pub fn gitlab(audience: impl Into<String>) -> Self {
        Self::gitlab_custom("https://gitlab.com", audience)
    }

    /// Creates a GitLab CI OIDC configuration for a self-managed instance.
    pub fn gitlab_custom(issuer: impl Into<String>, audience: impl Into<String>) -> Self {
        let issuer = issuer.into();
        let jwks_url = format!("{}/-/jwks", issuer.trim_end_matches('/'));
        Self {
            jwks_url,
            issuer,
            audience: audience.into(),
            repo_mappings: HashMap::new(),
            provider_type: OidcProviderType::GitLab,
        }
    }

    /// Creates a custom provider OIDC configuration.
    pub fn custom(
        issuer: impl Into<String>,
        jwks_url: impl Into<String>,
        audience: impl Into<String>,
        claim_field: impl Into<String>,
    ) -> Self {
        Self {
            jwks_url: jwks_url.into(),
            issuer: issuer.into(),
            audience: audience.into(),
            repo_mappings: HashMap::new(),
            provider_type: OidcProviderType::Custom {
                claim_field: claim_field.into(),
            },
        }
    }

    /// Adds a mapping entry.
    pub fn add_mapping(
        mut self,
        identity: impl Into<String>,
        project_id: impl Into<String>,
        role: Role,
    ) -> Self {
        self.repo_mappings
            .insert(identity.into(), (project_id.into(), role));
        self
    }
}

// ---------------------------------------------------------------------------
// Provider runtime
// ---------------------------------------------------------------------------

/// A provider that fetches JWKS and validates tokens from a single issuer.
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
    aud: StringOrVec,
    sub: String,
    repository: String,
    exp: u64,
    iat: Option<u64>,
}

/// GitLab CI OIDC claims.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitLabClaims {
    iss: String,
    aud: StringOrVec,
    sub: String,
    /// Full path of the project (e.g. `mygroup/myproject`).
    project_path: String,
    /// Namespace path (e.g. `mygroup`).
    #[serde(default)]
    namespace_path: Option<String>,
    /// Git ref (e.g. `main`).
    #[serde(rename = "ref")]
    #[serde(default)]
    git_ref: Option<String>,
    /// Pipeline source (e.g. `push`, `web`, `schedule`).
    #[serde(default)]
    pipeline_source: Option<String>,
    exp: u64,
    iat: Option<u64>,
}

/// Generic claims used by custom providers.
/// We deserialize the full payload as a map and then extract the configured claim.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GenericClaims {
    iss: String,
    sub: String,
    exp: u64,
    iat: Option<u64>,
    /// All remaining fields captured for claim extraction.
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

/// Helper: some OIDC providers encode `aud` as a string, others as an array.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrVec {
    Single(String),
    Multiple(Vec<String>),
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
            warn!(
                "Failed initial JWKS fetch from {}: {}",
                provider.config.jwks_url, e
            );
        }

        Ok(provider)
    }

    /// Creates a provider with pre-loaded JWKS (for testing).
    #[cfg(test)]
    pub(crate) fn with_jwks(config: OidcConfig, jwks: JwkSet) -> Self {
        Self {
            config,
            jwks: Arc::new(RwLock::new(Some(jwks))),
            client: Client::new(),
        }
    }

    /// Returns the issuer URL for this provider.
    pub fn issuer(&self) -> &str {
        &self.config.issuer
    }

    /// Returns the provider type.
    pub fn provider_type(&self) -> &OidcProviderType {
        &self.config.provider_type
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

    /// Validates an OIDC token and returns the corresponding mapped [`ApiKey`].
    pub async fn validate_token(&self, token: &str) -> Result<ApiKey, AuthError> {
        let header = decode_header(token).map_err(|e| AuthError::InvalidToken(e.to_string()))?;

        let kid = header
            .kid
            .ok_or_else(|| AuthError::InvalidToken("Missing 'kid' in token header".to_string()))?;

        // Extract decoding key from JWKS cache
        let decoding_key = {
            let cache = self.jwks.read().await;
            let jwks = cache
                .as_ref()
                .ok_or_else(|| AuthError::InvalidToken("JWKS not loaded yet".to_string()))?;

            let jwk = jwks.find(&kid).ok_or_else(|| {
                AuthError::InvalidToken(format!("Key '{}' not found in JWKS", kid))
            })?;

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

        match &self.config.provider_type {
            OidcProviderType::GitHub => self.validate_github(token, &decoding_key, &validation),
            OidcProviderType::GitLab => self.validate_gitlab(token, &decoding_key, &validation),
            OidcProviderType::Custom { claim_field } => {
                let field = claim_field.clone();
                self.validate_custom(token, &decoding_key, &validation, &field)
            }
        }
    }

    fn validate_github(
        &self,
        token: &str,
        key: &DecodingKey,
        validation: &Validation,
    ) -> Result<ApiKey, AuthError> {
        let token_data =
            decode::<GithubClaims>(token, key, validation).map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
                _ => AuthError::InvalidToken(e.to_string()),
            })?;

        let claims = token_data.claims;

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

        Ok(build_api_key(
            &claims.sub,
            &format!("GitHub Actions ({})", claims.repository),
            project_id,
            *role,
            claims.exp,
            claims.iat,
        ))
    }

    fn validate_gitlab(
        &self,
        token: &str,
        key: &DecodingKey,
        validation: &Validation,
    ) -> Result<ApiKey, AuthError> {
        let token_data =
            decode::<GitLabClaims>(token, key, validation).map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
                _ => AuthError::InvalidToken(e.to_string()),
            })?;

        let claims = token_data.claims;

        let (project_id, role) = self
            .config
            .repo_mappings
            .get(&claims.project_path)
            .ok_or_else(|| {
                AuthError::InvalidToken(format!(
                    "Project '{}' is not authorized",
                    claims.project_path
                ))
            })?;

        Ok(build_api_key(
            &claims.sub,
            &format!("GitLab CI ({})", claims.project_path),
            project_id,
            *role,
            claims.exp,
            claims.iat,
        ))
    }

    fn validate_custom(
        &self,
        token: &str,
        key: &DecodingKey,
        validation: &Validation,
        claim_field: &str,
    ) -> Result<ApiKey, AuthError> {
        let token_data =
            decode::<GenericClaims>(token, key, validation).map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => AuthError::ExpiredToken,
                _ => AuthError::InvalidToken(e.to_string()),
            })?;

        let claims = token_data.claims;

        // Extract the mapping key from the configured claim field.
        // First check `sub` (standard claim), then look in `extra`.
        let identity = if claim_field == "sub" {
            claims.sub.clone()
        } else {
            claims
                .extra
                .get(claim_field)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    AuthError::InvalidToken(format!("Claim '{}' not found in token", claim_field))
                })?
        };

        let (project_id, role) = self.config.repo_mappings.get(&identity).ok_or_else(|| {
            AuthError::InvalidToken(format!(
                "Identity '{}' (from claim '{}') is not authorized",
                identity, claim_field
            ))
        })?;

        let provider_name = self
            .config
            .issuer
            .split("//")
            .nth(1)
            .unwrap_or(&self.config.issuer);

        Ok(build_api_key(
            &claims.sub,
            &format!("OIDC {} ({})", provider_name, identity),
            project_id,
            *role,
            claims.exp,
            claims.iat,
        ))
    }
}

// ---------------------------------------------------------------------------
// Multi-provider registry
// ---------------------------------------------------------------------------

/// A registry of multiple [`OidcProvider`]s that tries each one in turn.
#[derive(Clone, Default)]
pub struct OidcRegistry {
    providers: Vec<OidcProvider>,
}

impl OidcRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Adds a provider to the registry.
    pub fn add(&mut self, provider: OidcProvider) {
        self.providers.push(provider);
    }

    /// Returns `true` when at least one provider is registered.
    pub fn has_providers(&self) -> bool {
        !self.providers.is_empty()
    }

    /// Validates a token against all registered providers, returning the first
    /// successful result. If no provider succeeds, returns the last error.
    pub async fn validate_token(&self, token: &str) -> Result<ApiKey, AuthError> {
        let mut last_err = AuthError::InvalidToken("No OIDC providers configured".to_string());

        for provider in &self.providers {
            match provider.validate_token(token).await {
                Ok(api_key) => return Ok(api_key),
                Err(e) => {
                    debug!(
                        issuer = %provider.issuer(),
                        error = %e,
                        "OIDC provider did not accept token"
                    );
                    last_err = e;
                }
            }
        }

        Err(last_err)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_api_key(
    sub: &str,
    name: &str,
    project_id: &str,
    role: Role,
    exp: u64,
    iat: Option<u64>,
) -> ApiKey {
    let expires_at = chrono::DateTime::<chrono::Utc>::from_timestamp(exp as i64, 0)
        .unwrap_or_else(chrono::Utc::now);

    let created_at = iat
        .and_then(|iat| chrono::DateTime::<chrono::Utc>::from_timestamp(iat as i64, 0))
        .unwrap_or_else(chrono::Utc::now);

    ApiKey {
        id: format!("oidc:{}", sub),
        name: name.to_string(),
        project_id: project_id.to_string(),
        scopes: role.allowed_scopes(),
        role,
        benchmark_regex: None,
        expires_at: Some(expires_at),
        created_at,
        last_used_at: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::jwk::{
        AlgorithmParameters, CommonParameters, Jwk, KeyAlgorithm, PublicKeyUse, RSAKeyParameters,
    };
    use jsonwebtoken::{EncodingKey, Header, encode};

    // A pre-generated 2048-bit RSA PKCS#8 private key for tests.
    const TEST_RSA_PRIVATE_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQDZybnf96nsNBgF
xV6bk/KoV1uJopbXHX4VYeYgp7llS8WpvkKjFzyoWowsmkhdlh934lI1cHEJPtdl
UlczdEgkhyro8aKRO1f6cdg8csH9Vj+Zyf1gavGDpHXLOfsjykLDpDfpb2GZ+6pr
uGuattFYF/vWDrwUx4lWRwCfRrCL4gW3A2i+uhUT/weJ5bvzOe/mXlF1VAw6+Bxb
FjjsBoupkMk9JXEQRl5/yMksrVIN2E8Wd7K7mcscUuXV42gBiQ2EJGC3Xz7jqzlx
LAb4EI+EeUXhEo5EA2mS897jzGU3QDxMOw8RdGgQLqpVx2zUozsYvYdcunf5yZhp
Hrg+XtBNAgMBAAECggEAAbTiU7cNBEZcl0hwvRLPn+DNLrxIPiCPIHXEYiZllWxB
lCwdOWFlgaYJFYXYVmnyGhcGvJ1flWGZf8PYxuZZ6UddgkJUpskNcSmYfKL02DGh
pFFRsw39qNv3JQ+I+oiLe2L7Z7mCtdO7HVI/0ISjfmd/hrHKUYHpUMYIYx/alza7
fFfBLxgjqwIM5wYL8WOrM7E6axsA7eFjj5Uad2nhgpRImTG0oLR49R0ldN8lYKZ6
4cbD27JS7vw716PgHGT2S+JTh3+6dFyty/DkL6S3+pUVPbQUbhCQLQMVzf5QhI0J
fDbdWXzF3fgBk9+uXHLIv9Spa9h+1/dv/2v8UMcMdQKBgQD9PivmY/Tcu8Sz3Rij
blJ8b41jcmzlgx3bLh55b//3iY/GBMT67rRqOGFEEoC+EGrfQ72voWYr+XFb/qf7
DoX6jfncGL1LSCDaS5BCc9Ekf9VX0Q56otF/12mpu8g5aYBDhJSd8JUcFtck+Dxz
1Y6dTwtGG0720NTRE7Xy/N+wSwKBgQDcKLx2vOdIFkuAaXv+YUvxZ0jJDAcscNEm
/wwGpcV+u3TBlqrfEEymfkca9YdoFoOO9u4g32EIe1k3ya/VxUuZxJhjiMBtpgh1
CymYHEp7i4U/Sa5zRMulmuq8NZj3ZJANi8rSqHJ+UJiv+ofRu2Tdve9xuBpzMskz
ZV6RqpaSxwKBgDc71itb5c43DgIE2RjcORV25ymnjWTJojtp5a+q4/NDh54y8Bui
8KqyPVSxjG7n+cdUaQzjcPtqXnUoJ880LbimOrbslmzTAIdcL8yuohEJ6KhMqpHI
7VSq0Rr6IAOVpSoUwq1oCb2kpawkkFrbW02oLddOoXxns+MeH3MuAEPdAoGAVlIi
kuu+QyV6tP6m/zZm8F/uyeVNar9RQlj9/h1BMk+Nl9nbZVqesykP+CIM1WL+ci+f
boQnJ4w1jwolR0v0OHY8ycn0qQlQh5O420s8aPRrakUZgViYAHadUu4w688iLC2D
eNVTDvPK6jTwy+sNwWOXXp8wv7pJ6Tz1t2eLYkECgYB4tKuGpV4i2f2Ve5BNfAwQ
Pct5tSWlUCbHRgaZ3hno6pR/WVs4HP6LmaA1pdwLL+3qG84OP3ARUzELEGRnNVT5
+/xobF/tDl7gdKvRSFhOF08mxg7evm5yRt+GGkX1+SA3St3queXDAVG6NtrKju5j
ggQxRhTX+ObL3zkIJahzUA==
-----END PRIVATE KEY-----";

    // The corresponding public key modulus (base64url, no padding).
    const TEST_RSA_N: &str = "2cm53_ep7DQYBcVem5PyqFdbiaKW1x1-FWHmIKe5ZUvFqb5Coxc8qFqMLJpIXZYfd-JSNXBxCT7XZVJXM3RIJIcq6PGikTtX-nHYPHLB_VY_mcn9YGrxg6R1yzn7I8pCw6Q36W9hmfuqa7hrmrbRWBf71g68FMeJVkcAn0awi-IFtwNovroVE_8HieW78znv5l5RdVQMOvgcWxY47AaLqZDJPSVxEEZef8jJLK1SDdhPFneyu5nLHFLl1eNoAYkNhCRgt18-46s5cSwG-BCPhHlF4RKORANpkvPe48xlN0A8TDsPEXRoEC6qVcds1KM7GL2HXLp3-cmYaR64Pl7QTQ";

    // Public exponent (65537 in base64url).
    const TEST_RSA_E: &str = "AQAB";

    /// Helper: return (encoding_key, jwk_set) for test token signing and verification.
    fn test_rsa_keys(kid: &str) -> (EncodingKey, JwkSet) {
        let encoding_key = EncodingKey::from_rsa_pem(TEST_RSA_PRIVATE_PEM).unwrap();

        let jwk = Jwk {
            common: CommonParameters {
                public_key_use: Some(PublicKeyUse::Signature),
                key_operations: None,
                key_algorithm: Some(KeyAlgorithm::RS256),
                key_id: Some(kid.to_string()),
                x509_url: None,
                x509_chain: None,
                x509_sha1_fingerprint: None,
                x509_sha256_fingerprint: None,
            },
            algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
                key_type: Default::default(),
                n: TEST_RSA_N.to_string(),
                e: TEST_RSA_E.to_string(),
            }),
        };

        let jwks = JwkSet { keys: vec![jwk] };

        (encoding_key, jwks)
    }

    fn encode_github_token(encoding_key: &EncodingKey, kid: &str, repo: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());

        let now = chrono::Utc::now().timestamp() as u64;
        let claims = serde_json::json!({
            "iss": "https://token.actions.githubusercontent.com",
            "aud": "perfgate",
            "sub": "repo:org/repo:ref:refs/heads/main",
            "repository": repo,
            "exp": now + 300,
            "iat": now,
        });

        encode(&header, &claims, encoding_key).unwrap()
    }

    fn encode_gitlab_token(
        encoding_key: &EncodingKey,
        kid: &str,
        project_path: &str,
        issuer: &str,
    ) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());

        let now = chrono::Utc::now().timestamp() as u64;
        let claims = serde_json::json!({
            "iss": issuer,
            "aud": "perfgate",
            "sub": format!("project_path:{}:ref_type:branch:ref:main", project_path),
            "project_path": project_path,
            "namespace_path": project_path.split('/').next().unwrap_or(""),
            "ref": "main",
            "pipeline_source": "push",
            "exp": now + 300,
            "iat": now,
        });

        encode(&header, &claims, encoding_key).unwrap()
    }

    fn encode_custom_token(
        encoding_key: &EncodingKey,
        kid: &str,
        issuer: &str,
        claim_field: &str,
        claim_value: &str,
    ) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());

        let now = chrono::Utc::now().timestamp() as u64;
        let claims = serde_json::json!({
            "iss": issuer,
            "aud": "perfgate",
            "sub": format!("custom:{}", claim_value),
            claim_field: claim_value,
            "exp": now + 300,
            "iat": now,
        });

        encode(&header, &claims, encoding_key).unwrap()
    }

    // -----------------------------------------------------------------------
    // GitHub provider tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_github_oidc_valid_token() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let config = OidcConfig::github("perfgate").add_mapping(
            "EffortlessMetrics/perfgate",
            "perfgate-oss",
            Role::Contributor,
        );

        let provider = OidcProvider::with_jwks(config, jwks);
        let token = encode_github_token(&enc, kid, "EffortlessMetrics/perfgate");

        let api_key = provider.validate_token(&token).await.unwrap();
        assert_eq!(api_key.project_id, "perfgate-oss");
        assert_eq!(api_key.role, Role::Contributor);
        assert!(api_key.id.starts_with("oidc:"));
        assert!(api_key.name.contains("GitHub Actions"));
        assert!(api_key.name.contains("EffortlessMetrics/perfgate"));
    }

    #[tokio::test]
    async fn test_github_oidc_unauthorized_repo() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let config = OidcConfig::github("perfgate").add_mapping(
            "EffortlessMetrics/perfgate",
            "perfgate-oss",
            Role::Contributor,
        );

        let provider = OidcProvider::with_jwks(config, jwks);
        let token = encode_github_token(&enc, kid, "evil-org/evil-repo");

        let err = provider.validate_token(&token).await.unwrap_err();
        match err {
            AuthError::InvalidToken(msg) => {
                assert!(msg.contains("not authorized"), "got: {}", msg);
            }
            other => panic!("Expected InvalidToken, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_github_oidc_unknown_kid() {
        let kid = "test-key-1";
        let (enc, _jwks) = test_rsa_keys(kid);

        let config = OidcConfig::github("perfgate").add_mapping("org/repo", "proj", Role::Viewer);

        // Use a different kid to create the JWKS
        let (_enc2, other_jwks) = test_rsa_keys("different-key");
        let provider = OidcProvider::with_jwks(config, other_jwks);
        let token = encode_github_token(&enc, kid, "org/repo");

        let err = provider.validate_token(&token).await.unwrap_err();
        match err {
            AuthError::InvalidToken(msg) => {
                assert!(msg.contains("not found in JWKS"), "got: {}", msg);
            }
            other => panic!("Expected InvalidToken, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // GitLab provider tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_gitlab_oidc_valid_token() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let config = OidcConfig::gitlab("perfgate").add_mapping(
            "mygroup/myproject",
            "myproject-prod",
            Role::Promoter,
        );

        let provider = OidcProvider::with_jwks(config, jwks);
        let token = encode_gitlab_token(&enc, kid, "mygroup/myproject", "https://gitlab.com");

        let api_key = provider.validate_token(&token).await.unwrap();
        assert_eq!(api_key.project_id, "myproject-prod");
        assert_eq!(api_key.role, Role::Promoter);
        assert!(api_key.id.starts_with("oidc:"));
        assert!(api_key.name.contains("GitLab CI"));
        assert!(api_key.name.contains("mygroup/myproject"));
    }

    #[tokio::test]
    async fn test_gitlab_oidc_self_managed() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let issuer = "https://gitlab.example.com";
        let config = OidcConfig::gitlab_custom(issuer, "perfgate").add_mapping(
            "team/repo",
            "internal-proj",
            Role::Admin,
        );

        assert_eq!(config.jwks_url, "https://gitlab.example.com/-/jwks");
        assert_eq!(config.issuer, issuer);

        let provider = OidcProvider::with_jwks(config, jwks);
        let token = encode_gitlab_token(&enc, kid, "team/repo", issuer);

        let api_key = provider.validate_token(&token).await.unwrap();
        assert_eq!(api_key.project_id, "internal-proj");
        assert_eq!(api_key.role, Role::Admin);
    }

    #[tokio::test]
    async fn test_gitlab_oidc_unauthorized_project() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let config =
            OidcConfig::gitlab("perfgate").add_mapping("mygroup/myproject", "proj", Role::Viewer);

        let provider = OidcProvider::with_jwks(config, jwks);
        let token = encode_gitlab_token(&enc, kid, "evil/project", "https://gitlab.com");

        let err = provider.validate_token(&token).await.unwrap_err();
        match err {
            AuthError::InvalidToken(msg) => {
                assert!(msg.contains("not authorized"), "got: {}", msg);
            }
            other => panic!("Expected InvalidToken, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Custom provider tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_custom_oidc_valid_token() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let config = OidcConfig::custom(
            "https://auth.example.com",
            "https://auth.example.com/.well-known/jwks.json",
            "perfgate",
            "team_slug",
        )
        .add_mapping("platform-team", "platform-proj", Role::Contributor);

        let provider = OidcProvider::with_jwks(config, jwks);
        let token = encode_custom_token(
            &enc,
            kid,
            "https://auth.example.com",
            "team_slug",
            "platform-team",
        );

        let api_key = provider.validate_token(&token).await.unwrap();
        assert_eq!(api_key.project_id, "platform-proj");
        assert_eq!(api_key.role, Role::Contributor);
        assert!(api_key.name.contains("auth.example.com"));
    }

    #[tokio::test]
    async fn test_custom_oidc_missing_claim() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let config = OidcConfig::custom(
            "https://auth.example.com",
            "https://auth.example.com/.well-known/jwks.json",
            "perfgate",
            "nonexistent_claim",
        )
        .add_mapping("val", "proj", Role::Viewer);

        let provider = OidcProvider::with_jwks(config, jwks);
        // The token has "team_slug" but not "nonexistent_claim"
        let token = encode_custom_token(&enc, kid, "https://auth.example.com", "team_slug", "val");

        let err = provider.validate_token(&token).await.unwrap_err();
        match err {
            AuthError::InvalidToken(msg) => {
                assert!(msg.contains("not found in token"), "got: {}", msg);
            }
            other => panic!("Expected InvalidToken, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_custom_oidc_sub_claim() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        let config = OidcConfig::custom(
            "https://auth.example.com",
            "https://auth.example.com/.well-known/jwks.json",
            "perfgate",
            "sub",
        )
        .add_mapping("custom:my-identity", "sub-proj", Role::Viewer);

        let provider = OidcProvider::with_jwks(config, jwks);
        let token = encode_custom_token(
            &enc,
            kid,
            "https://auth.example.com",
            "team_slug",
            "my-identity",
        );

        let api_key = provider.validate_token(&token).await.unwrap();
        assert_eq!(api_key.project_id, "sub-proj");
        assert_eq!(api_key.role, Role::Viewer);
    }

    // -----------------------------------------------------------------------
    // Registry tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_registry_tries_all_providers() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        // Provider 1: GitHub (won't match a GitLab token)
        let github_config =
            OidcConfig::github("perfgate").add_mapping("org/repo", "gh-proj", Role::Viewer);

        // Provider 2: GitLab (will match)
        let gitlab_config = OidcConfig::gitlab("perfgate").add_mapping(
            "mygroup/myproject",
            "gl-proj",
            Role::Contributor,
        );

        let mut registry = OidcRegistry::new();
        registry.add(OidcProvider::with_jwks(github_config, jwks.clone()));
        registry.add(OidcProvider::with_jwks(gitlab_config, jwks));

        // A GitLab-style token should be picked up by the second provider
        let token = encode_gitlab_token(&enc, kid, "mygroup/myproject", "https://gitlab.com");

        let api_key = registry.validate_token(&token).await.unwrap();
        assert_eq!(api_key.project_id, "gl-proj");
        assert_eq!(api_key.role, Role::Contributor);
    }

    #[tokio::test]
    async fn test_registry_no_providers() {
        let registry = OidcRegistry::new();
        assert!(!registry.has_providers());

        let err = registry.validate_token("some.jwt.token").await.unwrap_err();
        match err {
            AuthError::InvalidToken(msg) => {
                assert!(msg.contains("No OIDC providers"), "got: {}", msg);
            }
            other => panic!("Expected InvalidToken, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_registry_returns_last_error_when_all_fail() {
        let kid = "test-key-1";
        let (enc, jwks) = test_rsa_keys(kid);

        // Both providers will reject because the repo/project doesn't match
        let github_config =
            OidcConfig::github("perfgate").add_mapping("org/repo", "proj", Role::Viewer);
        let gitlab_config =
            OidcConfig::gitlab("perfgate").add_mapping("group/project", "proj", Role::Viewer);

        let mut registry = OidcRegistry::new();
        registry.add(OidcProvider::with_jwks(github_config, jwks.clone()));
        registry.add(OidcProvider::with_jwks(gitlab_config, jwks));

        let token = encode_github_token(&enc, kid, "unknown/repo");

        let err = registry.validate_token(&token).await.unwrap_err();
        assert!(matches!(err, AuthError::InvalidToken(_)));
    }

    // -----------------------------------------------------------------------
    // Config builder tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_github_config_defaults() {
        let config = OidcConfig::github("perfgate");
        assert_eq!(
            config.jwks_url,
            "https://token.actions.githubusercontent.com/.well-known/jwks"
        );
        assert_eq!(config.issuer, "https://token.actions.githubusercontent.com");
        assert_eq!(config.audience, "perfgate");
        assert_eq!(config.provider_type, OidcProviderType::GitHub);
        assert!(config.repo_mappings.is_empty());
    }

    #[test]
    fn test_gitlab_config_defaults() {
        let config = OidcConfig::gitlab("perfgate");
        assert_eq!(config.jwks_url, "https://gitlab.com/-/jwks");
        assert_eq!(config.issuer, "https://gitlab.com");
        assert_eq!(config.audience, "perfgate");
        assert_eq!(config.provider_type, OidcProviderType::GitLab);
    }

    #[test]
    fn test_gitlab_custom_config_trailing_slash() {
        let config = OidcConfig::gitlab_custom("https://gitlab.example.com/", "perfgate");
        assert_eq!(config.jwks_url, "https://gitlab.example.com/-/jwks");
        assert_eq!(config.issuer, "https://gitlab.example.com/");
    }

    #[test]
    fn test_custom_config() {
        let config = OidcConfig::custom(
            "https://auth.example.com",
            "https://auth.example.com/jwks",
            "my-audience",
            "org_id",
        );
        assert_eq!(config.issuer, "https://auth.example.com");
        assert_eq!(config.jwks_url, "https://auth.example.com/jwks");
        assert_eq!(config.audience, "my-audience");
        assert_eq!(
            config.provider_type,
            OidcProviderType::Custom {
                claim_field: "org_id".to_string()
            }
        );
    }

    #[test]
    fn test_config_add_mapping() {
        let config = OidcConfig::github("aud")
            .add_mapping("org/repo1", "proj1", Role::Viewer)
            .add_mapping("org/repo2", "proj2", Role::Admin);

        assert_eq!(config.repo_mappings.len(), 2);
        assert_eq!(
            config.repo_mappings.get("org/repo1"),
            Some(&("proj1".to_string(), Role::Viewer))
        );
        assert_eq!(
            config.repo_mappings.get("org/repo2"),
            Some(&("proj2".to_string(), Role::Admin))
        );
    }
}
