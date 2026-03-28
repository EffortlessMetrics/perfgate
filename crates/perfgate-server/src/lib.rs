//! REST API server for centralized baseline management.
//!
//! This crate provides a REST API server for storing and managing performance
//! baselines. It supports multiple storage backends (in-memory, SQLite, PostgreSQL)
//! and includes authentication via API keys.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Features
//!
//! - **Multi-tenancy**: Projects/namespaces for isolation
//! - **Version history**: Track baseline versions over time
//! - **Rich metadata**: Git refs, tags, custom metadata
//! - **Access control**: Role-based permissions (Viewer, Contributor, Promoter, Admin)
//! - **Multiple backends**: In-memory, SQLite, PostgreSQL (planned)
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use perfgate_server::{ServerConfig, StorageBackend, run_server};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ServerConfig::new()
//!         .bind("0.0.0.0:8080").unwrap()
//!         .storage_backend(StorageBackend::Sqlite)
//!         .sqlite_path("perfgate.db");
//!
//!     run_server(config).await.unwrap();
//! }
//! ```
//!
//! # API Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | POST | `/projects/{project}/baselines` | Upload a baseline |
//! | GET | `/projects/{project}/baselines/{benchmark}/latest` | Get latest baseline |
//! | GET | `/projects/{project}/baselines/{benchmark}/versions/{version}` | Get specific version |
//! | GET | `/projects/{project}/baselines` | List baselines |
//! | DELETE | `/projects/{project}/baselines/{benchmark}/versions/{version}` | Delete baseline |
//! | POST | `/projects/{project}/baselines/{benchmark}/promote` | Promote version |
//! | GET | `/audit` | List audit events (admin only) |
//! | GET | `/health` | Health check |
//! | GET | `/metrics` | Prometheus metrics |

pub mod auth;
pub mod cleanup;
pub mod error;
pub mod handlers;
pub mod metrics;
pub mod models;
pub mod oidc;
pub mod server;
pub mod storage;

#[cfg(any(test, feature = "test-utils"))]
pub mod testing;

pub use auth::{ApiKey, ApiKeyStore, AuthContext, AuthState, JwtClaims, JwtConfig, Role, Scope};
pub use error::{AuthError, ConfigError, StoreError};
pub use models::*;
pub use oidc::{OidcConfig, OidcProvider, OidcProviderType, OidcRegistry};
pub use server::{
    ApiKeyMetadata, AppState, PostgresPoolConfig, ServerConfig, StorageBackend, run_server,
};
pub use storage::{
    AuditStore, BaselineStore, FleetStore, InMemoryFleetStore, InMemoryKeyStore, InMemoryStore,
    KeyRecord, KeyStore, SqliteKeyStore, SqliteStore, StorageHealth,
};

/// Server version string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }
}
