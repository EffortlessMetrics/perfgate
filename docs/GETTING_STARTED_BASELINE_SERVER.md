# Getting Started with Baseline Server

This guide covers how to set up and use the perfgate Baseline Server for centralized baseline management.

## Overview

The perfgate Baseline Server provides:

- **Centralized storage**: Store baselines in a central location accessible by all CI runners
- **Version history**: Track changes to baselines over time
- **Access control**: Role-based permissions for uploading, promoting, and deleting baselines
- **Multi-tenancy**: Isolate baselines by project/namespace
- **Rich metadata**: Git refs, SHAs, tags, and custom metadata

## Prerequisites

- Rust 1.92+ (for building from source)
- SQLite 3.x (for production storage)

## Installation

### Build from Source

```bash
# Build the server
cargo build --release -p perfgate-server

# The binary will be at target/release/perfgate-server
```

### Install with Cargo

```bash
cargo install --path crates/perfgate-server
```

## Starting the Server

### Development Mode (In-Memory Storage)

For quick testing and development:

```bash
perfgate-server
```

This starts the server with:
- In-memory storage (data lost on restart)
- No authentication
- Default bind address `0.0.0.0:8080`

### Production Mode (SQLite Storage)

For persistent storage:

```bash
perfgate-server \
  --storage-type sqlite \
  --database-url ./perfgate.db \
  --bind 127.0.0.1 \
  --port 8080
```

### With Authentication

Add API keys for authentication:

```bash
perfgate-server \
  --storage-type sqlite \
  --database-url ./perfgate.db \
  --api-keys admin:pg_live_abc123def456... \
  --api-keys viewer:pg_live_xyz789...
```

The format is `role:key` where:
- `role`: One of `viewer`, `contributor`, `promoter`, `admin`
- `key`: API key in format `pg_live_<32-chars>` or `pg_test_<32-chars>`

## CLI Options

| Option | Default | Description |
|--------|---------|-------------|
| `--bind` | `0.0.0.0` | Bind address |
| `--port` | `8080` | Port number |
| `--storage-type` | `memory` | Storage backend: `memory`, `sqlite` |
| `--database-url` | - | Database path (required for sqlite) |
| `--api-keys` | - | API keys in format `role:key` (can be repeated) |
| `--no-cors` | false | Disable CORS headers |
| `--timeout` | `30` | Request timeout in seconds |
| `--log-level` | `info` | Log level: `trace`, `debug`, `info`, `warn`, `error` |
| `--log-format` | `json` | Log format: `json`, `pretty` |

## Configuring the CLI

### Environment Variables

Set environment variables to configure the perfgate CLI:

```bash
export PERFGATE_SERVER_URL=http://localhost:8080
export PERFGATE_API_KEY=pg_live_abc123def456...
export PERFGATE_PROJECT=my-project
```

### Per-Command Configuration

Or specify options per command:

```bash
perfgate check --config perfgate.toml --bench my-bench \
  --baseline-server http://localhost:8080 \
  --api-key pg_live_abc123... \
  --project my-project
```

## Basic Workflow

### 1. Upload a Baseline

After running benchmarks on your main branch:

```bash
# Run benchmark
perfgate run --name my-bench --out run.json -- ./my-benchmark

# Upload to server
perfgate promote --current run.json \
  --to-server \
  --project my-project \
  --benchmark my-bench
```

Or use the `baseline` subcommand:

```bash
perfgate baseline upload \
  --baseline-server http://localhost:8080/api/v1 \
  --api-key pg_live_abc123... \
  --project my-project \
  --benchmark my-bench \
  --file run.json
```

### 2. Compare Against Server Baseline

In your CI pipeline:

```bash
# Run benchmark on PR
perfgate run --name my-bench --out run.json -- ./my-benchmark

# Compare against server baseline
perfgate check --config perfgate.toml --bench my-bench \
  --baseline-server http://localhost:8080 \
  --api-key pg_live_abc123... \
  --project my-project
```

### 3. Promote to Server

After merging to main:

```bash
# Run benchmark on merged code
perfgate run --name my-bench --out run.json -- ./my-benchmark

# Promote to server
perfgate promote --current run.json \
  --to-server \
  --project my-project \
  --benchmark my-bench
```

## Authentication Setup

### API Key Format

- **Live keys**: `pg_live_<32-char-random>` - for production use
- **Test keys**: `pg_test_<32-char-random>` - for testing/development

### Generating Keys

Generate secure random keys:

```bash
# Using openssl
openssl rand -hex 16

# Using Python
python3 -c "import secrets; print('pg_live_' + secrets.token_hex(16))"
```

### Roles and Permissions

| Role | Upload | Read | Promote | Delete |
|------|--------|------|---------|--------|
| `viewer` | ❌ | ✅ | ❌ | ❌ |
| `contributor` | ✅ | ✅ | ❌ | ❌ |
| `promoter` | ✅ | ✅ | ✅ | ❌ |
| `admin` | ✅ | ✅ | ✅ | ✅ |

### Example: Setting Up CI Access

1. **Create keys for different purposes:**

```bash
perfgate-server \
  --api-keys admin:pg_live_admin_key_here... \
  --api-keys promoter:pg_live_ci_promoter_key... \
  --api-keys viewer:pg_live_ci_viewer_key...
```

2. **Configure CI for PR checks (read-only):**

```yaml
# GitHub Actions
env:
  PERFGATE_SERVER_URL: https://perfgate.example.com
  PERFGATE_API_KEY: ${{ secrets.PERFGATE_VIEWER_KEY }}
  PERFGATE_PROJECT: my-project
```

3. **Configure CI for main branch (promote access):**

```yaml
# GitHub Actions (on push to main)
env:
  PERFGATE_SERVER_URL: https://perfgate.example.com
  PERFGATE_API_KEY: ${{ secrets.PERFGATE_PROMOTER_KEY }}
  PERFGATE_PROJECT: my-project
```

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

### Example API Calls

**Upload a baseline:**

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

**Get latest baseline:**

```bash
curl http://localhost:8080/projects/my-project/baselines/my-bench/latest \
  -H "Authorization: Bearer pg_live_abc123..."
```

**List baselines:**

```bash
curl "http://localhost:8080/projects/my-project/baselines?limit=10" \
  -H "Authorization: Bearer pg_live_abc123..."
```

## Fallback Behavior

The perfgate client supports automatic fallback to local storage when the server is unavailable:

```bash
# With fallback to local baselines/ directory
perfgate check --config perfgate.toml --bench my-bench \
  --baseline-server http://localhost:8080 \
  --fallback-dir ./baselines
```

This ensures CI pipelines continue to work even if the baseline server is temporarily unavailable.

## Deployment Examples

### Docker

```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release -p perfgate-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y sqlite3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/perfgate-server /usr/local/bin/
EXPOSE 8080
CMD ["perfgate-server", "--storage-type", "sqlite", "--database-url", "/data/perfgate.db"]
```

### Docker Compose

```yaml
version: '3.8'
services:
  perfgate-server:
    build: .
    ports:
      - "8080:8080"
    volumes:
      - perfgate-data:/data
    environment:
      - PERFGATE_API_KEYS=admin:pg_live_your_key_here
    command: >
      perfgate-server
      --storage-type sqlite
      --database-url /data/perfgate.db
      --api-keys admin:pg_live_your_key_here

volumes:
  perfgate-data:
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: perfgate-server
spec:
  replicas: 1
  selector:
    matchLabels:
      app: perfgate-server
  template:
    metadata:
      labels:
        app: perfgate-server
    spec:
      containers:
      - name: perfgate-server
        image: perfgate-server:latest
        ports:
        - containerPort: 8080
        args:
        - --storage-type
        - sqlite
        - --database-url
        - /data/perfgate.db
        volumeMounts:
        - name: data
          mountPath: /data
        env:
        - name: PERFGATE_API_KEYS
          valueFrom:
            secretKeyRef:
              name: perfgate-secrets
              key: api-keys
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: perfgate-data
```

## Monitoring

### Health Check

```bash
curl http://localhost:8080/health
```

Response:
```json
{
  "status": "healthy",
  "version": "2.0.0",
  "storage": {
    "backend": "sqlite",
    "status": "healthy"
  }
}
```

### Logging

The server outputs structured JSON logs:

```bash
# Pretty format for development
perfgate-server --log-format pretty --log-level debug

# JSON format for production (default)
perfgate-server --log-format json --log-level info
```

## Troubleshooting

### Connection Refused

Ensure the server is running and the bind address is correct:

```bash
# Check if server is listening
netstat -tlnp | grep 8080

# Try with curl
curl http://localhost:8080/health
```

### Authentication Failed

Verify the API key format and role:

```bash
# Key format should be: pg_live_<32-chars>
# Role must be one of: viewer, contributor, promoter, admin
```

### Database Errors

For SQLite, ensure the database file is writable:

```bash
# Check permissions
ls -la perfgate.db

# Create database directory if needed
mkdir -p /data && touch /data/perfgate.db
```

## Related Documentation

- [Architecture Overview](ARCHITECTURE.md)
- [perfgate-server README](../crates/perfgate-server/README.md)
- [perfgate-client README](../crates/perfgate-client/README.md)
- [Main README](../README.md)
