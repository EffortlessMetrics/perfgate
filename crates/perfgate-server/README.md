# perfgate-server

REST API server for centralized baseline management.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](../../LICENSE-MIT)

## Overview

perfgate-server provides a centralized service for storing and managing performance baselines. It enables teams to:

- Share baselines across multiple repositories and CI runners
- Track baseline version history with rich metadata
- Control access with role-based permissions
- Scale to fleet-level performance management

**Documentation:** [Getting Started with Baseline Server](../../docs/GETTING_STARTED_BASELINE_SERVER.md)

## Features

- **Multi-tenancy**: Projects/namespaces for isolation
- **Version history**: Track baseline versions over time
- **Rich metadata**: Git refs, tags, custom metadata
- **Access control**: Role-based permissions (Viewer, Contributor, Promoter, Admin)
- **Multiple backends**: In-memory (dev), SQLite (production), PostgreSQL (planned)

## Installation

```bash
cargo install perfgate-server
```

## Usage

### Start server with in-memory storage (development)

```bash
perfgate-server
```

### Start with SQLite storage (production)

```bash
perfgate-server --storage-type sqlite --database-url ./perfgate.db
```

### Specify bind address and port

```bash
perfgate-server --bind 127.0.0.1 --port 3000
```

### Add API keys

```bash
perfgate-server --api-keys admin:pg_live_abc123def456... --api-keys viewer:pg_live_xyz789...
```

## CLI Options

| Option | Default | Description |
|--------|---------|-------------|
| `--bind` | `0.0.0.0` | Bind address |
| `--port` | `8080` | Port number |
| `--storage-type` | `memory` | Storage backend: memory, sqlite, postgres |
| `--database-url` | - | Database URL/path |
| `--api-keys` | - | API keys in format "role:key" |
| `--no-cors` | false | Disable CORS |
| `--timeout` | `30` | Request timeout in seconds |
| `--log-level` | `info` | Log level: trace, debug, info, warn, error |
| `--log-format` | `json` | Log format: json, pretty |

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/projects/{project}/baselines` | Upload a baseline |
| `GET` | `/projects/{project}/baselines/{benchmark}/latest` | Get latest baseline |
| `GET` | `/projects/{project}/baselines/{benchmark}/versions/{version}` | Get specific version |
| `GET` | `/projects/{project}/baselines` | List baselines |
| `DELETE` | `/projects/{project}/baselines/{benchmark}/versions/{version}` | Delete baseline |
| `POST` | `/projects/{project}/baselines/{benchmark}/promote` | Promote version |
| `GET` | `/health` | Health check |

## Authentication

API requests require an API key in the `Authorization` header:

```
Authorization: Bearer pg_live_abc123def456...
```

### API Key Format

- Live keys: `pg_live_<32-char-random>`
- Test keys: `pg_test_<32-char-random>`

### Roles

| Role | Permissions |
|------|-------------|
| `viewer` | Read-only access |
| `contributor` | Upload and read baselines |
| `promoter` | Upload, read, and promote baselines |
| `admin` | Full access including delete |

## Example: Upload a Baseline

```bash
curl -X POST http://localhost:8080/projects/my-project/baselines \
  -H "Authorization: Bearer pg_live_abc123..." \
  -H "Content-Type: application/json" \
  -d '{
    "benchmark": "my-bench",
    "version": "v1.0.0",
    "git_ref": "refs/heads/main",
    "git_sha": "abc123def456",
    "receipt": { ... }
  }'
```

## Example: Get Latest Baseline

```bash
curl http://localhost:8080/projects/my-project/baselines/my-bench/latest \
  -H "Authorization: Bearer pg_live_abc123..."
```

## Library Usage

```rust
use perfgate_server::{ServerConfig, StorageBackend, run_server};

#[tokio::main]
async fn main() {
    let config = ServerConfig::new()
        .bind("0.0.0.0:8080").unwrap()
        .storage_backend(StorageBackend::Sqlite)
        .sqlite_path("perfgate.db")
        .api_key("pg_live_abc123...", Role::Admin);

    run_server(config).await.unwrap();
}
```

## License

MIT OR Apache-2.0
