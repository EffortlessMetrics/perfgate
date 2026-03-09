//! Error types for the perfgate server.

use thiserror::Error;

/// Storage-related errors.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Baseline already exists
    #[error("Baseline already exists: {0}")]
    AlreadyExists(String),

    /// Baseline not found
    #[error("Baseline not found: {0}")]
    NotFound(String),

    /// Database connection error
    #[error("Database connection error: {0}")]
    ConnectionError(String),

    /// Query execution error
    #[error("Query error: {0}")]
    QueryError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    /// IO error
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    /// SQLite error
    #[error("SQLite error: {0}")]
    SqliteError(#[from] rusqlite::Error),

    /// Lock error
    #[error("Lock error: {0}")]
    LockError(String),

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl StoreError {
    /// Creates a new "already exists" error.
    pub fn already_exists(project: &str, benchmark: &str, version: &str) -> Self {
        Self::AlreadyExists(format!(
            "project={}, benchmark={}, version={}",
            project, benchmark, version
        ))
    }

    /// Creates a new "not found" error.
    pub fn not_found(project: &str, benchmark: &str, version: &str) -> Self {
        Self::NotFound(format!(
            "project={}, benchmark={}, version={}",
            project, benchmark, version
        ))
    }
}

/// Authentication-related errors.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Missing authentication header
    #[error("Missing authentication header")]
    MissingAuth,

    /// Invalid API key format
    #[error("Invalid API key format")]
    InvalidKeyFormat,

    /// Invalid API key
    #[error("Invalid API key")]
    InvalidKey,

    /// Expired API key
    #[error("API key has expired")]
    ExpiredKey,

    /// Invalid JWT token
    #[error("Invalid JWT token: {0}")]
    InvalidToken(String),

    /// Expired JWT token
    #[error("JWT token has expired")]
    ExpiredToken,

    /// Insufficient permissions
    #[error("Insufficient permissions: required {required}, has {actual}")]
    InsufficientPermissions {
        required: String,
        actual: String,
    },
}

/// Server configuration errors.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Invalid configuration value
    #[error("Invalid configuration: {0}")]
    InvalidValue(String),

    /// Missing required configuration
    #[error("Missing required configuration: {0}")]
    MissingRequired(String),

    /// File I/O error
    #[error("Configuration file error: {0}")]
    FileError(#[from] std::io::Error),

    /// Parse error
    #[error("Configuration parse error: {0}")]
    ParseError(String),
}
