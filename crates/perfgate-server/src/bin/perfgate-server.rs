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
//!
//! # GitHub Actions OIDC
//! perfgate-server --github-oidc EffortlessMetrics/perfgate:perfgate-oss:contributor
//!
//! # GitLab CI OIDC (gitlab.com)
//! perfgate-server --gitlab-oidc mygroup/myproject:my-project:contributor
//!
//! # GitLab CI OIDC (self-managed)
//! perfgate-server --gitlab-oidc mygroup/myproject:my-project:contributor \
//!     --gitlab-oidc-issuer https://gitlab.example.com
//!
//! # Custom OIDC provider
//! perfgate-server --oidc-provider issuer=https://auth.example.com,jwks_url=https://auth.example.com/.well-known/jwks.json,audience=perfgate,claim=team_slug,mapping=platform-team:my-project:contributor
//! ```

use clap::Parser;
use std::net::SocketAddr;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use perfgate_server::{
    JwtConfig, OidcConfig, PostgresPoolConfig, Role, ServerConfig, StorageBackend, run_server,
};
use std::time::Duration;

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

    /// Maximum number of connections in the PostgreSQL pool (default: 10)
    #[arg(long, default_value_t = 10)]
    pg_max_connections: u32,

    /// Minimum number of idle connections in the PostgreSQL pool (default: 2)
    #[arg(long, default_value_t = 2)]
    pg_min_connections: u32,

    /// Idle timeout in seconds for PostgreSQL pool connections (default: 300)
    #[arg(long, default_value_t = 300)]
    pg_idle_timeout: u64,

    /// Maximum lifetime in seconds for PostgreSQL pool connections (default: 1800)
    #[arg(long, default_value_t = 1800)]
    pg_max_lifetime: u64,

    /// Acquire timeout in seconds for the PostgreSQL pool (default: 5)
    #[arg(long, default_value_t = 5)]
    pg_acquire_timeout: u64,

    /// Statement timeout in seconds set on each new PostgreSQL connection (default: 30)
    #[arg(long, default_value_t = 30)]
    pg_statement_timeout: u64,

    /// API keys in format "role:key[:project[:benchmark_regex]]" (comma-separated, can be specified multiple times)
    /// Roles: admin, promoter, contributor, viewer
    /// project: defaults to "default" if omitted.
    /// benchmark_regex: optional regex to restrict benchmarks.
    /// Example: --api-keys admin:pg_live_abc123,contributor:pg_live_def456:my-project:^bench-.*$
    #[arg(long = "api-keys", value_parser = parse_api_key)]
    api_keys: Vec<ApiKeyConfigArg>,

    /// Environment variable that contains JSON key-policy document.
    #[arg(long = "api-keys-env")]
    api_keys_env: Option<String>,

    /// File path to JSON/TOML key-policy document.
    #[arg(long = "api-keys-file")]
    api_keys_file: Option<String>,

    /// Command that prints JSON/TOML key-policy document to stdout.
    #[arg(long = "api-keys-command")]
    api_keys_command: Option<String>,

    /// Reload interval for auth sources like --api-keys-file/--api-keys-command.
    #[arg(long = "reload-interval")]
    reload_interval: Option<String>,

    /// HS256 secret used to validate `Authorization: Token <jwt>` requests.
    #[arg(long)]
    jwt_secret: Option<String>,

    /// Expected JWT issuer.
    #[arg(long)]
    jwt_issuer: Option<String>,

    /// Expected JWT audience.
    #[arg(long)]
    jwt_audience: Option<String>,

    /// GitHub Actions OIDC mapping in format "org/repo:project_id:role" (can be specified multiple times).
    /// Example: --github-oidc EffortlessMetrics/perfgate:perfgate-oss:contributor
    #[arg(long = "github-oidc")]
    github_oidc: Vec<String>,

    /// Expected audience for GitHub OIDC tokens.
    #[arg(long, default_value = "perfgate")]
    github_oidc_audience: String,

    /// GitLab CI OIDC mapping in format "group/project:project_id:role" (can be specified multiple times).
    /// Example: --gitlab-oidc mygroup/myproject:my-project:contributor
    #[arg(long = "gitlab-oidc")]
    gitlab_oidc: Vec<String>,

    /// Expected audience for GitLab OIDC tokens.
    #[arg(long, default_value = "perfgate")]
    gitlab_oidc_audience: String,

    /// GitLab issuer URL for self-managed instances (default: https://gitlab.com).
    #[arg(long, default_value = "https://gitlab.com")]
    gitlab_oidc_issuer: String,

    /// Custom OIDC provider in format
    /// "issuer=URL,jwks_url=URL,audience=AUD,claim=FIELD,mapping=IDENTITY:PROJECT:ROLE".
    /// The `mapping` key can be repeated by passing --oidc-provider multiple times with the same
    /// issuer or by using semicolons: "...,mapping=id1:proj1:role1;id2:proj2:role2".
    #[arg(long = "oidc-provider")]
    oidc_provider: Vec<String>,

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

    /// Artifact retention period in days. Objects older than this are
    /// automatically cleaned up. 0 = no automatic cleanup (default).
    #[arg(long, default_value_t = 0)]
    retention_days: u64,

    /// Hours between background cleanup passes (default: 1).
    #[arg(long, default_value_t = 1)]
    cleanup_interval_hours: u64,
}

/// Helper struct for parsing API key CLI arguments.
#[derive(Debug, Clone)]
struct ApiKeyConfigArg {
    pub role: Role,
    pub key: String,
    pub project: String,
    pub benchmark_regex: Option<String>,
}

/// Parses an API key argument in format "role:key[:project[:benchmark_regex]]".
fn parse_api_key(s: &str) -> Result<ApiKeyConfigArg, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 4 {
        return Err(format!(
            "Invalid API key format '{}'. Expected 'role:key[:project[:benchmark_regex]]'",
            s
        ));
    }

    let role = parse_role(parts[0])?;

    let key = parts[1].to_string();
    let project = parts.get(2).unwrap_or(&"default").to_string();
    let benchmark_regex = parts.get(3).map(|s| {
        if *s == "*" {
            ".*".to_string()
        } else {
            s.to_string()
        }
    });

    Ok(ApiKeyConfigArg {
        role,
        key,
        project,
        benchmark_regex,
    })
}

/// Parses a role string (case-insensitive).
fn parse_role(s: &str) -> Result<Role, String> {
    match s.to_lowercase().as_str() {
        "admin" => Ok(Role::Admin),
        "promoter" => Ok(Role::Promoter),
        "contributor" => Ok(Role::Contributor),
        "viewer" => Ok(Role::Viewer),
        _ => Err(format!(
            "Unknown role '{}'. Expected: admin, promoter, contributor, viewer",
            s
        )),
    }
}

/// Parses an OIDC identity mapping "identity:project_id:role".
fn parse_oidc_mapping(mapping: &str, flag_name: &str) -> Result<(String, String, Role), String> {
    let parts: Vec<&str> = mapping.split(':').collect();
    if parts.len() != 3 {
        return Err(format!(
            "Invalid {} format '{}'. Expected 'identity:project_id:role'",
            flag_name, mapping
        ));
    }
    let identity = parts[0].to_string();
    let project = parts[1].to_string();
    let role = parse_role(parts[2])
        .map_err(|e| format!("In {} mapping '{}': {}", flag_name, mapping, e))?;
    Ok((identity, project, role))
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
        .cors(!args.no_cors)
        .retention_days(args.retention_days)
        .cleanup_interval_hours(args.cleanup_interval_hours);

    // Set database path for SQLite
    if storage_backend == StorageBackend::Sqlite {
        let path = args
            .database_url
            .clone()
            .unwrap_or_else(|| "perfgate.db".to_string());
        config = config.sqlite_path(path);
    }

    // Set PostgreSQL URL and pool configuration if provided
    if let (StorageBackend::Postgres, Some(url)) = (storage_backend, args.database_url) {
        config = config.postgres_url(url);
    }

    config = config.postgres_pool(PostgresPoolConfig {
        max_connections: args.pg_max_connections,
        min_connections: args.pg_min_connections,
        idle_timeout: Duration::from_secs(args.pg_idle_timeout),
        max_lifetime: Duration::from_secs(args.pg_max_lifetime),
        acquire_timeout: Duration::from_secs(args.pg_acquire_timeout),
        statement_timeout: Duration::from_secs(args.pg_statement_timeout),
    });

    // Add API keys
    for cfg in args.api_keys {
        config = config.scoped_api_key(cfg.key, cfg.role, cfg.project, cfg.benchmark_regex);
    }
    if let Some(env_var) = args.api_keys_env {
        config = config.api_keys_env(env_var);
    }
    if let Some(path) = args.api_keys_file {
        config = config.api_keys_file(path);
    }
    if let Some(command) = args.api_keys_command {
        config = config.api_keys_command(command);
    }
    if let Some(interval) = args.reload_interval {
        let parsed = humantime::parse_duration(&interval)
            .unwrap_or_else(|_| panic!("Invalid --reload-interval duration: {}", interval));
        config = config.auth_reload_interval(parsed);
    }

    // ----- GitHub OIDC -----
    if !args.github_oidc.is_empty() {
        let mut oidc_config = OidcConfig::github(&args.github_oidc_audience);
        for mapping in &args.github_oidc {
            let (identity, project, role) =
                parse_oidc_mapping(mapping, "--github-oidc").unwrap_or_else(|e| panic!("{}", e));
            oidc_config = oidc_config.add_mapping(identity, project, role);
        }
        config = config.oidc(oidc_config);
    }

    // ----- GitLab OIDC -----
    if !args.gitlab_oidc.is_empty() {
        let mut oidc_config = if args.gitlab_oidc_issuer == "https://gitlab.com" {
            OidcConfig::gitlab(&args.gitlab_oidc_audience)
        } else {
            OidcConfig::gitlab_custom(&args.gitlab_oidc_issuer, &args.gitlab_oidc_audience)
        };
        for mapping in &args.gitlab_oidc {
            let (identity, project, role) =
                parse_oidc_mapping(mapping, "--gitlab-oidc").unwrap_or_else(|e| panic!("{}", e));
            oidc_config = oidc_config.add_mapping(identity, project, role);
        }
        config = config.oidc(oidc_config);
    }

    // ----- Custom OIDC providers -----
    for provider_spec in &args.oidc_provider {
        config = config
            .oidc(parse_custom_oidc_provider(provider_spec).unwrap_or_else(|e| panic!("{}", e)));
    }

    if let Some(secret) = args.jwt_secret {
        let mut jwt = JwtConfig::hs256(secret.into_bytes());
        if let Some(issuer) = args.jwt_issuer {
            jwt = jwt.issuer(issuer);
        }
        if let Some(audience) = args.jwt_audience {
            jwt = jwt.audience(audience);
        }
        config = config.jwt(jwt);
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

/// Parses a custom OIDC provider specification from the CLI.
///
/// Format: `issuer=URL,jwks_url=URL,audience=AUD,claim=FIELD,mapping=ID:PROJ:ROLE[;ID2:PROJ2:ROLE2]`
fn parse_custom_oidc_provider(spec: &str) -> Result<OidcConfig, String> {
    let mut issuer = None;
    let mut jwks_url = None;
    let mut audience = None;
    let mut claim = None;
    let mut mappings: Vec<(String, String, Role)> = Vec::new();

    for part in spec.split(',') {
        if let Some(val) = part.strip_prefix("issuer=") {
            issuer = Some(val.to_string());
        } else if let Some(val) = part.strip_prefix("jwks_url=") {
            jwks_url = Some(val.to_string());
        } else if let Some(val) = part.strip_prefix("audience=") {
            audience = Some(val.to_string());
        } else if let Some(val) = part.strip_prefix("claim=") {
            claim = Some(val.to_string());
        } else if let Some(val) = part.strip_prefix("mapping=") {
            // Support semicolon-separated multiple mappings
            for m in val.split(';') {
                mappings.push(parse_oidc_mapping(m, "--oidc-provider mapping")?);
            }
        } else {
            return Err(format!(
                "Unknown key in --oidc-provider: '{}'. Expected issuer=, jwks_url=, audience=, claim=, mapping=",
                part
            ));
        }
    }

    let issuer = issuer.ok_or("Missing 'issuer=' in --oidc-provider")?;
    let jwks_url = jwks_url.ok_or("Missing 'jwks_url=' in --oidc-provider")?;
    let audience = audience.ok_or("Missing 'audience=' in --oidc-provider")?;
    let claim = claim.ok_or("Missing 'claim=' in --oidc-provider")?;

    if mappings.is_empty() {
        return Err("Missing 'mapping=' in --oidc-provider".to_string());
    }

    let mut config = OidcConfig::custom(issuer, jwks_url, audience, claim);
    for (identity, project, role) in mappings {
        config = config.add_mapping(identity, project, role);
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_key_admin() {
        let arg = parse_api_key("admin:pg_live_abc123").unwrap();
        assert_eq!(arg.role, Role::Admin);
        assert_eq!(arg.key, "pg_live_abc123");
        assert_eq!(arg.project, "default");
        assert_eq!(arg.benchmark_regex, None);
    }

    #[test]
    fn test_parse_api_key_viewer() {
        let arg = parse_api_key("viewer:pg_live_xyz789").unwrap();
        assert_eq!(arg.role, Role::Viewer);
        assert_eq!(arg.key, "pg_live_xyz789");
        assert_eq!(arg.project, "default");
    }

    #[test]
    fn test_parse_api_key_scoped() {
        let arg = parse_api_key("contributor:pg_live_key:my-proj:^bench-.*$").unwrap();
        assert_eq!(arg.role, Role::Contributor);
        assert_eq!(arg.key, "pg_live_key");
        assert_eq!(arg.project, "my-proj");
        assert_eq!(arg.benchmark_regex, Some("^bench-.*$".to_string()));
    }

    #[test]
    fn test_parse_api_key_star_becomes_dot_star() {
        let arg = parse_api_key("contributor:pg_live_key:my-proj:*").unwrap();
        assert_eq!(arg.benchmark_regex, Some(".*".to_string()));

        // Explicit `.*` stays unchanged
        let arg2 = parse_api_key("contributor:pg_live_key:my-proj:.*").unwrap();
        assert_eq!(arg2.benchmark_regex, Some(".*".to_string()));

        // Other valid regex stays unchanged
        let arg3 = parse_api_key("contributor:pg_live_key:my-proj:^bench-.*$").unwrap();
        assert_eq!(arg3.benchmark_regex, Some("^bench-.*$".to_string()));
    }

    #[test]
    fn test_parse_api_key_case_insensitive() {
        let arg = parse_api_key("ADMIN:pg_live_abc").unwrap();
        assert_eq!(arg.role, Role::Admin);

        let arg = parse_api_key("Contributor:pg_live_abc").unwrap();
        assert_eq!(arg.role, Role::Contributor);
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
        assert_eq!(args.retention_days, 0);
        assert_eq!(args.cleanup_interval_hours, 1);
        // Verify default pool parameters
        assert_eq!(args.pg_max_connections, 10);
        assert_eq!(args.pg_min_connections, 2);
        assert_eq!(args.pg_idle_timeout, 300);
        assert_eq!(args.pg_max_lifetime, 1800);
        assert_eq!(args.pg_acquire_timeout, 5);
        assert_eq!(args.pg_statement_timeout, 30);
    }

    #[test]
    fn test_cli_args_postgres_pool() {
        let args = Args::try_parse_from([
            "perfgate-server",
            "--storage-type",
            "postgres",
            "--database-url",
            "postgres://localhost:5432/perfgate",
            "--pg-max-connections",
            "20",
            "--pg-min-connections",
            "5",
            "--pg-idle-timeout",
            "120",
            "--pg-max-lifetime",
            "3600",
            "--pg-acquire-timeout",
            "10",
            "--pg-statement-timeout",
            "60",
        ])
        .unwrap();

        assert_eq!(args.storage_type, "postgres");
        assert_eq!(args.pg_max_connections, 20);
        assert_eq!(args.pg_min_connections, 5);
        assert_eq!(args.pg_idle_timeout, 120);
        assert_eq!(args.pg_max_lifetime, 3600);
        assert_eq!(args.pg_acquire_timeout, 10);
        assert_eq!(args.pg_statement_timeout, 60);
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
            "--jwt-secret",
            "super-secret",
            "--jwt-issuer",
            "perfgate",
            "--jwt-audience",
            "perfgate-api",
        ])
        .unwrap();

        assert_eq!(args.bind, "127.0.0.1");
        assert_eq!(args.port, 3000);
        assert_eq!(args.storage_type, "sqlite");
        assert_eq!(args.database_url, Some("/tmp/test.db".to_string()));
        assert!(args.no_cors);
        assert_eq!(args.api_keys.len(), 1);
        assert_eq!(args.jwt_secret, Some("super-secret".to_string()));
        assert_eq!(args.jwt_issuer, Some("perfgate".to_string()));
        assert_eq!(args.jwt_audience, Some("perfgate-api".to_string()));
    }

    #[test]
    fn test_cli_args_gitlab_oidc() {
        let args = Args::try_parse_from([
            "perfgate-server",
            "--gitlab-oidc",
            "mygroup/myproject:my-project:contributor",
        ])
        .unwrap();

        assert_eq!(args.gitlab_oidc.len(), 1);
        assert_eq!(
            args.gitlab_oidc[0],
            "mygroup/myproject:my-project:contributor"
        );
        assert_eq!(args.gitlab_oidc_issuer, "https://gitlab.com");
        assert_eq!(args.gitlab_oidc_audience, "perfgate");
    }

    #[test]
    fn test_cli_args_gitlab_oidc_self_managed() {
        let args = Args::try_parse_from([
            "perfgate-server",
            "--gitlab-oidc",
            "team/repo:internal:admin",
            "--gitlab-oidc-issuer",
            "https://gitlab.example.com",
            "--gitlab-oidc-audience",
            "my-service",
        ])
        .unwrap();

        assert_eq!(args.gitlab_oidc.len(), 1);
        assert_eq!(args.gitlab_oidc_issuer, "https://gitlab.example.com");
        assert_eq!(args.gitlab_oidc_audience, "my-service");
    }

    #[test]
    fn test_cli_args_oidc_provider() {
        let args = Args::try_parse_from([
            "perfgate-server",
            "--oidc-provider",
            "issuer=https://auth.example.com,jwks_url=https://auth.example.com/jwks,audience=perfgate,claim=org_id,mapping=my-org:proj:contributor",
        ])
        .unwrap();

        assert_eq!(args.oidc_provider.len(), 1);
    }

    #[test]
    fn test_parse_oidc_mapping_valid() {
        let (identity, project, role) =
            parse_oidc_mapping("org/repo:my-proj:contributor", "--github-oidc").unwrap();
        assert_eq!(identity, "org/repo");
        assert_eq!(project, "my-proj");
        assert_eq!(role, Role::Contributor);
    }

    #[test]
    fn test_parse_oidc_mapping_invalid() {
        assert!(parse_oidc_mapping("bad-format", "--test").is_err());
        assert!(parse_oidc_mapping("a:b:badrole", "--test").is_err());
        assert!(parse_oidc_mapping("a:b:c:d", "--test").is_err());
    }

    #[test]
    fn test_parse_custom_oidc_provider_valid() {
        let config = parse_custom_oidc_provider(
            "issuer=https://auth.example.com,jwks_url=https://auth.example.com/jwks,audience=perfgate,claim=team_slug,mapping=platform:proj:contributor",
        ).unwrap();

        assert_eq!(config.issuer, "https://auth.example.com");
        assert_eq!(config.jwks_url, "https://auth.example.com/jwks");
        assert_eq!(config.audience, "perfgate");
        assert_eq!(config.repo_mappings.len(), 1);
        assert!(config.repo_mappings.contains_key("platform"));
    }

    #[test]
    fn test_parse_custom_oidc_provider_multiple_mappings() {
        let config = parse_custom_oidc_provider(
            "issuer=https://auth.example.com,jwks_url=https://auth.example.com/jwks,audience=perfgate,claim=team,mapping=team-a:proj-a:viewer;team-b:proj-b:admin",
        ).unwrap();

        assert_eq!(config.repo_mappings.len(), 2);
        assert!(config.repo_mappings.contains_key("team-a"));
        assert!(config.repo_mappings.contains_key("team-b"));
    }

    #[test]
    fn test_parse_custom_oidc_provider_missing_fields() {
        // Missing issuer
        assert!(
            parse_custom_oidc_provider("jwks_url=u,audience=a,claim=c,mapping=i:p:admin").is_err()
        );
        // Missing claim
        assert!(
            parse_custom_oidc_provider("issuer=i,jwks_url=u,audience=a,mapping=i:p:admin").is_err()
        );
        // Missing mapping
        assert!(parse_custom_oidc_provider("issuer=i,jwks_url=u,audience=a,claim=c").is_err());
    }

    #[test]
    fn test_parse_custom_oidc_provider_unknown_key() {
        assert!(
            parse_custom_oidc_provider(
                "issuer=i,jwks_url=u,audience=a,claim=c,mapping=i:p:admin,unknown=val"
            )
            .is_err()
        );
    }

    #[test]
    fn test_cli_args_retention() {
        let args = Args::try_parse_from([
            "perfgate-server",
            "--retention-days",
            "30",
            "--cleanup-interval-hours",
            "6",
        ])
        .unwrap();

        assert_eq!(args.retention_days, 30);
        assert_eq!(args.cleanup_interval_hours, 6);
    }
}
