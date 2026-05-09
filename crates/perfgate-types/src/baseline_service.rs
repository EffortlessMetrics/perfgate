//! Baseline service wire contracts.
//!
//! Defines request/response types, baseline records, project models, and verdict
//! history used by both the server and client crates.
//!
//! These request, response, record, project, audit, and verdict types are shared
//! by `perfgate-client` and `perfgate-server`.
//!
//! # Example
//!
//! ```
//! use perfgate_types::baseline_service::BASELINE_SCHEMA_V1;
//!
//! assert_eq!(BASELINE_SCHEMA_V1, "perfgate.baseline.v1");
//! ```

pub mod auth;

use crate::{
    DecisionArtifactIndex, MetricStatus, RunReceipt, ScenarioReceipt, TradeoffReceipt,
    VerdictCounts, VerdictStatus,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema identifier for baseline records.
pub const BASELINE_SCHEMA_V1: &str = "perfgate.baseline.v1";

/// Schema identifier for project records.
pub const PROJECT_SCHEMA_V1: &str = "perfgate.project.v1";

/// Schema identifier for verdict records.
pub const VERDICT_SCHEMA_V1: &str = "perfgate.verdict.v1";

/// Schema identifier for decision ledger records.
pub const DECISION_RECORD_SCHEMA_V1: &str = "perfgate.decision_record.v1";

/// Schema identifier for audit event records.
pub const AUDIT_SCHEMA_V1: &str = "perfgate.audit.v1";

/// Schema identifier for health response fixtures.
pub const HEALTH_SCHEMA_V1: &str = "perfgate.health.v1";

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
    /// Coefficient of variation for benchmark wall time in this verdict.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_ms_cv: Option<f64>,
    /// Historical flakiness score derived from recent wall-time CV samples.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flakiness_score: Option<f64>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wall_ms_cv: Option<f64>,
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

/// A stored performance decision receipt for the server-side decision ledger.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DecisionRecord {
    /// Schema identifier (perfgate.decision_record.v1)
    pub schema: String,
    /// Unique decision identifier.
    pub id: String,
    /// Project identifier.
    pub project: String,
    /// Scenario/workload name, when present in the tradeoff receipt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// Final decision metric status after tradeoff policy.
    pub status: MetricStatus,
    /// Final policy verdict exposed by the tradeoff receipt.
    pub verdict: VerdictStatus,
    /// Accepted tradeoff rule names.
    #[serde(default)]
    pub accepted_rules: Vec<String>,
    /// Whether a human review is required before treating the decision as accepted.
    #[serde(default)]
    pub review_required: bool,
    /// Reasons the decision needs review.
    #[serde(default)]
    pub review_reasons: Vec<String>,
    /// Git reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Git commit SHA.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Optional scenario receipt captured with the decision.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario_receipt: Option<ScenarioReceipt>,
    /// Tradeoff decision receipt.
    pub tradeoff_receipt: TradeoffReceipt,
    /// Optional artifact index for the decision evidence set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_index: Option<DecisionArtifactIndex>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

/// Request for uploading a performance decision.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadDecisionRequest {
    pub tradeoff: TradeoffReceipt,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario: Option<ScenarioReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_index: Option<DecisionArtifactIndex>,
    pub git_ref: Option<String>,
    pub git_sha: Option<String>,
}

/// Query parameters for listing decision records.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListDecisionsQuery {
    pub scenario: Option<String>,
    pub status: Option<MetricStatus>,
    pub verdict: Option<VerdictStatus>,
    pub review_required: Option<bool>,
    pub accepted: Option<bool>,
    pub rule: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u64,
}

impl Default for ListDecisionsQuery {
    fn default() -> Self {
        Self {
            scenario: None,
            status: None,
            verdict: None,
            review_required: None,
            accepted: None,
            rule: None,
            limit: default_limit(),
            offset: 0,
        }
    }
}

impl ListDecisionsQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_scenario(mut self, scenario: impl Into<String>) -> Self {
        self.scenario = Some(scenario.into());
        self
    }
    pub fn with_status(mut self, status: MetricStatus) -> Self {
        self.status = Some(status);
        self
    }
    pub fn with_verdict(mut self, verdict: VerdictStatus) -> Self {
        self.verdict = Some(verdict);
        self
    }
    pub fn with_review_required(mut self, review_required: bool) -> Self {
        self.review_required = Some(review_required);
        self
    }
    pub fn with_accepted(mut self, accepted: bool) -> Self {
        self.accepted = Some(accepted);
        self
    }
    pub fn with_rule(mut self, rule: impl Into<String>) -> Self {
        self.rule = Some(rule.into());
        self
    }
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }
    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = offset;
        self
    }
}

/// Response for decision list operation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListDecisionsResponse {
    pub decisions: Vec<DecisionRecord>,
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
    /// A performance decision record
    Decision,
}

impl std::fmt::Display for AuditResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuditResourceType::Baseline => write!(f, "baseline"),
            AuditResourceType::Key => write!(f, "key"),
            AuditResourceType::Verdict => write!(f, "verdict"),
            AuditResourceType::Decision => write!(f, "decision"),
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
            "decision" => Ok(AuditResourceType::Decision),
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
    /// Coarse, sanitized failure detail when the backend is unhealthy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
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
    pub role: auth::Role,
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
    pub role: auth::Role,
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
    pub role: auth::Role,
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

// ---------------------------------------------------------------------------
// Fleet-wide dependency regression detection types
// ---------------------------------------------------------------------------

/// Schema identifier for dependency event records.
pub const DEPENDENCY_EVENT_SCHEMA_V1: &str = "perfgate.dependency_event.v1";

/// Schema identifier for fleet alert records.
pub const FLEET_ALERT_SCHEMA_V1: &str = "perfgate.fleet_alert.v1";

/// A single dependency version change observed alongside a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DependencyChange {
    /// Dependency name (e.g., crate name)
    pub name: String,
    /// Previous version (None if newly added)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// New version (None if removed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
}

/// A recorded dependency change event with its performance impact.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DependencyEvent {
    /// Schema identifier
    pub schema: String,
    /// Unique event identifier
    pub id: String,
    /// Project that reported the event
    pub project: String,
    /// Benchmark name
    pub benchmark: String,
    /// Dependency name
    pub dep_name: String,
    /// Previous version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// New version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    /// Primary metric name (e.g., "wall_ms")
    pub metric: String,
    /// Percentage change in that metric (positive = regression)
    pub delta_pct: f64,
    /// Timestamp of the event
    pub created_at: DateTime<Utc>,
}

/// Request to record a dependency change event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecordDependencyEventRequest {
    /// Project that observed the event
    pub project: String,
    /// Benchmark name
    pub benchmark: String,
    /// List of dependency changes observed
    pub dependency_changes: Vec<DependencyChange>,
    /// Primary metric name
    pub metric: String,
    /// Percentage change in the metric (positive = regression)
    pub delta_pct: f64,
}

/// Response after recording dependency events.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RecordDependencyEventResponse {
    /// Number of events recorded
    pub recorded: usize,
}

/// A project affected by a fleet-wide dependency regression.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct AffectedProject {
    /// Project identifier
    pub project: String,
    /// Benchmark name
    pub benchmark: String,
    /// Primary metric name
    pub metric: String,
    /// Percentage change
    pub delta_pct: f64,
}

/// A fleet-wide alert: multiple projects regressed after the same dependency update.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct FleetAlert {
    /// Schema identifier
    pub schema: String,
    /// Unique alert identifier
    pub id: String,
    /// Dependency name
    pub dependency: String,
    /// Previous version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    /// New version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    /// Projects affected by this dependency change
    pub affected_projects: Vec<AffectedProject>,
    /// Confidence score (0.0 - 1.0): higher means more projects affected
    pub confidence: f64,
    /// Average delta percentage across affected projects
    pub avg_delta_pct: f64,
    /// When the alert was first detected
    pub first_seen: DateTime<Utc>,
}

/// Query parameters for listing fleet alerts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListFleetAlertsQuery {
    /// Minimum number of affected projects to include
    #[serde(default = "default_min_affected")]
    pub min_affected: usize,
    /// Only include alerts since this time
    pub since: Option<DateTime<Utc>>,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u32,
}

impl Default for ListFleetAlertsQuery {
    fn default() -> Self {
        Self {
            min_affected: default_min_affected(),
            since: None,
            limit: default_limit(),
        }
    }
}

fn default_min_affected() -> usize {
    2
}

/// Response for listing fleet alerts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListFleetAlertsResponse {
    pub alerts: Vec<FleetAlert>,
}

/// Query parameters for dependency impact lookup.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependencyImpactQuery {
    /// Only include events since this time
    pub since: Option<DateTime<Utc>>,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u32,
}

impl Default for DependencyImpactQuery {
    fn default() -> Self {
        Self {
            since: None,
            limit: default_limit(),
        }
    }
}

/// Response for dependency impact lookup.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DependencyImpactResponse {
    /// Dependency name
    pub dependency: String,
    /// All recorded events for this dependency
    pub events: Vec<DependencyEvent>,
    /// Number of distinct projects affected
    pub project_count: usize,
    /// Average delta percentage
    pub avg_delta_pct: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BenchMeta, HostInfo, RUN_SCHEMA_V1, RunMeta, Stats, ToolInfo, U64Summary};
    use chrono::TimeZone;

    fn timestamp() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
    }

    fn run_receipt() -> RunReceipt {
        RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.15.1".to_string(),
            },
            run: RunMeta {
                id: "run-1".to_string(),
                started_at: "2026-01-02T03:04:05Z".to_string(),
                ended_at: "2026-01-02T03:04:06Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "bench-a".to_string(),
                cwd: None,
                command: vec!["bench".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: Vec::new(),
            stats: Stats {
                wall_ms: U64Summary::new(100, 90, 110),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    fn baseline_record() -> BaselineRecord {
        BaselineRecord {
            schema: BASELINE_SCHEMA_V1.to_string(),
            id: "baseline-1".to_string(),
            project: "project-a".to_string(),
            benchmark: "bench-a".to_string(),
            version: "v1".to_string(),
            git_ref: Some("refs/heads/main".to_string()),
            git_sha: Some("abc123".to_string()),
            receipt: run_receipt(),
            metadata: BTreeMap::from([("runner".to_string(), "linux".to_string())]),
            tags: vec!["stable".to_string()],
            created_at: timestamp(),
            updated_at: timestamp(),
            content_hash: "deadbeef".to_string(),
            source: BaselineSource::Promote,
            deleted: false,
        }
    }

    #[test]
    fn baseline_record_helpers_preserve_contract_fields() {
        let record = baseline_record();
        assert_eq!(record.etag(), "\"sha256:deadbeef\"");

        let summary = BaselineSummary::from(record);
        assert_eq!(summary.id, "baseline-1");
        assert_eq!(summary.benchmark, "bench-a");
        assert_eq!(summary.version, "v1");
        assert_eq!(summary.git_ref.as_deref(), Some("refs/heads/main"));
        assert_eq!(summary.git_sha.as_deref(), Some("abc123"));
        assert_eq!(summary.tags, ["stable"]);
        assert!(summary.receipt.is_some());
    }

    #[test]
    fn baseline_query_builder_tracks_filters_and_params() {
        let query = ListBaselinesQuery::new()
            .with_benchmark("bench-a")
            .with_benchmark_prefix("bench")
            .with_limit(10)
            .with_offset(20)
            .with_receipts();
        assert_eq!(query.benchmark.as_deref(), Some("bench-a"));
        assert_eq!(query.benchmark_prefix.as_deref(), Some("bench"));
        assert!(query.include_receipt);

        let mut query = query;
        query.git_ref = Some("main".to_string());
        query.git_sha = Some("abc123".to_string());
        query.tags = Some("stable, production,, ".to_string());
        query.since = Some(timestamp());
        query.until = Some(timestamp());

        assert_eq!(query.parsed_tags(), ["stable", "production"]);
        let params = query.to_query_params();
        assert!(params.contains(&("benchmark".to_string(), "bench-a".to_string())));
        assert!(params.contains(&("benchmark_prefix".to_string(), "bench".to_string())));
        assert!(params.contains(&("git_ref".to_string(), "main".to_string())));
        assert!(params.contains(&("git_sha".to_string(), "abc123".to_string())));
        assert!(params.contains(&("tags".to_string(), "stable, production,, ".to_string())));
        assert!(params.contains(&("since".to_string(), timestamp().to_rfc3339())));
        assert!(params.contains(&("until".to_string(), timestamp().to_rfc3339())));
        assert!(params.contains(&("limit".to_string(), "10".to_string())));
        assert!(params.contains(&("offset".to_string(), "20".to_string())));
        assert!(params.contains(&("include_receipt".to_string(), "true".to_string())));
    }

    #[test]
    fn verdict_and_audit_queries_have_stable_defaults() {
        let verdicts = ListVerdictsQuery::new()
            .with_benchmark("bench-a")
            .with_status(VerdictStatus::Warn)
            .with_limit(25)
            .with_offset(5);
        assert_eq!(verdicts.benchmark.as_deref(), Some("bench-a"));
        assert_eq!(verdicts.status, Some(VerdictStatus::Warn));
        assert_eq!(verdicts.limit, 25);
        assert_eq!(verdicts.offset, 5);

        let audit = ListAuditEventsQuery::default();
        assert_eq!(audit.limit, default_limit());
        assert_eq!(audit.offset, 0);
        assert!(audit.project.is_none());
        assert!(audit.action.is_none());
        assert!(audit.resource_type.is_none());
        assert!(audit.actor.is_none());
        assert!(audit.since.is_none());
        assert!(audit.until.is_none());
    }

    #[test]
    fn retention_and_fleet_defaults_are_stable() {
        let retention = RetentionPolicy::default();
        assert_eq!(retention.max_versions, Some(50));
        assert_eq!(retention.max_age_days, Some(365));
        assert_eq!(retention.preserve_tags, ["production", "stable"]);

        let fleet = ListFleetAlertsQuery::default();
        assert_eq!(fleet.min_affected, default_min_affected());
        assert_eq!(fleet.limit, default_limit());
        assert!(fleet.since.is_none());

        let dependency = DependencyImpactQuery::default();
        assert_eq!(dependency.limit, default_limit());
        assert!(dependency.since.is_none());
    }

    #[test]
    fn health_storage_detail_is_additive() {
        let legacy = serde_json::json!({
            "status": "healthy",
            "version": "0.15.1",
            "storage": {
                "backend": "memory",
                "status": "healthy"
            }
        });
        let legacy: HealthResponse = serde_json::from_value(legacy).expect("legacy health");
        assert_eq!(legacy.storage.detail, None);

        let detailed = serde_json::json!({
            "status": "degraded",
            "version": "0.15.1",
            "storage": {
                "backend": "postgres",
                "status": "unhealthy",
                "detail": "query_error"
            }
        });
        let detailed: HealthResponse = serde_json::from_value(detailed).expect("detailed health");
        assert_eq!(detailed.storage.detail.as_deref(), Some("query_error"));
    }

    #[test]
    fn audit_enums_parse_and_render_wire_tokens() {
        assert_eq!(AuditAction::Create.to_string(), "create");
        assert_eq!(AuditAction::Update.to_string(), "update");
        assert_eq!(AuditAction::Delete.to_string(), "delete");
        assert_eq!(AuditAction::Promote.to_string(), "promote");
        assert_eq!("create".parse::<AuditAction>(), Ok(AuditAction::Create));
        assert_eq!("update".parse::<AuditAction>(), Ok(AuditAction::Update));
        assert_eq!("delete".parse::<AuditAction>(), Ok(AuditAction::Delete));
        assert_eq!("promote".parse::<AuditAction>(), Ok(AuditAction::Promote));
        assert!("unknown".parse::<AuditAction>().is_err());

        assert_eq!(AuditResourceType::Baseline.to_string(), "baseline");
        assert_eq!(AuditResourceType::Key.to_string(), "key");
        assert_eq!(AuditResourceType::Verdict.to_string(), "verdict");
        assert_eq!(
            "baseline".parse::<AuditResourceType>(),
            Ok(AuditResourceType::Baseline)
        );
        assert_eq!(
            "key".parse::<AuditResourceType>(),
            Ok(AuditResourceType::Key)
        );
        assert_eq!(
            "verdict".parse::<AuditResourceType>(),
            Ok(AuditResourceType::Verdict)
        );
        assert!("runner".parse::<AuditResourceType>().is_err());
    }

    #[test]
    fn api_error_constructors_use_stable_codes() {
        assert_eq!(ApiError::new("custom", "message").code, "custom");
        assert_eq!(ApiError::unauthorized("message").code, "unauthorized");
        assert_eq!(ApiError::forbidden("message").code, "forbidden");
        assert_eq!(ApiError::not_found("message").code, "not_found");
        assert_eq!(ApiError::bad_request("message").code, "bad_request");
        assert_eq!(ApiError::conflict("message").code, "conflict");
        assert_eq!(ApiError::internal_error("message").code, "internal_error");
        assert_eq!(ApiError::internal("message").code, "internal_error");
        assert_eq!(ApiError::validation("message").code, "invalid_input");
        assert_eq!(ApiError::already_exists("message").code, "conflict");
    }
}
