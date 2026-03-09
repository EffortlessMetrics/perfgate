//! perfgate-server binary - REST API server for baseline management.
//!
//! Usage:
//!
//! ```bash
//! # Start server with in-memory storage
//! perfgate-server
//!
//! # Start with SQLite storage
//! perfgate-server --storage-type sqlite --database-url ./perfgate.db
//!
//! # Specify bind address and port
//! perfgate-server --bind 127.0.0.1 --port 3000
//!
//! # Add API keys
//! perfgate-server --api-keys admin:pg_live_abc123...,viewer:pg_live_def456...
//! ```

use clap::Parser;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use perfgate_server::{Role, ServerConfig, StorageBackend, run_server};

/// perfgate baseline service server.
#[derive(Parser, Debug)]
#[command(name = "perfgate-server")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Bind address (default: 0.0.0.0)
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,

    /// Port number (default: 8080)
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Storage backend type: memory, sqlite, postgres
    #[arg(long, default_value = "memory")]
    storage_type: String,

    /// Database URL (for sqlite: path to db file, for postgres: connection string)
    #[arg(long)]
    database_url: Option<String>,

    /// API keys in format "role:key" (comma-separated, can be specified multiple times)
    /// Roles: admin, promoter, contributor, viewer
    /// Example: --api-keys admin:pg_live_abc123...,viewer:pg_live_def456...
    #[arg(long = "api-keys", value_parser = parse_api_key)]
    api_keys: Vec<(Role, String)>,

    /// Disable CORS
    #[arg(long)]
    no_cors: bool,

    /// Request timeout in seconds
    #[arg(long, default_value_t = 30)]
    timeout: u64,

    /// Log level: trace, debug, info, warn, error
    #[arg(long, default_value = "info")]
    log_level: String,

    /// Log format: json, pretty
    #[arg(long, default_value = "json")]
    log_format: String,
}

/// Parses an API key argument in format "role:key".
fn parse_api_key(s: &str) -> Result<(Role, String), String> {
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(format!(
            "Invalid API key format '{}'. Expected 'role:key'",
            s
        ));
    }

    let role = match parts[0].to_lowercase().as_str() {
        "admin" => Role::Admin,
        "promoter" => Role::Promoter,
        "contributor" => Role::Contributor,
        "viewer" => Role::Viewer,
        _ => {
            return Err(format!(
                "Unknown role '{}'. Expected: admin, promoter, contributor, viewer",
                parts[0]
            ));
        }
    };

    Ok((role, parts[1].to_string()))
}

/// Initializes the logging system.
fn init_logging(level: &str, format: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));

    let registry = tracing_subscriber::registry().with(filter);

    match format {
        "pretty" => {
            registry
                .with(tracing_subscriber::fmt::layer().pretty())
                .init();
        }
        _ => {
            registry
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize logging
    init_logging(&args.log_level, &args.log_format);

    // Parse storage backend
    let storage_backend: StorageBackend = args
        .storage_type
        .parse()
        .unwrap_or_else(|e| panic!("Invalid storage type: {}", e));

    // Build bind address
    let bind_addr: SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .unwrap_or_else(|e| panic!("Invalid bind address: {}", e));

    // Build configuration
    let mut config = ServerConfig::new()
        .bind(bind_addr.to_string())
        .unwrap_or_else(|_| panic!("Invalid bind address"))
        .storage_backend(storage_backend)
        .cors(!args.no_cors);

    // Set database path for SQLite
    if storage_backend == StorageBackend::Sqlite {
        let path = args
            .database_url
            .clone()
            .unwrap_or_else(|| "perfgate.db".to_string());
        config = config.sqlite_path(path);
    }

    // Set PostgreSQL URL if provided
    if storage_backend == StorageBackend::Postgres {
        if let Some(url) = args.database_url {
            config = config.postgres_url(url);
        }
    }

    // Add API keys
    for (role, key) in args.api_keys {
        config = config.api_key(key, role);
    }

    info!(
        bind = %bind_addr,
        storage = ?storage_backend,
        "Starting perfgate server"
    );

    // Run the server
    if let Err(e) = run_server(config).await {
        eprintln!("Server error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_key_admin() {
        let (role, key) = parse_api_key("admin:pg_live_abc123").unwrap();
        assert_eq!(role, Role::Admin);
        assert_eq!(key, "pg_live_abc123");
    }

    #[test]
    fn test_parse_api_key_viewer() {
        let (role, key) = parse_api_key("viewer:pg_live_xyz789").unwrap();
        assert_eq!(role, Role::Viewer);
        assert_eq!(key, "pg_live_xyz789");
    }

    #[test]
    fn test_parse_api_key_case_insensitive() {
        let (role, _) = parse_api_key("ADMIN:pg_live_abc").unwrap();
        assert_eq!(role, Role::Admin);

        let (role, _) = parse_api_key("Contributor:pg_live_abc").unwrap();
        assert_eq!(role, Role::Contributor);
    }

    #[test]
    fn test_parse_api_key_invalid_format() {
        assert!(parse_api_key("invalid").is_err());
        assert!(parse_api_key("invalidrole:pg_live_abc").is_err());
    }

    #[test]
    fn test_cli_args_default() {
        let args = Args::try_parse_from(["perfgate-server"]).unwrap();
        assert_eq!(args.bind, "0.0.0.0");
        assert_eq!(args.port, 8080);
        assert_eq!(args.storage_type, "memory");
        assert!(!args.no_cors);
    }

    #[test]
    fn test_cli_args_custom() {
        let args = Args::try_parse_from([
            "perfgate-server",
            "--bind",
            "127.0.0.1",
            "--port",
            "3000",
            "--storage-type",
            "sqlite",
            "--database-url",
            "/tmp/test.db",
            "--no-cors",
            "--api-keys",
            "admin:pg_live_abc123",
        ])
        .unwrap();

        assert_eq!(args.bind, "127.0.0.1");
        assert_eq!(args.port, 3000);
        assert_eq!(args.storage_type, "sqlite");
        assert_eq!(args.database_url, Some("/tmp/test.db".to_string()));
        assert!(args.no_cors);
        assert_eq!(args.api_keys.len(), 1);
    }
}
