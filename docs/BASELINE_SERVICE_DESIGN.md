# perfgate v2.0 Baseline Service API Design

This document describes the architecture and API design for the perfgate v2.0 Baseline Service, enabling centralized baseline management for fleet-scale performance monitoring.

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [API Specification](#2-api-specification)
3. [Data Models and Schemas](#3-data-models-and-schemas)
4. [Authentication and Authorization](#4-authentication-and-authorization)
5. [Client Integration Design](#5-client-integration-design)
6. [Configuration Format](#6-configuration-format)
7. [Migration Path from v1.x](#7-migration-path-from-v1x)

---

## 1. Architecture Overview

### 1.1 System Context

The Baseline Service provides centralized storage and management of performance baselines, replacing file-based and cloud-object storage with a purpose-built service that supports:

- Multi-tenancy (projects/namespaces)
- Version history and rollback
- Rich metadata filtering
- Access control and audit logging
- High availability and scalability

### 1.2 High-Level Architecture

```mermaid
flowchart TB
    subgraph Client Layer
        CLI[perfgate CLI]
        CLIENT[perfgate-client lib]
    end

    subgraph Service Layer
        SERVER[perfgate-server]
        AUTH[Auth Middleware]
        API[REST API]
    end

    subgraph Storage Layer
        MEMORY[(In-Memory)]
        SQLITE[(SQLite)]
        POSTGRES[(PostgreSQL)]
    end

    CLI --> CLIENT
    CLIENT --> AUTH
    AUTH --> API
    API --> SERVER
    SERVER --> MEMORY
    SERVER --> SQLITE
    SERVER --> POSTGRES
```

### 1.3 Component Architecture

```mermaid
flowchart LR
    subgraph perfgate-server
        direction TB
        HTTP[HTTP Server<br/>axum/warp]
        ROUTES[Route Handlers]
        AUTHZ[Authorization Layer]
        SERVICE[Service Layer]
        STORE[Storage Trait]
    end

    subgraph Storage Backends
        MEM[MemoryStore]
        SQL[SqliteStore]
        PG[PostgresStore]
    end

    HTTP --> ROUTES
    ROUTES --> AUTHZ
    AUTHZ --> SERVICE
    SERVICE --> STORE
    STORE --> MEM
    STORE --> SQL
    STORE --> PG
```

### 1.4 Crate Dependencies

The new architecture extends the existing clean architecture with two new crates:

```
perfgate-types (existing)
       ↓
perfgate-domain (existing)
       ↓
perfgate-adapters (existing)
       ↓
perfgate-client (NEW) ←──→ perfgate-server (NEW)
       ↓
perfgate-app (existing)
       ↓
perfgate-cli (existing)
```

**New Crates:**

| Crate | Responsibility |
|-------|----------------|
| `perfgate-server` | REST API server, storage backends, multi-tenancy |
| `perfgate-client` | Rust client library, retry logic, fallback support |

### 1.5 Design Principles

1. **Backward Compatibility**: Existing file-based and cloud storage workflows MUST continue to work
2. **Graceful Degradation**: Client MUST fall back to local storage when server is unavailable
3. **Minimal Dependencies**: Server SHOULD be deployable with zero external dependencies (SQLite mode)
4. **Clean Architecture**: Server follows the same layered architecture as the core project
5. **Schema Reuse**: Use existing `RunReceipt` and related types from `perfgate-types`

---

## 2. API Specification

### 2.1 Base URL and Versioning

```
Base URL: https://perfgate.example.com/api/v1
Content-Type: application/json
```

All API responses include standard HTTP status codes and error envelopes.

### 2.2 Authentication Header

```
Authorization: Bearer <api-key>
```

Or for token-based auth:

```
Authorization: Token <token>
```

### 2.3 Endpoints

#### 2.3.1 Baseline CRUD Operations

##### Upload Baseline

```http
POST /api/v1/projects/{project}/baselines
Content-Type: application/json

{
  "benchmark": "my-bench",
  "version": "v1.0.0",
  "git_ref": "refs/heads/main",
  "git_sha": "abc123def456",
  "receipt": { ... RunReceipt ... },
  "metadata": {
    "ci_run_id": "12345",
    "triggered_by": "user@example.com"
  },
  "tags": ["production", "main-branch"]
}
```

**Response:**
```json
{
  "id": "bl_01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "benchmark": "my-bench",
  "version": "v1.0.0",
  "created_at": "2026-03-08T12:00:00Z",
  "etag": "\"sha256:abc123...\""
}
```

| Status | Meaning |
|--------|---------|
| 201 | Created successfully |
| 400 | Invalid request body |
| 401 | Authentication required |
| 403 | No write permission |
| 409 | Version already exists |

##### Download Baseline

```http
GET /api/v1/projects/{project}/baselines/{benchmark}/latest
GET /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}
GET /api/v1/projects/{project}/baselines/{benchmark}?git_ref=refs/heads/main
```

**Response:**
```json
{
  "id": "bl_01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "benchmark": "my-bench",
  "version": "v1.0.0",
  "git_ref": "refs/heads/main",
  "git_sha": "abc123def456",
  "created_at": "2026-03-08T12:00:00Z",
  "receipt": { ... RunReceipt ... },
  "metadata": {},
  "tags": ["production"]
}
```

| Status | Meaning |
|--------|---------|
| 200 | Success |
| 401 | Authentication required |
| 403 | No read permission |
| 404 | Baseline not found |

##### List Baselines

```http
GET /api/v1/projects/{project}/baselines
GET /api/v1/projects/{project}/baselines?benchmark_prefix=my-
GET /api/v1/projects/{project}/baselines?git_ref=refs/heads/main
GET /api/v1/projects/{project}/baselines?tags=production
GET /api/v1/projects/{project}/baselines?since=2026-01-01T00:00:00Z
GET /api/v1/projects/{project}/baselines?limit=50&offset=0
```

**Query Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `benchmark` | string | Exact benchmark name match |
| `benchmark_prefix` | string | Benchmark name prefix filter |
| `git_ref` | string | Git reference filter (supports glob patterns) |
| `git_sha` | string | Exact git SHA filter |
| `tags` | string[] | Filter by tags (comma-separated, AND logic) |
| `since` | timestamp | Filter baselines created after this time |
| `until` | timestamp | Filter baselines created before this time |
| `limit` | integer | Maximum results (default: 50, max: 200) |
| `offset` | integer | Pagination offset |
| `include_receipt` | boolean | Include full receipt in response (default: false) |

**Response:**
```json
{
  "baselines": [
    {
      "id": "bl_01ARZ3NDEKTSV4RRFFQ69G5FAV",
      "benchmark": "my-bench",
      "version": "v1.0.0",
      "git_ref": "refs/heads/main",
      "created_at": "2026-03-08T12:00:00Z",
      "tags": ["production"]
    }
  ],
  "pagination": {
    "total": 127,
    "limit": 50,
    "offset": 0,
    "has_more": true
  }
}
```

##### Delete Baseline

```http
DELETE /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}
```

**Response:**
```json
{
  "deleted": true,
  "id": "bl_01ARZ3NDEKTSV4RRFFQ69G5FAV"
}
```

| Status | Meaning |
|--------|---------|
| 200 | Deleted successfully |
| 401 | Authentication required |
| 403 | No delete permission |
| 404 | Baseline not found |

##### Promote Baseline (Atomic Update)

```http
POST /api/v1/projects/{project}/baselines/{benchmark}/promote
Content-Type: application/json

{
  "from_version": "v1.0.0-rc1",
  "to_version": "v1.0.0",
  "git_ref": "refs/heads/main",
  "git_sha": "abc123def456",
  "normalize": true
}
```

**Response:**
```json
{
  "id": "bl_01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "benchmark": "my-bench",
  "version": "v1.0.0",
  "promoted_from": "v1.0.0-rc1",
  "created_at": "2026-03-08T12:00:00Z"
}
```

#### 2.3.2 Batch Operations

##### Batch Upload

```http
POST /api/v1/projects/{project}/baselines:batchUpload
Content-Type: application/json

{
  "baselines": [
    {
      "benchmark": "bench-1",
      "version": "v1.0.0",
      "receipt": { ... }
    },
    {
      "benchmark": "bench-2",
      "version": "v1.0.0",
      "receipt": { ... }
    }
  ]
}
```

**Response:**
```json
{
  "results": [
    { "benchmark": "bench-1", "status": "created", "id": "bl_..." },
    { "benchmark": "bench-2", "status": "created", "id": "bl_..." }
  ],
  "summary": {
    "total": 2,
    "created": 2,
    "failed": 0
  }
}
```

##### Batch Download

```http
POST /api/v1/projects/{project}/baselines:batchGet
Content-Type: application/json

{
  "benchmarks": ["bench-1", "bench-2", "bench-3"],
  "version": "latest"
}
```

**Response:**
```json
{
  "baselines": [
    { "benchmark": "bench-1", "status": "found", "receipt": { ... } },
    { "benchmark": "bench-2", "status": "found", "receipt": { ... } },
    { "benchmark": "bench-3", "status": "not_found" }
  ]
}
```

#### 2.3.3 Version History

##### List Versions

```http
GET /api/v1/projects/{project}/baselines/{benchmark}/versions
```

**Response:**
```json
{
  "versions": [
    {
      "version": "v1.0.0",
      "created_at": "2026-03-08T12:00:00Z",
      "git_ref": "refs/heads/main",
      "git_sha": "abc123",
      "is_current": true
    },
    {
      "version": "v1.0.0-rc1",
      "created_at": "2026-03-01T12:00:00Z",
      "git_ref": "refs/heads/release/v1.0",
      "git_sha": "def456",
      "is_current": false
    }
  ],
  "pagination": { ... }
}
```

##### Rollback Version

```http
POST /api/v1/projects/{project}/baselines/{benchmark}/rollback
Content-Type: application/json

{
  "to_version": "v0.9.0",
  "reason": "Regression detected in v1.0.0"
}
```

#### 2.3.4 Health and Metadata

##### Health Check

```http
GET /api/v1/health
```

**Response:**
```json
{
  "status": "healthy",
  "version": "2.0.0",
  "storage": {
    "backend": "postgresql",
    "status": "connected"
  }
}
```

##### Server Info

```http
GET /api/v1/info
```

**Response:**
```json
{
  "version": "2.0.0",
  "api_versions": ["v1"],
  "features": {
    "batch_operations": true,
    "versioning": true,
    "audit_log": true
  }
}
```

### 2.4 Error Response Format

All error responses follow a consistent format:

```json
{
  "error": {
    "code": "BASELINE_NOT_FOUND",
    "message": "Baseline 'my-bench' not found in project 'my-project'",
    "details": {
      "project": "my-project",
      "benchmark": "my-bench"
    },
    "request_id": "req_01ARZ3NDEKTSV4RRFFQ69G5FAV"
  }
}
```

**Standard Error Codes:**

| Code | HTTP Status | Description |
|------|-------------|-------------|
| `UNAUTHORIZED` | 401 | Missing or invalid authentication |
| `FORBIDDEN` | 403 | Insufficient permissions |
| `NOT_FOUND` | 404 | Resource not found |
| `ALREADY_EXISTS` | 409 | Resource already exists |
| `VALIDATION_ERROR` | 400 | Invalid request body |
| `RATE_LIMITED` | 429 | Too many requests |
| `INTERNAL_ERROR` | 500 | Server error |

---

## 3. Data Models and Schemas

### 3.1 Core Models

#### BaselineRecord

The primary storage model for baselines:

```rust
/// Schema: perfgate.baseline.v1
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BaselineRecord {
    /// Schema identifier
    pub schema: String,
    
    /// Unique baseline identifier (ULID)
    pub id: String,
    
    /// Project/namespace identifier
    pub project: String,
    
    /// Benchmark name (must match perfgate-types validation)
    pub benchmark: String,
    
    /// Semantic version for this baseline
    pub version: String,
    
    /// Git reference (branch, tag, or ref)
    pub git_ref: Option<String>,
    
    /// Git commit SHA
    pub git_sha: Option<String>,
    
    /// Full run receipt (perfgate.run.v1)
    pub receipt: RunReceipt,
    
    /// User-provided metadata
    pub metadata: BTreeMap<String, String>,
    
    /// Tags for filtering
    pub tags: Vec<String>,
    
    /// Creation timestamp (RFC 3339)
    pub created_at: String,
    
    /// Last modification timestamp
    pub updated_at: String,
    
    /// Content hash for ETag/optimistic locking
    pub content_hash: String,
    
    /// Creation source (upload, promote, migrate)
    pub source: BaselineSource,
    
    /// Soft delete flag
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSource {
    Upload,
    Promote,
    Migrate,
    Rollback,
}
```

#### BaselineVersion

Version history metadata (without full receipt):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BaselineVersion {
    pub version: String,
    pub git_ref: Option<String>,
    pub git_sha: Option<String>,
    pub created_at: String,
    pub created_by: Option<String>,
    pub is_current: bool,
    pub source: BaselineSource,
}
```

#### Project

Multi-tenancy namespace:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    /// Schema identifier
    pub schema: String,
    
    /// Project identifier (URL-safe)
    pub id: String,
    
    /// Display name
    pub name: String,
    
    /// Project description
    pub description: Option<String>,
    
    /// Creation timestamp
    pub created_at: String,
    
    /// Retention policy
    pub retention: RetentionPolicy,
    
    /// Default baseline versioning strategy
    pub versioning: VersioningStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RetentionPolicy {
    /// Maximum versions to keep per benchmark
    pub max_versions: Option<u32>,
    
    /// Delete baselines older than this many days
    pub max_age_days: Option<u32>,
    
    /// Keep versions with these tags indefinitely
    pub preserve_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VersioningStrategy {
    Semantic,
    Timestamp,
    GitRef,
}
```

### 3.2 API Request/Response Types

#### UploadBaselineRequest

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadBaselineRequest {
    /// Benchmark name
    pub benchmark: String,
    
    /// Version identifier (defaults to timestamp if not provided)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    
    /// Git reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    
    /// Git commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    
    /// Run receipt (perfgate.run.v1)
    pub receipt: RunReceipt,
    
    /// Optional metadata
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    
    /// Optional tags
    #[serde(default)]
    pub tags: Vec<String>,
    
    /// Normalize receipt before storing (strip run_id, timestamps)
    #[serde(default)]
    pub normalize: bool,
}
```

#### UploadBaselineResponse

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadBaselineResponse {
    pub id: String,
    pub benchmark: String,
    pub version: String,
    pub created_at: String,
    pub etag: String,
}
```

#### ListBaselinesResponse

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListBaselinesResponse {
    pub baselines: Vec<BaselineSummary>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BaselineSummary {
    pub id: String,
    pub benchmark: String,
    pub version: String,
    pub git_ref: Option<String>,
    pub created_at: String,
    pub tags: Vec<String>,
    
    /// Only included when include_receipt=true
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<RunReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaginationInfo {
    pub total: u64,
    pub limit: u32,
    pub offset: u64,
    pub has_more: bool,
}
```

### 3.3 JSON Schema

All new schemas follow the existing pattern with `schema` field:

| Schema | Identifier | Purpose |
|--------|------------|---------|
| BaselineRecord | `perfgate.baseline.v1` | Stored baseline with metadata |
| Project | `perfgate.project.v1` | Project/namespace configuration |

Schemas are generated via `schemars` and stored in `schemas/` directory for contract testing.

---

## 4. Authentication and Authorization

### 4.1 Authentication Methods

The service supports multiple authentication methods:

#### 4.1.1 API Keys

Static keys for service-to-service authentication:

```rust
#[derive(Debug, Clone)]
pub struct ApiKey {
    pub id: String,
    pub key_hash: String,  // SHA-256 of key
    pub name: String,
    pub project_id: String,
    pub scopes: Vec<Scope>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Read,
    Write,
    Promote,
    Delete,
    Admin,
}
```

**Key Format:** `pg_live_<32-char-random>` or `pg_test_<32-char-random>`

**Header:** `Authorization: Bearer pg_live_abc123...`

#### 4.1.2 JWT Tokens

Short-lived tokens for CI/CD pipelines:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub iss: String,        // Issuer
    pub sub: String,        // Subject (user or service)
    pub aud: String,        // Audience (project ID)
    pub exp: i64,           // Expiration
    pub iat: i64,           // Issued at
    pub scopes: Vec<Scope>,
    pub project_id: String,
}
```

**Header:** `Authorization: Token <jwt>`

#### 4.1.3 GitHub OIDC (Future)

Integration with GitHub Actions OIDC for passwordless authentication:

```yaml
# .github/workflows/perf.yml
permissions:
  id-token: write
  contents: read

jobs:
  perf:
    steps:
      - uses: perfgate/action@v2
        with:
          server: https://perfgate.example.com
          oidc: true  # Uses GitHub OIDC token
```

### 4.2 Authorization Model

#### 4.2.1 Role-Based Access Control (RBAC)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Read-only access to baselines
    Viewer,
    
    /// Can upload and read baselines
    Contributor,
    
    /// Can promote baselines to production
    Promoter,
    
    /// Full access including delete
    Admin,
}

impl Role {
    pub fn allowed_scopes(&self) -> Vec<Scope> {
        match self {
            Role::Viewer => vec![Scope::Read],
            Role::Contributor => vec![Scope::Read, Scope::Write],
            Role::Promoter => vec![Scope::Read, Scope::Write, Scope::Promote],
            Role::Admin => vec![Scope::Read, Scope::Write, Scope::Promote, Scope::Delete, Scope::Admin],
        }
    }
}
```

#### 4.2.2 Permission Matrix

| Action | Viewer | Contributor | Promoter | Admin |
|--------|--------|-------------|----------|-------|
| List baselines | ✅ | ✅ | ✅ | ✅ |
| Download baseline | ✅ | ✅ | ✅ | ✅ |
| Upload baseline | ❌ | ✅ | ✅ | ✅ |
| Promote baseline | ❌ | ❌ | ✅ | ✅ |
| Delete baseline | ❌ | ❌ | ❌ | ✅ |
| Manage API keys | ❌ | ❌ | ❌ | ✅ |
| View audit log | ❌ | ❌ | ❌ | ✅ |

#### 4.2.3 Project-Level Isolation

Each project is isolated with its own:

- API keys and tokens
- Baseline namespace
- User roles
- Retention policies

Cross-project access requires explicit configuration.

### 4.3 Audit Logging

All mutations are logged:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub timestamp: String,
    pub project_id: String,
    pub action: AuditAction,
    pub resource_type: String,
    pub resource_id: String,
    pub actor: Actor,
    pub details: BTreeMap<String, String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Create,
    Update,
    Delete,
    Promote,
    Rollback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub kind: ActorKind,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    ApiKey,
    User,
    Service,
}
```

---

## 5. Client Integration Design

### 5.1 perfgate-client Crate

A new crate `perfgate-client` provides the Rust client library:

```rust
// crates/perfgate-client/src/lib.rs

pub struct BaselineClient {
    config: ClientConfig,
    http: reqwest::Client,
    fallback: Option<FallbackStorage>,
}

pub struct ClientConfig {
    /// Server URL (e.g., "https://perfgate.example.com/api/v1")
    pub server_url: String,
    
    /// Authentication method
    pub auth: AuthMethod,
    
    /// Project ID
    pub project: String,
    
    /// Request timeout (default: 30s)
    pub timeout: Duration,
    
    /// Retry configuration
    pub retry: RetryConfig,
    
    /// Fallback storage when server unavailable
    pub fallback: Option<FallbackStorage>,
}

pub enum AuthMethod {
    ApiKey(String),
    Token(String),
    None,
}

pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_retries: u32,
    
    /// Base delay between retries
    pub base_delay: Duration,
    
    /// Maximum delay between retries
    pub max_delay: Duration,
    
    /// Retry on these status codes
    pub retry_status_codes: Vec<u16>,
}

pub enum FallbackStorage {
    Local { dir: PathBuf },
    S3 { bucket: String, prefix: String },
    Gcs { bucket: String, prefix: String },
}
```

### 5.2 Client Operations

```rust
impl BaselineClient {
    /// Upload a baseline to the server
    pub async fn upload(
        &self,
        request: UploadBaselineRequest,
    ) -> Result<UploadBaselineResponse, ClientError>;
    
    /// Download a baseline from the server
    pub async fn download(
        &self,
        benchmark: &str,
        version: &BaselineVersion,
    ) -> Result<BaselineRecord, ClientError>;
    
    /// Download latest baseline for a benchmark
    pub async fn download_latest(
        &self,
        benchmark: &str,
    ) -> Result<BaselineRecord, ClientError>;
    
    /// List baselines with optional filters
    pub async fn list(
        &self,
        filters: ListFilters,
    ) -> Result<ListBaselinesResponse, ClientError>;
    
    /// Promote a baseline version
    pub async fn promote(
        &self,
        benchmark: &str,
        request: PromoteRequest,
    ) -> Result<BaselineRecord, ClientError>;
    
    /// Delete a baseline version
    pub async fn delete(
        &self,
        benchmark: &str,
        version: &str,
    ) -> Result<(), ClientError>;
    
    /// Batch download multiple baselines
    pub async fn batch_download(
        &self,
        benchmarks: &[&str],
    ) -> Result<Vec<BaselineRecord>, ClientError>;
    
    /// Check server health
    pub async fn health_check(&self) -> Result<HealthStatus, ClientError>;
}
```

### 5.3 Fallback Behavior

When the server is unavailable, the client falls back to configured storage:

```rust
impl BaselineClient {
    pub async fn download_with_fallback(
        &self,
        benchmark: &str,
    ) -> Result<BaselineRecord, ClientError> {
        // Try server first
        match self.download_latest(benchmark).await {
            Ok(record) => return Ok(record),
            Err(ClientError::ServerUnavailable(_)) => {
                // Fall back to local/cloud storage
                if let Some(fallback) = &self.fallback {
                    return self.download_from_fallback(benchmark, fallback).await;
                }
            }
            Err(e) => return Err(e),
        }
        
        Err(ClientError::NoFallbackAvailable)
    }
}
```

### 5.4 CLI Integration

The CLI is extended with server-aware commands:

#### Modified Commands

##### `perfgate run`

```bash
# Upload result to server after run
perfgate run --name my-bench --upload --out out.json -- ./bench.sh

# Upload with version and tags
perfgate run --name my-bench --upload \
    --baseline-version v1.0.0 \
    --baseline-tag production \
    --out out.json -- ./bench.sh
```

##### `perfgate compare`

```bash
# Fetch baseline from server
perfgate compare --baseline-server --current out.json --out compare.json

# With explicit version
perfgate compare --baseline-server --baseline-version v1.0.0 --current out.json
```

##### `perfgate promote`

```bash
# Promote to server
perfgate promote --current out.json --to-server

# Promote with version
perfgate promote --current out.json --to-server --version v1.0.0
```

##### `perfgate check`

```bash
# Use server for baselines (config-driven)
perfgate check --config perfgate.toml --bench my-bench --baseline-server
```

#### New Commands

##### `perfgate baseline`

Dedicated baseline management commands:

```bash
# List baselines
perfgate baseline list --project my-project

# Download baseline
perfgate baseline download --benchmark my-bench --out baseline.json

# Upload baseline
perfgate baseline upload --benchmark my-bench --receipt run.json

# Delete baseline version
perfgate baseline delete --benchmark my-bench --version v0.9.0

# View version history
perfgate baseline history --benchmark my-bench

# Rollback to previous version
perfgate baseline rollback --benchmark my-bench --to v0.9.0
```

---

## 6. Configuration Format

### 6.1 Server Configuration

```toml
# perfgate-server.toml

[server]
# Server bind address
host = "0.0.0.0"
port = 8080

# TLS configuration (optional)
tls_cert = "/etc/perfgate/cert.pem"
tls_key = "/etc/perfgate/key.pem"

# Request timeout (seconds)
timeout = 30

[storage]
# Storage backend: memory, sqlite, postgres
backend = "sqlite"

# SQLite configuration (used when backend = "sqlite")
[storage.sqlite]
path = "/var/lib/perfgate/perfgate.db"

# PostgreSQL configuration (used when backend = "postgres")
[storage.postgres]
url = "postgresql://user:pass@localhost/perfgate"
max_connections = 10
min_connections = 2

[auth]
# API key hashing algorithm
key_hash_algorithm = "sha256"

# JWT configuration (optional)
[auth.jwt]
issuer = "perfgate"
audience = "perfgate-api"
secret = "${JWT_SECRET}"  # Environment variable
expiration_seconds = 3600

[retention]
# Default retention policy
default_max_versions = 50
default_max_age_days = 365

[logging]
# Log level: trace, debug, info, warn, error
level = "info"

# Log format: json, pretty
format = "json"

# Audit log configuration
[logging.audit]
enabled = true
path = "/var/log/perfgate/audit.jsonl"
```

### 6.2 Client Configuration

Extended `perfgate.toml` format:

```toml
# perfgate.toml

[defaults]
repeat = 5
warmup = 1
threshold = 0.20
warn_factor = 0.90

# Legacy local baseline storage
baseline_dir = "baselines"

# NEW: Server configuration
[baseline_server]
# Enable server mode (default: false)
enabled = true

# Server URL
url = "https://perfgate.example.com/api/v1"

# Project ID
project = "my-project"

# Authentication
# Option 1: API key
api_key = "${PERFGATE_API_KEY}"

# Option 2: Token (JWT)
# token = "${PERFGATE_TOKEN}"

# Request timeout (seconds)
timeout = 30

# Retry configuration
[baseline_server.retry]
max_retries = 3
base_delay_ms = 100
max_delay_ms = 5000

# Fallback storage when server unavailable
[baseline_server.fallback]
# Fallback type: local, s3, gcs, none
type = "local"
dir = "baselines"

# Alternative: S3 fallback
# [baseline_server.fallback]
# type = "s3"
# bucket = "my-perfgate-baselines"
# prefix = "baselines/"
# region = "us-east-1"

# Upload behavior
[baseline_server.upload]
# Auto-upload after run
auto_upload = false

# Default version format: semantic, timestamp, git_ref
version_format = "git_ref"

# Default tags
tags = ["ci"]

[[bench]]
name = "my-bench"
command = ["./scripts/bench.sh"]
```

### 6.3 Environment Variables

| Variable | Description |
|----------|-------------|
| `PERFGATE_SERVER_URL` | Override server URL |
| `PERFGATE_API_KEY` | API key for authentication |
| `PERFGATE_TOKEN` | JWT token for authentication |
| `PERFGATE_PROJECT` | Default project ID |
| `PERFGATE_TIMEOUT` | Request timeout (seconds) |
| `PERFGATE_FALLBACK_DIR` | Fallback storage directory |

---

## 7. Migration Path from v1.x

### 7.1 Compatibility Mode

v2.0 maintains full backward compatibility with v1.x workflows:

1. **File-based baselines**: Continue to work without changes
2. **Cloud storage (S3/GCS)**: Continue to work via `object_store`
3. **Mixed mode**: Can use server for some projects, local for others

### 7.2 Migration Steps

#### Phase 1: Deploy Server (Non-Breaking)

1. Deploy `perfgate-server` with SQLite backend
2. Create projects and API keys
3. No changes to CI/CD pipelines required

#### Phase 2: Migrate Existing Baselines

Use the migration tool:

```bash
# Migrate local baselines to server
perfgate migrate \
    --from local:baselines/ \
    --to server:https://perfgate.example.com/api/v1 \
    --project my-project \
    --api-key $PERFGATE_API_KEY

# Migrate S3 baselines to server
perfgate migrate \
    --from s3://my-bucket/baselines/ \
    --to server:https://perfgate.example.com/api/v1 \
    --project my-project \
    --api-key $PERFGATE_API_KEY
```

Migration tool features:
- Preserves all metadata
- Creates version history
- Generates migration report
- Supports dry-run mode

#### Phase 3: Update CI/CD Configuration

Update `perfgate.toml`:

```toml
# Before (v1.x)
[defaults]
baseline_dir = "baselines"

# After (v2.0)
[defaults]
baseline_dir = "baselines"  # Keep as fallback

[baseline_server]
enabled = true
url = "https://perfgate.example.com/api/v1"
project = "my-project"
api_key = "${PERFGATE_API_KEY}"

[baseline_server.fallback]
type = "local"
dir = "baselines"
```

#### Phase 4: Validate and Cleanup

1. Run parallel mode (server + local) for validation period
2. Compare results to ensure consistency
3. Remove local baselines after confidence established
4. Disable fallback if desired

### 7.3 Migration Tool Architecture

```rust
// crates/perfgate-cli/src/commands/migrate.rs

pub struct MigrateCommand {
    pub from: MigrationSource,
    pub to: MigrationTarget,
    pub project: String,
    pub dry_run: bool,
    pub parallel: usize,
}

pub enum MigrationSource {
    Local { dir: PathBuf },
    S3 { bucket: String, prefix: String },
    Gcs { bucket: String, prefix: String },
}

pub enum MigrationTarget {
    Server { url: String, api_key: String },
}

impl MigrateCommand {
    pub async fn execute(&self) -> Result<MigrationReport, MigrationError> {
        // 1. Discover all baselines in source
        // 2. For each baseline:
        //    a. Parse and validate
        //    b. Upload to server with version
        //    c. Record success/failure
        // 3. Generate report
    }
}
```

### 7.4 Version Compatibility Matrix

| perfgate CLI | perfgate-server | Baseline Schema | Notes |
|--------------|-----------------|-----------------|-------|
| v1.x | N/A | perfgate.run.v1 | File/cloud storage only |
| v2.0 | v2.0 | perfgate.run.v1, perfgate.baseline.v1 | Server mode optional |
| v2.1+ | v2.0+ | Same | Full compatibility |

### 7.5 Rollback Plan

If issues arise during migration:

1. **Immediate**: Disable server in config (`baseline_server.enabled = false`)
2. **Fallback**: Client automatically uses local/cloud storage
3. **Recovery**: Server can export baselines back to file system

```bash
# Export from server to local files
perfgate export-baselines \
    --from server \
    --to local:baselines/ \
    --project my-project
```

---

## Appendix A: Implementation Roadmap

### Phase 1: Core Infrastructure (MVP)

- [ ] `perfgate-server` crate with axum HTTP server
- [ ] `perfgate-client` crate with async client
- [ ] In-memory storage backend
- [ ] SQLite storage backend
- [ ] Basic API key authentication
- [ ] CRUD endpoints for baselines
- [ ] CLI integration for `run`, `compare`, `promote`

### Phase 2: Production Features

- [ ] PostgreSQL storage backend
- [ ] JWT token authentication
- [ ] Role-based access control
- [ ] Version history and rollback
- [ ] Batch operations
- [ ] Audit logging
- [ ] Migration tool

### Phase 3: Advanced Features

- [ ] GitHub OIDC authentication
- [ ] Baseline comparison analytics
- [ ] Trend visualization API
- [ ] Webhook notifications
- [ ] Prometheus metrics endpoint
- [ ] Multi-region replication

---

## Appendix B: Storage Backend Schema

### SQLite/PostgreSQL Schema

```sql
-- Projects table
CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    config JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- Baselines table
CREATE TABLE baselines (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    benchmark TEXT NOT NULL,
    version TEXT NOT NULL,
    git_ref TEXT,
    git_sha TEXT,
    receipt JSONB NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}',
    tags TEXT[] NOT NULL DEFAULT '{}',
    source TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    deleted BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    
    UNIQUE(project_id, benchmark, version)
);

-- Indexes
CREATE INDEX idx_baselines_project_benchmark ON baselines(project_id, benchmark);
CREATE INDEX idx_baselines_project_git_ref ON baselines(project_id, git_ref);
CREATE INDEX idx_baselines_project_tags ON baselines USING GIN(project_id, tags);
CREATE INDEX idx_baselines_created_at ON baselines(created_at DESC);

-- API keys table
CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    scopes TEXT[] NOT NULL,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ
);

-- Audit log table
CREATE TABLE audit_log (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    actor JSONB NOT NULL,
    details JSONB NOT NULL DEFAULT '{}',
    ip_address TEXT,
    user_agent TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_audit_log_project ON audit_log(project_id, created_at DESC);
```

---

## Appendix C: OpenAPI Specification (Partial)

```yaml
openapi: 3.1.0
info:
  title: perfgate Baseline Service API
  version: 2.0.0
  description: Centralized baseline management for perfgate

servers:
  - url: https://perfgate.example.com/api/v1
    description: Production server

paths:
  /projects/{project}/baselines:
    get:
      summary: List baselines
      parameters:
        - name: project
          in: path
          required: true
          schema:
            type: string
        - name: benchmark_prefix
          in: query
          schema:
            type: string
        - name: git_ref
          in: query
          schema:
            type: string
        - name: tags
          in: query
          schema:
            type: array
            items:
              type: string
        - name: limit
          in: query
          schema:
            type: integer
            default: 50
            maximum: 200
        - name: offset
          in: query
          schema:
            type: integer
            default: 0
      responses:
        '200':
          description: List of baselines
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/ListBaselinesResponse'
    post:
      summary: Upload a baseline
      parameters:
        - name: project
          in: path
          required: true
          schema:
            type: string
      requestBody:
        required: true
        content:
          application/json:
            schema:
              $ref: '#/components/schemas/UploadBaselineRequest'
      responses:
        '201':
          description: Baseline created
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/UploadBaselineResponse'
        '409':
          description: Version already exists

  /projects/{project}/baselines/{benchmark}/latest:
    get:
      summary: Get latest baseline for a benchmark
      parameters:
        - name: project
          in: path
          required: true
          schema:
            type: string
        - name: benchmark
          in: path
          required: true
          schema:
            type: string
      responses:
        '200':
          description: Baseline record
          content:
            application/json:
              schema:
                $ref: '#/components/schemas/BaselineRecord'
        '404':
          description: Baseline not found

components:
  schemas:
    BaselineRecord:
      type: object
      required:
        - schema
        - id
        - project
        - benchmark
        - version
        - receipt
        - created_at
      properties:
        schema:
          type: string
          const: perfgate.baseline.v1
        id:
          type: string
        project:
          type: string
        benchmark:
          type: string
        version:
          type: string
        git_ref:
          type: string
        git_sha:
          type: string
        receipt:
          $ref: '#/components/schemas/RunReceipt'
        metadata:
          type: object
          additionalProperties:
            type: string
        tags:
          type: array
          items:
            type: string
        created_at:
          type: string
          format: date-time

  securitySchemes:
    bearerAuth:
      type: http
      scheme: bearer
      bearerFormat: API Key

security:
  - bearerAuth: []
```
