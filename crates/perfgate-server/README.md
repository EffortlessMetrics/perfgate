# perfgate-server

Centralized baseline management for teams that run benchmarks across multiple CI runners.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](../../LICENSE-MIT)

## Why

Performance baselines live on individual CI runners. When different runners execute benchmarks, they each see different baselines -- or none at all. Promoting, versioning, and auditing baselines becomes a manual chore that does not scale.

`perfgate-server` is a REST API that stores baselines centrally so every CI job, repository, and team member works from the same source of truth. It ships as a single binary with built-in storage, auth, and a web dashboard.

## Quick start

```bash
cargo install perfgate-server

# SQLite (recommended for production)
perfgate-server --storage-type sqlite --database-url ./perfgate.db \
  --api-keys admin:pg_live_<your-key>

# In-memory (development / demos)
perfgate-server
```

## Feature highlights

| Feature | Details |
|---------|---------|
| **Storage backends** | In-memory, SQLite, PostgreSQL |
| **Artifact offload** | S3, GCS, and Azure Blob via `--artifacts-url` |
| **Auth** | API keys (scoped to project + benchmark regex), JWT (HS256), GitHub Actions OIDC |
| **Role-based access** | Viewer, Contributor, Promoter, Admin |
| **Web dashboard** | Embedded SPA served at `/` -- no extra deployment needed |
| **Fleet analytics** | Dependency-change impact tracking and cross-project alerts |
| **Verdict history** | Record and query pass/warn/fail verdicts over time |
| **Observability** | Structured JSON logging, request IDs, `/health` endpoint |
| **Graceful shutdown** | Handles SIGTERM / Ctrl-C cleanly |

## REST API

All data endpoints live under `/api/v1`. The health check and dashboard are at the root.

| Method | Path | Auth | Description |
|--------|------|:----:|-------------|
| `GET` | `/health` | -- | Health check with storage status |
| `GET` | `/` | -- | Web dashboard |
| `POST` | `/api/v1/projects/{project}/baselines` | Y | Upload a baseline |
| `GET` | `/api/v1/projects/{project}/baselines` | Y | List baselines (filterable) |
| `GET` | `/api/v1/projects/{project}/baselines/{bench}/latest` | Y | Get latest baseline |
| `GET` | `/api/v1/projects/{project}/baselines/{bench}/versions/{ver}` | Y | Get specific version |
| `DELETE` | `/api/v1/projects/{project}/baselines/{bench}/versions/{ver}` | Y | Soft-delete a version |
| `POST` | `/api/v1/projects/{project}/baselines/{bench}/promote` | Y | Promote a version |
| `POST` | `/api/v1/projects/{project}/verdicts` | Y | Submit a verdict |
| `GET` | `/api/v1/projects/{project}/verdicts` | Y | List verdicts |
| `POST` | `/api/v1/fleet/dependency-event` | Y | Record dependency change events |
| `GET` | `/api/v1/fleet/alerts` | Y | List fleet-wide alerts |
| `GET` | `/api/v1/fleet/dependency/{dep}/impact` | Y | Query dependency impact |

## Authentication

Pass an API key as a Bearer token (`Authorization: Bearer pg_live_<32-char-random>`).
Keys are scoped to a project and optionally restricted by benchmark regex:

```bash
--api-keys contributor:pg_live_abc123:my-project:^bench-.*$
```

For GitHub Actions CI, use OIDC (`--github-oidc org/repo:project-id:contributor`).
JWT tokens (HS256) are also supported via `--jwt-secret`.

## Configuration

| Flag | Default | Description |
|------|---------|-------------|
| `--bind` | `0.0.0.0` | Bind address |
| `--port` | `8080` | Port |
| `--storage-type` | `memory` | `memory`, `sqlite`, or `postgres` |
| `--database-url` | -- | DB path (SQLite) or connection string (Postgres) |
| `--artifacts-url` | -- | Object-store URL (`s3://...`, `gs://...`, `az://...`) |
| `--api-keys` | -- | `role:key[:project[:benchmark_regex]]` (repeatable) |
| `--github-oidc` | -- | `org/repo:project_id:role` (repeatable) |
| `--jwt-secret` | -- | HS256 secret for JWT auth |
| `--no-cors` | `false` | Disable CORS |
| `--timeout` | `30` | Request timeout (seconds) |
| `--log-level` | `info` | `trace`, `debug`, `info`, `warn`, `error` |
| `--log-format` | `json` | `json` or `pretty` |

## Storage backends

| Backend | Use case | Persistence | Setup |
|---------|----------|:-----------:|-------|
| **memory** | Dev / tests | None | Zero config |
| **sqlite** | Single-node production | Disk | `--database-url ./perfgate.db` |
| **postgres** | Multi-node / HA | Disk | `--database-url postgresql://host/db` |

Artifact payloads (run receipts) can be offloaded to S3, GCS, or Azure Blob Storage independently of the metadata backend.

## Library usage

```rust
use perfgate_server::{ServerConfig, StorageBackend, run_server};

#[tokio::main]
async fn main() {
    let config = ServerConfig::new()
        .bind("0.0.0.0:8080").unwrap()
        .storage_backend(StorageBackend::Sqlite)
        .sqlite_path("perfgate.db");
    run_server(config).await.unwrap();
}
```

See also: [Getting Started with Baseline Server](../../docs/GETTING_STARTED_BASELINE_SERVER.md)

## License

MIT OR Apache-2.0
