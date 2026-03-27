//! Common API types and models for the perfgate baseline service.
//!
//! Defines request/response types, baseline records, project models, and verdict
//! history used by both the server and client crates.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Example
//!
//! ```
//! use perfgate_api::BASELINE_SCHEMA_V1;
//!
//! assert_eq!(BASELINE_SCHEMA_V1, "perfgate.baseline.v1");
//! ```

use chrono::{DateTime, Utc};
use perfgate_types::{RunReceipt, VerdictCounts, VerdictStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema identifier for baseline records.
pub const BASELINE_SCHEMA_V1: &str = "perfgate.baseline.v1";

/// Schema identifier for project records.
pub const PROJECT_SCHEMA_V1: &str = "perfgate.project.v1";

/// Schema identifier for verdict records.
pub const VERDICT_SCHEMA_V1: &str = "perfgate.verdict.v1";

/// Schema identifier for audit event records.
pub const AUDIT_SCHEMA_V1: &str = "perfgate.audit.v1";

/// Source of baseline creation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSource {
    /// Uploaded directly via API
    #[default]
    Upload,
    /// Created via promote operation
    Promote,
    /// Migrated from external storage
    Migrate,
    /// Created via rollback operation
    Rollback,
}

/// The primary storage model for baselines.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct BaselineRecord {
    /// Schema identifier (perfgate.baseline.v1)
    pub schema: String,
    /// Unique baseline identifier (ULID format)
    pub id: String,
    /// Project/namespace identifier
    pub project: String,
    /// Benchmark name
    pub benchmark: String,
    /// Semantic version
    pub version: String,
    /// Git reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Git commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Full run receipt
    pub receipt: RunReceipt,
    /// User-provided metadata
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creation timestamp (RFC 3339)
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp
    pub updated_at: DateTime<Utc>,
    /// Content hash for ETag
    pub content_hash: String,
    /// Creation source
    pub source: BaselineSource,
    /// Soft delete flag
    #[serde(default)]
    pub deleted: bool,
}

impl BaselineRecord {
    /// Returns the ETag value for this baseline.
    pub fn etag(&self) -> String {
        format!("\"sha256:{}\"", self.content_hash)
    }
}

/// A record of a benchmark execution verdict.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct VerdictRecord {
    /// Schema identifier (perfgate.verdict.v1)
    pub schema: String,
    /// Unique verdict identifier
    pub id: String,
    /// Project identifier
    pub project: String,
    /// Benchmark name
    pub benchmark: String,
    /// Run identifier from receipt
    pub run_id: String,
    /// Overall status (pass/warn/fail/skip)
    pub status: VerdictStatus,
    /// Detailed counts
    pub counts: VerdictCounts,
    /// List of reasons for the verdict
    pub reasons: Vec<String>,
    /// Git reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Git commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

/// Request for submitting a verdict.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SubmitVerdictRequest {
    pub benchmark: String,
    pub run_id: String,
    pub status: VerdictStatus,
    pub counts: VerdictCounts,
    pub reasons: Vec<String>,
    pub git_ref: Option<String>,
    pub git_sha: Option<String>,
}

/// Request for verdict list operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListVerdictsQuery {
    /// Filter by exact benchmark name
    pub benchmark: Option<String>,
    /// Filter by status
    pub status: Option<VerdictStatus>,
    /// Filter by creation date (after)
    pub since: Option<DateTime<Utc>>,
    /// Filter by creation date (before)
    pub until: Option<DateTime<Utc>>,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Pagination offset
    #[serde(default)]
    pub offset: u64,
}

impl Default for ListVerdictsQuery {
    fn default() -> Self {
        Self {
            benchmark: None,
            status: None,
            since: None,
            until: None,
            limit: default_limit(),
            offset: 0,
        }
    }
}

impl ListVerdictsQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_benchmark(mut self, b: impl Into<String>) -> Self {
        self.benchmark = Some(b.into());
        self
    }
    pub fn with_status(mut self, s: VerdictStatus) -> Self {
        self.status = Some(s);
        self
    }
    pub fn with_limit(mut self, l: u32) -> Self {
        self.limit = l;
        self
    }
    pub fn with_offset(mut self, o: u64) -> Self {
        self.offset = o;
        self
    }
}

/// Response for verdict list operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListVerdictsResponse {
    pub verdicts: Vec<VerdictRecord>,
    pub pagination: PaginationInfo,
}

/// Version history metadata (without full receipt).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BaselineVersion {
    /// Version identifier
    pub version: String,
    /// Git reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Git commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Creator identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    /// Whether this is the current/promoted version
    pub is_current: bool,
    /// Source of this version
    pub source: BaselineSource,
}

/// Retention policy for a project.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// Maximum number of versions to keep per benchmark.
    pub max_versions: Option<u32>,
    /// Maximum age of a version in days.
    pub max_age_days: Option<u32>,
    /// Tags that prevent a version from being deleted.
    pub preserve_tags: Vec<String>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_versions: Some(50),
            max_age_days: Some(365),
            preserve_tags: vec!["production".to_string(), "stable".to_string()],
        }
    }
}

/// Strategy for auto-generating versions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersioningStrategy {
    /// Use run_id from receipt as version
    #[default]
    RunId,
    /// Use timestamp as version
    Timestamp,
    /// Use git_sha as version
    GitSha,
    /// Manual version required
    Manual,
}

/// Multi-tenancy namespace with retention policies.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Project {
    /// Schema identifier (perfgate.project.v1)
    pub schema: String,
    /// Project identifier (URL-safe)
    pub id: String,
    /// Display name
    pub name: String,
    /// Project description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Retention policy
    pub retention: RetentionPolicy,
    /// Default baseline versioning strategy
    pub versioning: VersioningStrategy,
}

/// Request for baseline list operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListBaselinesQuery {
    /// Filter by exact benchmark name
    pub benchmark: Option<String>,
    /// Filter by benchmark name prefix
    pub benchmark_prefix: Option<String>,
    /// Filter by git reference
    pub git_ref: Option<String>,
    /// Filter by git SHA
    pub git_sha: Option<String>,
    /// Filter by tags (comma-separated)
    pub tags: Option<String>,
    /// Filter by creation date (after)
    pub since: Option<DateTime<Utc>>,
    /// Filter by creation date (before)
    pub until: Option<DateTime<Utc>>,
    /// Include full receipts in output
    #[serde(default)]
    pub include_receipt: bool,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Pagination offset
    #[serde(default)]
    pub offset: u64,
}

impl Default for ListBaselinesQuery {
    fn default() -> Self {
        Self {
            benchmark: None,
            benchmark_prefix: None,
            git_ref: None,
            git_sha: None,
            tags: None,
            since: None,
            until: None,
            include_receipt: false,
            limit: default_limit(),
            offset: 0,
        }
    }
}

fn default_limit() -> u32 {
    50
}

impl ListBaselinesQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_benchmark(mut self, b: impl Into<String>) -> Self {
        self.benchmark = Some(b.into());
        self
    }
    pub fn with_benchmark_prefix(mut self, p: impl Into<String>) -> Self {
        self.benchmark_prefix = Some(p.into());
        self
    }
    pub fn with_offset(mut self, o: u64) -> Self {
        self.offset = o;
        self
    }
    pub fn with_limit(mut self, l: u32) -> Self {
        self.limit = l;
        self
    }
    pub fn with_receipts(mut self) -> Self {
        self.include_receipt = true;
        self
    }
    pub fn parsed_tags(&self) -> Vec<String> {
        self.tags
            .as_ref()
            .map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn to_query_params(&self) -> Vec<(String, String)> {
        let mut params = Vec::new();
        if let Some(b) = &self.benchmark {
            params.push(("benchmark".to_string(), b.clone()));
        }
        if let Some(p) = &self.benchmark_prefix {
            params.push(("benchmark_prefix".to_string(), p.clone()));
        }
        if let Some(r) = &self.git_ref {
            params.push(("git_ref".to_string(), r.clone()));
        }
        if let Some(s) = &self.git_sha {
            params.push(("git_sha".to_string(), s.clone()));
        }
        if let Some(t) = &self.tags {
            params.push(("tags".to_string(), t.clone()));
        }
        if let Some(s) = &self.since {
            params.push(("since".to_string(), s.to_rfc3339()));
        }
        if let Some(u) = &self.until {
            params.push(("until".to_string(), u.to_rfc3339()));
        }
        params.push(("limit".to_string(), self.limit.to_string()));
        params.push(("offset".to_string(), self.offset.to_string()));
        if self.include_receipt {
            params.push(("include_receipt".to_string(), "true".to_string()));
        }
        params
    }
}

/// Pagination information for lists.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaginationInfo {
    /// Total count of items (if known)
    pub total: u64,
    /// Offset of current page
    pub offset: u64,
    /// Limit of items per page
    pub limit: u32,
    /// Whether more items are available
    pub has_more: bool,
}

/// Response for baseline list operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListBaselinesResponse {
    /// List of baseline summaries or records
    pub baselines: Vec<BaselineSummary>,
    /// Pagination metadata
    pub pagination: PaginationInfo,
}

/// Summary of a baseline record (without full receipt).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct BaselineSummary {
    pub id: String,
    pub benchmark: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub git_ref: Option<String>,
    pub git_sha: Option<String>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<RunReceipt>,
}

impl From<BaselineRecord> for BaselineSummary {
    fn from(record: BaselineRecord) -> Self {
        Self {
            id: record.id,
            benchmark: record.benchmark,
            version: record.version,
            created_at: record.created_at,
            git_ref: record.git_ref,
            git_sha: record.git_sha,
            tags: record.tags,
            receipt: Some(record.receipt),
        }
    }
}

/// Request for baseline upload.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadBaselineRequest {
    pub benchmark: String,
    pub version: Option<String>,
    pub git_ref: Option<String>,
    pub git_sha: Option<String>,
    pub receipt: RunReceipt,
    pub metadata: BTreeMap<String, String>,
    pub tags: Vec<String>,
    pub normalize: bool,
}

/// Response for successful baseline upload.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadBaselineResponse {
    pub id: String,
    pub benchmark: String,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub etag: String,
}

/// Request for baseline promotion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PromoteBaselineRequest {
    pub from_version: String,
    pub to_version: String,
    pub git_ref: Option<String>,
    pub git_sha: Option<String>,
    pub tags: Vec<String>,
    #[serde(default)]
    pub normalize: bool,
}

/// Response for baseline promotion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PromoteBaselineResponse {
    pub id: String,
    pub benchmark: String,
    pub version: String,
    pub promoted_from: String,
    pub promoted_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

/// Response for baseline deletion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteBaselineResponse {
    pub deleted: bool,
    pub id: String,
    pub benchmark: String,
    pub version: String,
    pub deleted_at: DateTime<Utc>,
}

// =========================================================================
// Audit logging types
// =========================================================================

/// The action that was performed in an audit event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// A resource was created (e.g., baseline upload)
    Create,
    /// A resource was updated
    Update,
    /// A resource was deleted (soft or hard)
    Delete,
    /// A baseline was promoted
    Promote,
}

impl std::fmt::Display for AuditAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditAction::Create => write!(f, "create"),
            AuditAction::Update => write!(f, "update"),
            AuditAction::Delete => write!(f, "delete"),
            AuditAction::Promote => write!(f, "promote"),
        }
    }
}

impl std::str::FromStr for AuditAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "create" => Ok(AuditAction::Create),
            "update" => Ok(AuditAction::Update),
            "delete" => Ok(AuditAction::Delete),
            "promote" => Ok(AuditAction::Promote),
            other => Err(format!("Unknown audit action: {}", other)),
        }
    }
}

/// The type of resource affected by an audit event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditResourceType {
    /// A baseline record
    Baseline,
    /// An API key
    Key,
    /// A verdict record
    Verdict,
}

impl std::fmt::Display for AuditResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditResourceType::Baseline => write!(f, "baseline"),
            AuditResourceType::Key => write!(f, "key"),
            AuditResourceType::Verdict => write!(f, "verdict"),
        }
    }
}

impl std::str::FromStr for AuditResourceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "baseline" => Ok(AuditResourceType::Baseline),
            "key" => Ok(AuditResourceType::Key),
            "verdict" => Ok(AuditResourceType::Verdict),
            other => Err(format!("Unknown resource type: {}", other)),
        }
    }
}

/// An append-only audit event for tracking mutations and admin actions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct AuditEvent {
    /// Unique event identifier
    pub id: String,
    /// Timestamp of the event (RFC 3339)
    pub timestamp: DateTime<Utc>,
    /// Actor identity (API key ID or OIDC subject)
    pub actor: String,
    /// The action performed
    pub action: AuditAction,
    /// Type of resource affected
    pub resource_type: AuditResourceType,
    /// Identifier for the affected resource
    pub resource_id: String,
    /// Project scope
    pub project: String,
    /// Additional structured metadata (endpoint-specific details)
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Query parameters for listing audit events.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListAuditEventsQuery {
    /// Filter by project
    pub project: Option<String>,
    /// Filter by action
    pub action: Option<String>,
    /// Filter by resource type
    pub resource_type: Option<String>,
    /// Filter by actor
    pub actor: Option<String>,
    /// Filter by events after this time
    pub since: Option<DateTime<Utc>>,
    /// Filter by events before this time
    pub until: Option<DateTime<Utc>>,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Pagination offset
    #[serde(default)]
    pub offset: u64,
}

impl Default for ListAuditEventsQuery {
    fn default() -> Self {
        Self {
            project: None,
            action: None,
            resource_type: None,
            actor: None,
            since: None,
            until: None,
            limit: default_limit(),
            offset: 0,
        }
    }
}

/// Response for audit event list operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListAuditEventsResponse {
    /// The audit events matching the query
    pub events: Vec<AuditEvent>,
    /// Pagination metadata
    pub pagination: PaginationInfo,
}

/// Health status of a storage backend.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct StorageHealth {
    pub backend: String,
    pub status: String,
}

/// Connection pool metrics exposed via the health endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PoolMetrics {
    /// Number of idle connections in the pool.
    pub idle: u32,
    /// Number of active (in-use) connections.
    pub active: u32,
    /// Maximum number of connections the pool is configured for.
    pub max: u32,
}

/// Response for health check.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub storage: StorageHealth,
    /// Connection pool metrics (present only for pooled backends such as PostgreSQL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool: Option<PoolMetrics>,
}

/// Generic error response for the API.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }
    pub fn unauthorized(msg: &str) -> Self {
        Self::new("unauthorized", msg)
    }
    pub fn forbidden(msg: &str) -> Self {
        Self::new("forbidden", msg)
    }
    pub fn not_found(msg: &str) -> Self {
        Self::new("not_found", msg)
    }
    pub fn bad_request(msg: &str) -> Self {
        Self::new("bad_request", msg)
    }
    pub fn conflict(msg: &str) -> Self {
        Self::new("conflict", msg)
    }
    pub fn internal_error(msg: &str) -> Self {
        Self::new("internal_error", msg)
    }
    pub fn internal(msg: &str) -> Self {
        Self::internal_error(msg)
    }
    pub fn validation(msg: &str) -> Self {
        Self::new("invalid_input", msg)
    }
    pub fn already_exists(msg: &str) -> Self {
        Self::new("conflict", msg)
    }
}

// ── API Key Management Types ──────────────────────────────────────────

/// Request for creating a new API key.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateKeyRequest {
    /// Human-readable description
    pub description: String,
    /// Role to assign (viewer, contributor, promoter, admin)
    pub role: perfgate_auth::Role,
    /// Project this key is scoped to (use "*" for all projects)
    pub project: String,
    /// Optional glob pattern to restrict benchmark access
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Optional expiration timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Response for creating a new API key (contains the plaintext key once).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateKeyResponse {
    /// Unique key identifier (for management)
    pub id: String,
    /// The plaintext API key (only returned once)
    pub key: String,
    /// Human-readable description
    pub description: String,
    /// Assigned role
    pub role: perfgate_auth::Role,
    /// Scoped project
    pub project: String,
    /// Optional benchmark pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Expiration timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// A redacted API key entry returned by list operations.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KeyEntry {
    /// Unique key identifier
    pub id: String,
    /// Redacted key prefix (e.g., "pg_live_abc1...***")
    pub key_prefix: String,
    /// Human-readable description
    pub description: String,
    /// Assigned role
    pub role: perfgate_auth::Role,
    /// Scoped project
    pub project: String,
    /// Optional benchmark pattern
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Expiration timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Revocation timestamp (if revoked)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Response for listing API keys.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListKeysResponse {
    /// List of key entries (redacted)
    pub keys: Vec<KeyEntry>,
}

/// Response for revoking an API key.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevokeKeyResponse {
    /// The key ID that was revoked
    pub id: String,
    /// When the key was revoked
    pub revoked_at: DateTime<Utc>,
}

#[cfg(feature = "server")]
impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.code.as_str() {
            "bad_request" | "invalid_input" => http::StatusCode::BAD_REQUEST,
            "unauthorized" => http::StatusCode::UNAUTHORIZED,
            "forbidden" => http::StatusCode::FORBIDDEN,
            "not_found" => http::StatusCode::NOT_FOUND,
            "conflict" => http::StatusCode::CONFLICT,
            _ => http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(self)).into_response()
    }
}
