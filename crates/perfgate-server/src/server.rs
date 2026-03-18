//! Server configuration and bootstrap.
//!
//! This module provides the [`ServerConfig`] and [`run_server`] function
//! for starting the HTTP server.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Router, middleware,
    routing::{delete, get, post},
};
use tower::ServiceBuilder;
use tower_http::{
    cors::{Any, CorsLayer},
    request_id::MakeRequestUuid,
    trace::TraceLayer,
};
use tracing::info;

use crate::auth::{ApiKey, ApiKeyStore, AuthState, JwtConfig, Role, auth_middleware};
use crate::error::ConfigError;
use crate::handlers::{
    delete_baseline, get_baseline, get_latest_baseline, health_check, list_baselines,
    promote_baseline, upload_baseline,
};
use crate::storage::{
    ArtifactStore, BaselineStore, InMemoryStore, ObjectArtifactStore, PostgresStore, SqliteStore,
};

/// Storage backend type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageBackend {
    /// In-memory storage (for testing/development)
    #[default]
    Memory,
    /// SQLite persistent storage
    Sqlite,
    /// PostgreSQL storage (not yet implemented)
    Postgres,
}

impl std::str::FromStr for StorageBackend {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "memory" => Ok(Self::Memory),
            "sqlite" => Ok(Self::Sqlite),
            "postgres" | "postgresql" => Ok(Self::Postgres),
            _ => Err(format!("Unknown storage backend: {}", s)),
        }
    }
}

/// Server configuration.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Bind address (e.g., "0.0.0.0:8080")
    pub bind: SocketAddr,

    /// Storage backend type
    pub storage_backend: StorageBackend,

    /// SQLite database path (when storage_backend is Sqlite)
    pub sqlite_path: Option<PathBuf>,

    /// PostgreSQL connection URL (when storage_backend is Postgres)
    pub postgres_url: Option<String>,

    /// Artifact storage URL (e.g., s3://bucket/prefix)
    pub artifacts_url: Option<String>,

    /// API keys for authentication (key -> role mapping)
    pub api_keys: Vec<(String, Role)>,

    /// Optional JWT validation settings.
    pub jwt: Option<JwtConfig>,

    /// Enable CORS for all origins
    pub cors: bool,

    /// Request timeout in seconds
    pub timeout_seconds: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".parse().unwrap(),
            storage_backend: StorageBackend::Memory,
            sqlite_path: None,
            postgres_url: None,
            artifacts_url: None,
            api_keys: vec![],
            jwt: None,
            cors: true,
            timeout_seconds: 30,
        }
    }
}

impl ServerConfig {
    /// Creates a new configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the bind address.
    pub fn bind(mut self, addr: impl Into<String>) -> Result<Self, ConfigError> {
        self.bind = addr
            .into()
            .parse()
            .map_err(|e| ConfigError::InvalidValue(format!("Invalid bind address: {}", e)))?;
        Ok(self)
    }

    /// Sets the storage backend.
    pub fn storage_backend(mut self, backend: StorageBackend) -> Self {
        self.storage_backend = backend;
        self
    }

    /// Sets the SQLite database path.
    pub fn sqlite_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.sqlite_path = Some(path.into());
        self
    }

    /// Sets the PostgreSQL connection URL.
    pub fn postgres_url(mut self, url: impl Into<String>) -> Self {
        self.postgres_url = Some(url.into());
        self
    }

    /// Sets the artifacts storage URL.
    pub fn artifacts_url(mut self, url: impl Into<String>) -> Self {
        self.artifacts_url = Some(url.into());
        self
    }

    /// Adds an API key with a specific role.
    pub fn api_key(mut self, key: impl Into<String>, role: Role) -> Self {
        self.api_keys.push((key.into(), role));
        self
    }

    /// Enables JWT token authentication.
    pub fn jwt(mut self, jwt: JwtConfig) -> Self {
        self.jwt = Some(jwt);
        self
    }

    /// Enables or disables CORS.
    pub fn cors(mut self, enabled: bool) -> Self {
        self.cors = enabled;
        self
    }
}

/// Creates the artifact storage based on configuration.
pub(crate) async fn create_artifacts(
    config: &ServerConfig,
) -> Result<Option<Arc<dyn ArtifactStore>>, ConfigError> {
    if let Some(url) = &config.artifacts_url {
        info!(url = %url, "Using object storage for artifacts");
        let (store, _path) = object_store::parse_url(&url.parse().map_err(|e| {
            ConfigError::InvalidValue(format!("Invalid artifacts URL: {}", e))
        })?)
        .map_err(|e| ConfigError::InvalidValue(format!("Failed to parse artifacts URL: {}", e)))?;

        Ok(Some(Arc::new(ObjectArtifactStore::new(Arc::from(store)))))
    } else {
        Ok(None)
    }
}

/// Creates the storage backend based on configuration.
pub(crate) async fn create_storage(
    config: &ServerConfig,
) -> Result<Arc<dyn BaselineStore>, ConfigError> {
    let artifacts = create_artifacts(config).await?;

    match config.storage_backend {
        StorageBackend::Memory => {
            info!("Using in-memory storage");
            Ok(Arc::new(InMemoryStore::new()))
        }
        StorageBackend::Sqlite => {
            let path = config
                .sqlite_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("perfgate.db"));
            info!(path = %path.display(), "Using SQLite storage");
            let store = SqliteStore::new(&path, artifacts)
                .map_err(|e| ConfigError::InvalidValue(format!("Failed to open SQLite: {}", e)))?;
            Ok(Arc::new(store))
        }
        StorageBackend::Postgres => {
            let url = config
                .postgres_url
                .clone()
                .unwrap_or_else(|| "postgres://localhost:5432/perfgate".to_string());
            info!(url = %url, "Using PostgreSQL storage");
            let store = PostgresStore::new(&url, artifacts).await
                .map_err(|e| ConfigError::InvalidValue(format!("Failed to connect to Postgres: {}", e)))?;
            Ok(Arc::new(store))
        }
    }
}

/// Creates the API key store from configuration.
pub(crate) async fn create_key_store(
    config: &ServerConfig,
) -> Result<Arc<ApiKeyStore>, ConfigError> {
    let store = ApiKeyStore::new();

    // Add configured API keys
    for (key, role) in &config.api_keys {
        let api_key = ApiKey::new(
            uuid::Uuid::new_v4().to_string(),
            format!("{:?} key", role),
            "default".to_string(),
            *role,
        );
        store.add_key(api_key, key).await;
        info!(role = ?role, "Added API key");
    }

    Ok(Arc::new(store))
}

/// Creates the router with all routes configured.
pub(crate) fn create_router(
    store: Arc<dyn BaselineStore>,
    auth_state: AuthState,
    config: &ServerConfig,
) -> Router {
    // Health check (no auth required)
    let health_routes = Router::new().route("/health", get(health_check));

    // API routes that require authentication
    let api_routes = Router::new()
        // Baseline CRUD
        .route("/projects/{project}/baselines", post(upload_baseline))
        .route(
            "/projects/{project}/baselines/{benchmark}/latest",
            get(get_latest_baseline),
        )
        .route(
            "/projects/{project}/baselines/{benchmark}/versions/{version}",
            get(get_baseline),
        )
        .route(
            "/projects/{project}/baselines/{benchmark}/versions/{version}",
            delete(delete_baseline),
        )
        .route("/projects/{project}/baselines", get(list_baselines))
        .route(
            "/projects/{project}/baselines/{benchmark}/promote",
            post(promote_baseline),
        )
        .layer(middleware::from_fn_with_state(auth_state, auth_middleware));

    // Combine routes under /api/v1, plus root /health
    let mut app = Router::new()
        .merge(health_routes.clone())
        .nest("/api/v1", health_routes.merge(api_routes));

    // Add CORS if enabled
    if config.cors {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    }

    app.with_state(store)
}

/// Runs the HTTP server.
///
/// This function starts the server and blocks until shutdown.
pub async fn run_server(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        bind = %config.bind,
        backend = ?config.storage_backend,
        "Starting perfgate server"
    );

    // Create storage
    let store = create_storage(&config).await?;

    // Create key store
    let key_store = create_key_store(&config).await?;
    let auth_state = AuthState::new(key_store, config.jwt.clone());

    // Create router
    let app = create_router(store.clone(), auth_state, &config);

    // Add tracing and request ID layers
    let app = app.layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(tower_http::request_id::SetRequestIdLayer::x_request_id(
                MakeRequestUuid,
            )),
    );

    // Create listener
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    info!(addr = %config.bind, "Server listening");

    // Run server with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Server shutdown complete");
    Ok(())
}

/// Creates a shutdown signal handler.
async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_config_default() {
        let config = ServerConfig::new();
        assert_eq!(config.bind.to_string(), "0.0.0.0:8080");
        assert_eq!(config.storage_backend, StorageBackend::Memory);
    }

    #[test]
    fn test_server_config_builder() {
        let config = ServerConfig::new()
            .bind("127.0.0.1:3000")
            .unwrap()
            .storage_backend(StorageBackend::Sqlite)
            .sqlite_path("/tmp/test.db")
            .api_key("test-key", Role::Admin)
            .jwt(JwtConfig::hs256(b"test-secret".to_vec()).issuer("perfgate"))
            .cors(false);

        assert_eq!(config.bind.to_string(), "127.0.0.1:3000");
        assert_eq!(config.storage_backend, StorageBackend::Sqlite);
        assert_eq!(config.sqlite_path, Some(PathBuf::from("/tmp/test.db")));
        assert_eq!(config.api_keys.len(), 1);
        assert!(config.jwt.is_some());
        assert!(!config.cors);
    }

    #[test]
    fn test_storage_backend_from_str() {
        assert_eq!(
            "memory".parse::<StorageBackend>().unwrap(),
            StorageBackend::Memory
        );
        assert_eq!(
            "sqlite".parse::<StorageBackend>().unwrap(),
            StorageBackend::Sqlite
        );
        assert_eq!(
            "postgres".parse::<StorageBackend>().unwrap(),
            StorageBackend::Postgres
        );
        assert!("invalid".parse::<StorageBackend>().is_err());
    }

    #[tokio::test]
    async fn test_create_storage_memory() {
        let config = ServerConfig::new().storage_backend(StorageBackend::Memory);
        let storage = create_storage(&config).await.unwrap();
        assert_eq!(storage.backend_type(), "memory");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_storage_sqlite() {
        let config = ServerConfig::new()
            .storage_backend(StorageBackend::Sqlite)
            .sqlite_path(":memory:");
        let storage = create_storage(&config).await.unwrap();
        assert_eq!(storage.backend_type(), "sqlite");
    }

    #[tokio::test]
    async fn test_create_storage_postgres() {
        let config = ServerConfig::new()
            .storage_backend(StorageBackend::Postgres)
            .postgres_url("postgresql://localhost/test");
        let result = create_storage(&config).await;
        // Should fail because no Postgres is running
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_key_store() {
        let config = ServerConfig::new()
            .api_key("pg_live_test123456789012345678901234567890", Role::Admin)
            .api_key("pg_live_viewer123456789012345678901234567", Role::Viewer);

        let key_store = create_key_store(&config).await.unwrap();
        let keys = key_store.list_keys().await;

        assert_eq!(keys.len(), 2);
    }

    #[tokio::test]
    async fn test_router_creation() {
        let store = Arc::new(InMemoryStore::new());
        let auth_state = AuthState::new(Arc::new(ApiKeyStore::new()), None);
        let config = ServerConfig::new();

        let _router = create_router(store, auth_state, &config);
        // Router created successfully
    }
}
