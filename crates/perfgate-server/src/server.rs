//! Server configuration and bootstrap.
//!
//! This module provides the [`ServerConfig`] and [`run_server`] function
//! for starting the HTTP server.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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

use crate::auth::{
    ApiKey, ApiKeyStore, AuthState, JwtConfig, Role, auth_middleware, local_mode_auth_middleware,
};
use crate::cleanup::spawn_cleanup_task;
use crate::error::ConfigError;
use crate::handlers::{
    DefaultRetentionDays, admin_cleanup, create_key, dashboard_index, delete_baseline,
    dependency_impact, get_baseline, get_latest_baseline, get_trend, health_check,
    list_audit_events, list_baselines, list_fleet_alerts, list_keys, list_verdicts,
    promote_baseline, record_dependency_event, revoke_key, static_asset, submit_verdict,
    upload_baseline,
};
use crate::metrics::{metrics_handler, metrics_middleware, setup_metrics_recorder};
use crate::oidc::{OidcConfig, OidcProvider, OidcRegistry};
use crate::storage::fleet::{FleetStore, InMemoryFleetStore};
use crate::storage::{
    ArtifactStore, AuditStore, BaselineStore, InMemoryKeyStore, InMemoryStore, KeyStore,
    ObjectArtifactStore, PostgresStore, SqliteKeyStore, SqliteStore,
};
use metrics_exporter_prometheus::PrometheusHandle;

/// Shared application state holding all stores.
#[derive(Clone)]
pub struct AppState {
    /// Baseline/verdict store
    pub store: Arc<dyn BaselineStore>,
    /// Audit event store
    pub audit: Arc<dyn AuditStore>,
}

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

/// Configuration for PostgreSQL connection pool tuning.
///
/// These parameters control how sqlx manages the underlying connection pool.
/// All fields have sensible defaults suitable for moderate production load.
///
/// # Recommended PostgreSQL server settings
///
/// For production workloads, tune these `postgresql.conf` knobs alongside the
/// pool parameters:
///
/// | PostgreSQL setting      | Recommended value | Notes                                    |
/// |-------------------------|-------------------|------------------------------------------|
/// | `max_connections`       | 100 - 200         | Must exceed pool `max_connections`        |
/// | `shared_buffers`        | 25% of RAM        | Main query cache                         |
/// | `work_mem`              | 4 - 16 MB         | Per-sort/hash memory                     |
/// | `idle_in_transaction_session_timeout` | 30s  | Kill idle-in-transaction sessions        |
/// | `statement_timeout`     | 30s               | Server-side query timeout                |
/// | `tcp_keepalives_idle`   | 60                | Seconds before TCP keepalive probes      |
/// | `tcp_keepalives_interval` | 10              | Seconds between probes                   |
/// | `tcp_keepalives_count`  | 6                 | Failed probes before disconnect           |
#[derive(Debug, Clone)]
pub struct PostgresPoolConfig {
    /// Maximum number of connections in the pool (default: 10).
    pub max_connections: u32,
    /// Minimum number of idle connections to maintain (default: 2).
    pub min_connections: u32,
    /// Time a connection may sit idle before being closed (default: 300s).
    pub idle_timeout: Duration,
    /// Maximum lifetime of a connection before it is recycled (default: 1800s).
    pub max_lifetime: Duration,
    /// How long to wait when acquiring a connection from the pool (default: 5s).
    pub acquire_timeout: Duration,
    /// Statement timeout set on each new connection via `SET statement_timeout`
    /// (default: 30s). Prevents runaway queries.
    pub statement_timeout: Duration,
}

impl Default for PostgresPoolConfig {
    fn default() -> Self {
        Self {
            max_connections: 10,
            min_connections: 2,
            idle_timeout: Duration::from_secs(300),
            max_lifetime: Duration::from_secs(1800),
            acquire_timeout: Duration::from_secs(5),
            statement_timeout: Duration::from_secs(30),
        }
    }
}

/// API key configuration.
#[derive(Debug, Clone)]
pub struct ApiKeyConfig {
    /// The actual API key string
    pub key: String,
    /// Assigned role
    pub role: Role,
    /// Project identifier the key is restricted to
    pub project: String,
    /// Optional regex to restrict access to specific benchmarks
    pub benchmark_regex: Option<String>,
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

    /// PostgreSQL connection pool configuration
    pub postgres_pool: PostgresPoolConfig,

    /// Artifact storage URL (e.g., s3://bucket/prefix)
    pub artifacts_url: Option<String>,

    /// API keys for authentication
    pub api_keys: Vec<ApiKeyConfig>,

    /// Optional JWT validation settings.
    pub jwt: Option<JwtConfig>,

    /// OIDC provider configurations (GitHub, GitLab, custom).
    pub oidc_configs: Vec<OidcConfig>,

    /// Enable CORS for all origins
    pub cors: bool,

    /// Request timeout in seconds
    pub timeout_seconds: u64,

    /// Local mode: disable authentication for single-user local use.
    pub local_mode: bool,

    /// Artifact retention period in days (0 = no cleanup).
    pub retention_days: u64,

    /// Interval between background cleanup passes (in hours).
    pub cleanup_interval_hours: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".parse().unwrap(),
            storage_backend: StorageBackend::Memory,
            sqlite_path: None,
            postgres_url: None,
            postgres_pool: PostgresPoolConfig::default(),
            artifacts_url: None,
            api_keys: vec![],
            jwt: None,
            oidc_configs: vec![],
            cors: true,
            timeout_seconds: 30,
            local_mode: false,
            retention_days: 0,
            cleanup_interval_hours: 1,
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

    /// Sets the PostgreSQL connection pool configuration.
    pub fn postgres_pool(mut self, pool_config: PostgresPoolConfig) -> Self {
        self.postgres_pool = pool_config;
        self
    }

    /// Sets the artifacts storage URL.
    pub fn artifacts_url(mut self, url: impl Into<String>) -> Self {
        self.artifacts_url = Some(url.into());
        self
    }

    /// Adds an API key with a specific role.
    pub fn api_key(self, key: impl Into<String>, role: Role) -> Self {
        self.scoped_api_key(key, role, "default", None)
    }

    /// Adds a scoped API key restricted to a project and optional benchmark regex.
    pub fn scoped_api_key(
        mut self,
        key: impl Into<String>,
        role: Role,
        project: impl Into<String>,
        benchmark_regex: Option<String>,
    ) -> Self {
        self.api_keys.push(ApiKeyConfig {
            key: key.into(),
            role,
            project: project.into(),
            benchmark_regex,
        });
        self
    }

    /// Enables JWT token authentication.
    pub fn jwt(mut self, jwt: JwtConfig) -> Self {
        self.jwt = Some(jwt);
        self
    }

    /// Adds an OIDC provider configuration.
    ///
    /// Multiple providers can be registered (e.g. GitHub + GitLab).
    pub fn oidc(mut self, config: OidcConfig) -> Self {
        self.oidc_configs.push(config);
        self
    }

    /// Enables or disables CORS.
    pub fn cors(mut self, enabled: bool) -> Self {
        self.cors = enabled;
        self
    }

    /// Enables or disables local mode (no authentication).
    pub fn local_mode(mut self, enabled: bool) -> Self {
        self.local_mode = enabled;
        self
    }

    /// Sets the artifact retention period in days. 0 means no cleanup.
    pub fn retention_days(mut self, days: u64) -> Self {
        self.retention_days = days;
        self
    }

    /// Sets the interval (in hours) between background cleanup passes.
    pub fn cleanup_interval_hours(mut self, hours: u64) -> Self {
        self.cleanup_interval_hours = hours;
        self
    }
}

/// Creates the artifact storage based on configuration.
pub(crate) async fn create_artifacts(
    config: &ServerConfig,
) -> Result<Option<Arc<dyn ArtifactStore>>, ConfigError> {
    if let Some(url) = &config.artifacts_url {
        info!(url = %url, "Using object storage for artifacts");
        let (store, _path) = object_store::parse_url(
            &url.parse()
                .map_err(|e| ConfigError::InvalidValue(format!("Invalid artifacts URL: {}", e)))?,
        )
        .map_err(|e| ConfigError::InvalidValue(format!("Failed to parse artifacts URL: {}", e)))?;

        Ok(Some(Arc::new(ObjectArtifactStore::new(Arc::from(store)))))
    } else {
        Ok(None)
    }
}

/// Creates the storage backend based on configuration.
///
/// Returns both a `BaselineStore` and an `AuditStore` backed by the same backend.
#[allow(dead_code)]
pub(crate) async fn create_storage(
    config: &ServerConfig,
) -> Result<(Arc<dyn BaselineStore>, Arc<dyn AuditStore>), ConfigError> {
    let artifacts = create_artifacts(config).await?;
    create_storage_with_artifacts(config, artifacts).await
}

/// Creates the storage backend with a pre-built artifact store.
pub(crate) async fn create_storage_with_artifacts(
    config: &ServerConfig,
    artifacts: Option<Arc<dyn ArtifactStore>>,
) -> Result<(Arc<dyn BaselineStore>, Arc<dyn AuditStore>), ConfigError> {
    match config.storage_backend {
        StorageBackend::Memory => {
            info!("Using in-memory storage");
            let store = Arc::new(InMemoryStore::new());
            Ok((store.clone(), store))
        }
        StorageBackend::Sqlite => {
            let path = config
                .sqlite_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("perfgate.db"));
            info!(path = %path.display(), "Using SQLite storage");
            let store = SqliteStore::new(&path, artifacts)
                .map_err(|e| ConfigError::InvalidValue(format!("Failed to open SQLite: {}", e)))?;
            let store = Arc::new(store);
            Ok((store.clone(), store))
        }
        StorageBackend::Postgres => {
            let url = config
                .postgres_url
                .clone()
                .unwrap_or_else(|| "postgres://localhost:5432/perfgate".to_string());
            info!(url = %url, "Using PostgreSQL storage");
            let store = PostgresStore::new(&url, artifacts, &config.postgres_pool)
                .await
                .map_err(|e| {
                    ConfigError::InvalidValue(format!("Failed to connect to Postgres: {}", e))
                })?;
            let store = Arc::new(store);
            Ok((store.clone(), store))
        }
    }
}

/// Creates the in-memory API key store from CLI configuration.
pub(crate) async fn create_key_store(
    config: &ServerConfig,
) -> Result<Arc<ApiKeyStore>, ConfigError> {
    let store = ApiKeyStore::new();

    // Add configured API keys
    for cfg in &config.api_keys {
        let mut api_key = ApiKey::new(
            uuid::Uuid::new_v4().to_string(),
            format!("{:?} key for {}", cfg.role, cfg.project),
            cfg.project.clone(),
            cfg.role,
        );
        api_key.benchmark_regex = cfg.benchmark_regex.clone();

        store.add_key(api_key, &cfg.key).await;
        info!(role = ?cfg.role, project = %cfg.project, "Added API key");
    }

    Ok(Arc::new(store))
}

/// Creates the persistent key store based on the storage backend.
pub(crate) fn create_persistent_key_store(
    config: &ServerConfig,
    sqlite_conn: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
) -> Result<Arc<dyn KeyStore>, ConfigError> {
    match config.storage_backend {
        StorageBackend::Sqlite => {
            if let Some(conn) = sqlite_conn {
                let store = SqliteKeyStore::new(conn).map_err(|e| {
                    ConfigError::InvalidValue(format!("Failed to create SQLite key store: {}", e))
                })?;
                info!("Using SQLite persistent key store");
                Ok(Arc::new(store))
            } else {
                info!("Using in-memory key store (no SQLite connection available)");
                Ok(Arc::new(InMemoryKeyStore::new()))
            }
        }
        _ => {
            info!("Using in-memory key store");
            Ok(Arc::new(InMemoryKeyStore::new()))
        }
    }
}

/// Creates the fleet store (currently always in-memory).
pub(crate) fn create_fleet_store() -> Arc<dyn FleetStore> {
    Arc::new(InMemoryFleetStore::new())
}

/// Creates the router with all routes configured.
pub(crate) fn create_router(
    state: AppState,
    persistent_key_store: Arc<dyn KeyStore>,
    fleet_store: Arc<dyn FleetStore>,
    artifact_store: Option<Arc<dyn ArtifactStore>>,
    auth_state: AuthState,
    config: &ServerConfig,
    prometheus_handle: Option<PrometheusHandle>,
) -> Router {
    let local_mode = config.local_mode;

    // Health check (no auth required)
    let health_routes = Router::new().route("/health", get(health_check));

    // Info endpoint: exposes local_mode flag for the dashboard.
    let info_routes = Router::new().route(
        "/info",
        get(move || async move { axum::Json(serde_json::json!({ "local_mode": local_mode })) }),
    );

    // Dashboard routes (no auth required for read-only view)
    let dashboard_routes = Router::new()
        .route("/", get(dashboard_index))
        .route("/index.html", get(dashboard_index))
        .route("/assets/{*path}", get(static_asset));

    // Key management routes (admin-only, using the persistent key store as state)
    let key_routes = Router::new()
        .route("/keys", post(create_key))
        .route("/keys", get(list_keys))
        .route("/keys/{id}", delete(revoke_key))
        .with_state(persistent_key_store)
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ));

    // Admin routes (require admin auth, never skipped even in local mode)
    let admin_routes = Router::new()
        .route("/admin/cleanup", delete(admin_cleanup))
        .layer(axum::Extension(artifact_store.clone()))
        .layer(axum::Extension(DefaultRetentionDays(config.retention_days)))
        .layer(middleware::from_fn_with_state(
            auth_state.clone(),
            auth_middleware,
        ));

    // Fleet routes (cross-project, no per-project auth scope)
    let fleet_routes = Router::new()
        .route("/fleet/dependency-event", post(record_dependency_event))
        .route("/fleet/alerts", get(list_fleet_alerts))
        .route(
            "/fleet/dependency/{dep_name}/impact",
            get(dependency_impact),
        )
        .with_state(fleet_store);

    // API routes — auth middleware is skipped in local mode.
    let api_routes_inner = Router::new()
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
        .route("/projects/{project}/verdicts", post(submit_verdict))
        .route("/projects/{project}/verdicts", get(list_verdicts))
        .route(
            "/projects/{project}/baselines/{benchmark}/promote",
            post(promote_baseline),
        )
        .route(
            "/projects/{project}/baselines/{benchmark}/trend",
            get(get_trend),
        )
        // Audit log
        .route("/audit", get(list_audit_events));

    let api_routes = if config.local_mode {
        api_routes_inner.layer(middleware::from_fn(local_mode_auth_middleware))
    } else {
        api_routes_inner.layer(middleware::from_fn_with_state(auth_state, auth_middleware))
    };

    // Combine routes under /api/v1, plus root /health and dashboard
    let mut app = Router::new()
        .merge(dashboard_routes)
        .merge(health_routes.clone())
        .nest(
            "/api/v1",
            health_routes
                .merge(info_routes)
                .merge(api_routes)
                .merge(key_routes)
                .merge(admin_routes)
                .merge(fleet_routes),
        );

    // Add /metrics endpoint if Prometheus handle is available
    if let Some(handle) = prometheus_handle {
        let metrics_routes = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(handle);
        app = app.merge(metrics_routes);
        // Apply metrics middleware to all routes
        app = app.layer(middleware::from_fn(metrics_middleware));
    }

    // Add CORS if enabled
    if config.cors {
        app = app.layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    }

    app.with_state(state)
}

/// Runs the HTTP server.
///
/// This function starts the server and blocks until shutdown.
pub async fn run_server(config: ServerConfig) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        bind = %config.bind,
        backend = ?config.storage_backend,
        retention_days = config.retention_days,
        "Starting perfgate server"
    );

    // Create artifact store (shared between baseline store and cleanup)
    let artifact_store = create_artifacts(&config).await?;

    // Create storage
    let (store, audit) = create_storage_with_artifacts(&config, artifact_store.clone()).await?;

    // Create the persistent key store (shares SQLite connection when applicable)
    let sqlite_conn = if config.storage_backend == StorageBackend::Sqlite {
        let path = config
            .sqlite_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("perfgate.db"));
        let conn = rusqlite::Connection::open(&path).map_err(|e| {
            ConfigError::InvalidValue(format!("Failed to open SQLite for key store: {}", e))
        })?;
        Some(Arc::new(std::sync::Mutex::new(conn)))
    } else {
        None
    };
    let persistent_key_store = create_persistent_key_store(&config, sqlite_conn)?;

    // Create in-memory key store (for CLI-provided keys)
    let key_store = create_key_store(&config).await?;

    let mut oidc_registry = OidcRegistry::new();
    for oidc_cfg in &config.oidc_configs {
        let provider = OidcProvider::new(oidc_cfg.clone())
            .await
            .map_err(|e| e.to_string())?;
        oidc_registry.add(provider);
    }

    let auth_state = AuthState::new(key_store, config.jwt.clone(), oidc_registry)
        .with_persistent_key_store(persistent_key_store.clone());

    // Spawn background cleanup task if retention is configured
    let cleanup_handle = if config.retention_days > 0 {
        if let Some(ref art_store) = artifact_store {
            info!(
                retention_days = config.retention_days,
                interval_hours = config.cleanup_interval_hours,
                "Spawning background artifact cleanup task"
            );
            Some(spawn_cleanup_task(
                art_store.clone(),
                config.retention_days,
                config.cleanup_interval_hours,
            ))
        } else {
            info!(
                "Retention policy configured but no artifact store available; skipping background cleanup"
            );
            None
        }
    } else {
        None
    };

    // Install Prometheus metrics recorder
    let prometheus_handle = setup_metrics_recorder();
    info!("Prometheus metrics enabled at /metrics");

    let app_state = AppState { store, audit };

    // Create fleet store
    let fleet_store = create_fleet_store();

    // Create router
    let app = create_router(
        app_state,
        persistent_key_store.clone(),
        fleet_store,
        artifact_store,
        auth_state,
        &config,
        Some(prometheus_handle),
    );

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

    // Abort cleanup task on shutdown
    if let Some(handle) = cleanup_handle {
        handle.abort();
    }

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
            .scoped_api_key(
                "scoped-key",
                Role::Contributor,
                "my-proj",
                Some("^bench-.*$".to_string()),
            )
            .jwt(JwtConfig::hs256(b"test-secret".to_vec()).issuer("perfgate"))
            .cors(false);

        assert_eq!(config.bind.to_string(), "127.0.0.1:3000");
        assert_eq!(config.storage_backend, StorageBackend::Sqlite);
        assert_eq!(config.sqlite_path, Some(PathBuf::from("/tmp/test.db")));
        assert_eq!(config.api_keys.len(), 2);
        assert_eq!(config.api_keys[1].project, "my-proj");
        assert_eq!(
            config.api_keys[1].benchmark_regex,
            Some("^bench-.*$".to_string())
        );
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
        let (storage, _audit) = create_storage(&config).await.unwrap();
        assert_eq!(storage.backend_type(), "memory");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_storage_sqlite() {
        let config = ServerConfig::new()
            .storage_backend(StorageBackend::Sqlite)
            .sqlite_path(":memory:");
        let (storage, _audit) = create_storage(&config).await.unwrap();
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
            .scoped_api_key(
                "pg_live_viewer123456789012345678901234567",
                Role::Viewer,
                "project-1",
                None,
            );

        let key_store = create_key_store(&config).await.unwrap();
        let keys = key_store.list_keys().await;

        assert_eq!(keys.len(), 2);
        let viewer_key = keys.iter().find(|k| k.role == Role::Viewer).unwrap();
        assert_eq!(viewer_key.project_id, "project-1");
    }

    #[tokio::test]
    async fn test_router_creation() {
        let store = Arc::new(InMemoryStore::new());
        let persistent_key_store: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let fleet_store = create_fleet_store();
        let auth_state = AuthState::new(Arc::new(ApiKeyStore::new()), None, Default::default());
        let config = ServerConfig::new();
        let app_state = AppState {
            store: store.clone(),
            audit: store,
        };

        let _router = create_router(
            app_state,
            persistent_key_store,
            fleet_store,
            None,
            auth_state,
            &config,
            None,
        );
        // Router created successfully
    }

    #[tokio::test]
    async fn test_router_local_mode_injects_auth_context_for_api_routes() {
        let store = Arc::new(InMemoryStore::new());
        let persistent_key_store: Arc<dyn KeyStore> = Arc::new(InMemoryKeyStore::new());
        let fleet_store = create_fleet_store();
        let auth_state = AuthState::new(Arc::new(ApiKeyStore::new()), None, Default::default());
        let config = ServerConfig::new().local_mode(true);
        let app_state = AppState {
            store: store.clone(),
            audit: store,
        };

        let router = create_router(
            app_state,
            persistent_key_store,
            fleet_store,
            None,
            auth_state,
            &config,
            None,
        );

        let response = tower::ServiceExt::oneshot(
            router,
            axum::http::Request::builder()
                .uri("/api/v1/projects/test/baselines")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), axum::http::StatusCode::OK);
    }

    #[test]
    fn test_postgres_pool_config_defaults() {
        let cfg = PostgresPoolConfig::default();
        assert_eq!(cfg.max_connections, 10);
        assert_eq!(cfg.min_connections, 2);
        assert_eq!(cfg.idle_timeout, Duration::from_secs(300));
        assert_eq!(cfg.max_lifetime, Duration::from_secs(1800));
        assert_eq!(cfg.acquire_timeout, Duration::from_secs(5));
        assert_eq!(cfg.statement_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_server_config_with_postgres_pool() {
        let pool_config = PostgresPoolConfig {
            max_connections: 20,
            min_connections: 5,
            idle_timeout: Duration::from_secs(120),
            max_lifetime: Duration::from_secs(3600),
            acquire_timeout: Duration::from_secs(10),
            statement_timeout: Duration::from_secs(60),
        };

        let config = ServerConfig::new()
            .storage_backend(StorageBackend::Postgres)
            .postgres_url("postgres://localhost:5432/perfgate")
            .postgres_pool(pool_config);

        assert_eq!(config.postgres_pool.max_connections, 20);
        assert_eq!(config.postgres_pool.min_connections, 5);
        assert_eq!(config.postgres_pool.idle_timeout, Duration::from_secs(120));
        assert_eq!(config.postgres_pool.max_lifetime, Duration::from_secs(3600));
        assert_eq!(
            config.postgres_pool.acquire_timeout,
            Duration::from_secs(10)
        );
        assert_eq!(
            config.postgres_pool.statement_timeout,
            Duration::from_secs(60)
        );
    }

    #[tokio::test]
    async fn test_health_endpoint_no_pool_for_memory() {
        let store: Arc<dyn crate::storage::BaselineStore> = Arc::new(InMemoryStore::new());
        assert!(store.pool_metrics().is_none());
    }
}
