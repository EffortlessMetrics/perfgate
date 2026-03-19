//! Authentication and authorization types for perfgate.

use chrono::{DateTime, Utc};
use perfgate_error::AuthError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// API key prefix for live keys.
pub const API_KEY_PREFIX_LIVE: &str = "pg_live_";

/// API key prefix for test keys.
pub const API_KEY_PREFIX_TEST: &str = "pg_test_";

/// Permission scope for API operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// Represents an authenticated API key.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
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

    /// Expiration timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last usage timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
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
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    /// Checks if the key has expired.
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires_at {
            return exp < Utc::now();
        }
        false
    }

    /// Checks if the key has a specific scope.
    pub fn has_scope(&self, scope: Scope) -> bool {
        self.scopes.contains(&scope)
    }
}

/// JWT claims accepted by the server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
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
    fn test_generate_api_key() {
        let live_key = generate_api_key(false);
        assert!(live_key.starts_with(API_KEY_PREFIX_LIVE));
        assert!(live_key.len() >= 40);

        let test_key = generate_api_key(true);
        assert!(test_key.starts_with(API_KEY_PREFIX_TEST));
        assert!(test_key.len() >= 40);
    }
}
