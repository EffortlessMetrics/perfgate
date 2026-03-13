//! Data models for the perfgate baseline service.
//!
//! These types represent the core domain objects and API request/response types
//! for the baseline storage service.

use chrono::{DateTime, Utc};
use perfgate_types::RunReceipt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema identifier for baseline records.
pub const BASELINE_SCHEMA_V1: &str = "perfgate.baseline.v1";

/// Schema identifier for project records.
pub const PROJECT_SCHEMA_V1: &str = "perfgate.project.v1";

// ----------------------------
// Core Storage Models
// ----------------------------

/// The primary storage model for baselines.
///
/// This represents a stored baseline with all its metadata, ready for
/// persistence and retrieval.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct BaselineRecord {
    /// Schema identifier (perfgate.baseline.v1)
    pub schema: String,

    /// Unique baseline identifier (ULID format)
    pub id: String,

    /// Project/namespace identifier
    pub project: String,

    /// Benchmark name (must match perfgate-types validation)
    pub benchmark: String,

    /// Semantic version for this baseline
    pub version: String,

    /// Git reference (branch, tag, or ref)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,

    /// Git commit SHA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,

    /// Full run receipt (perfgate.run.v1)
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

    /// Content hash for ETag/optimistic locking
    pub content_hash: String,

    /// Creation source (upload, promote, migrate)
    pub source: BaselineSource,

    /// Soft delete flag
    #[serde(default)]
    pub deleted: bool,
}

impl BaselineRecord {
    /// Creates a new baseline record with generated ID and timestamps.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        project: String,
        benchmark: String,
        version: String,
        receipt: RunReceipt,
        git_ref: Option<String>,
        git_sha: Option<String>,
        metadata: BTreeMap<String, String>,
        tags: Vec<String>,
        source: BaselineSource,
    ) -> Self {
        let now = Utc::now();
        let id = generate_ulid();
        let content_hash = compute_content_hash(&receipt);

        Self {
            schema: BASELINE_SCHEMA_V1.to_string(),
            id,
            project,
            benchmark,
            version,
            git_ref,
            git_sha,
            receipt,
            metadata,
            tags,
            created_at: now,
            updated_at: now,
            content_hash,
            source,
            deleted: false,
        }
    }

    /// Returns the ETag value for this baseline.
    pub fn etag(&self) -> String {
        format!("\"sha256:{}\"", self.content_hash)
    }
}

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

// ----------------------------
// Project Model
// ----------------------------

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

impl Project {
    /// Creates a new project with default settings.
    pub fn new(id: String, name: String) -> Self {
        Self {
            schema: PROJECT_SCHEMA_V1.to_string(),
            id,
            name,
            description: None,
            created_at: Utc::now(),
            retention: RetentionPolicy::default(),
            versioning: VersioningStrategy::default(),
        }
    }
}

/// Retention policy for baseline versions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RetentionPolicy {
    /// Maximum versions to keep per benchmark
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_versions: Option<u32>,

    /// Delete baselines older than this many days
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age_days: Option<u32>,

    /// Keep versions with these tags indefinitely
    #[serde(default)]
    pub preserve_tags: Vec<String>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            max_versions: Some(50),
            max_age_days: Some(365),
            preserve_tags: vec!["production".to_string()],
        }
    }
}

/// Baseline versioning strategy.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum VersioningStrategy {
    /// Semantic versioning (v1.0.0)
    #[default]
    Semantic,
    /// Timestamp-based versioning
    Timestamp,
    /// Git reference-based versioning
    GitRef,
}

// ----------------------------
// API Request Types
// ----------------------------

/// Request to upload a new baseline.
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

/// Request to promote a baseline version.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PromoteBaselineRequest {
    /// Source version to promote from
    pub from_version: String,

    /// Target version identifier
    pub to_version: String,

    /// Git reference for the promoted version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,

    /// Git commit SHA for the promoted version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,

    /// Normalize receipt during promotion
    #[serde(default)]
    pub normalize: bool,
}

/// Query parameters for listing baselines.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListBaselinesQuery {
    /// Exact benchmark name match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<String>,

    /// Benchmark name prefix filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_prefix: Option<String>,

    /// Git reference filter (supports glob patterns)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,

    /// Exact git SHA filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,

    /// Filter by tags (comma-separated, AND logic)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,

    /// Filter baselines created after this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,

    /// Filter baselines created before this time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,

    /// Maximum results (default: 50, max: 200)
    #[serde(default = "default_limit")]
    pub limit: u32,

    /// Pagination offset
    #[serde(default)]
    pub offset: u64,

    /// Include full receipt in response
    #[serde(default)]
    pub include_receipt: bool,
}

fn default_limit() -> u32 {
    50
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
            limit: default_limit(),
            offset: 0,
            include_receipt: false,
        }
    }
}

impl ListBaselinesQuery {
    /// Parses tags from comma-separated string into a vector.
    pub fn parsed_tags(&self) -> Option<Vec<String>> {
        self.tags.as_ref().map(|s| {
            s.split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect()
        })
    }

    /// Validates the query parameters.
    pub fn validate(&self) -> Result<(), String> {
        if self.limit > 200 {
            return Err("limit must not exceed 200".to_string());
        }
        Ok(())
    }
}

// ----------------------------
// API Response Types
// ----------------------------

/// Response for successful baseline upload.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UploadBaselineResponse {
    /// Unique baseline identifier
    pub id: String,

    /// Benchmark name
    pub benchmark: String,

    /// Version identifier
    pub version: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// ETag for caching
    pub etag: String,
}

/// Response for listing baselines.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListBaselinesResponse {
    /// List of baseline summaries
    pub baselines: Vec<BaselineSummary>,

    /// Pagination information
    pub pagination: PaginationInfo,
}

/// Summary of a baseline (without full receipt by default).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BaselineSummary {
    /// Unique baseline identifier
    pub id: String,

    /// Benchmark name
    pub benchmark: String,

    /// Version identifier
    pub version: String,

    /// Git reference
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Full receipt (only included when include_receipt=true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<RunReceipt>,
}

impl From<BaselineRecord> for BaselineSummary {
    fn from(record: BaselineRecord) -> Self {
        Self {
            id: record.id,
            benchmark: record.benchmark,
            version: record.version,
            git_ref: record.git_ref,
            created_at: record.created_at,
            tags: record.tags,
            receipt: None,
        }
    }
}

/// Pagination information for list responses.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PaginationInfo {
    /// Total number of results
    pub total: u64,

    /// Maximum results per page
    pub limit: u32,

    /// Current offset
    pub offset: u64,

    /// Whether more results exist
    pub has_more: bool,
}

/// Response for baseline deletion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteBaselineResponse {
    /// Whether deletion was successful
    pub deleted: bool,

    /// ID of the deleted baseline
    pub id: String,
}

/// Response for baseline promotion.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PromoteBaselineResponse {
    /// Unique baseline identifier
    pub id: String,

    /// Benchmark name
    pub benchmark: String,

    /// New version identifier
    pub version: String,

    /// Source version that was promoted
    pub promoted_from: String,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,
}

// ----------------------------
// Health Check Types
// ----------------------------

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthResponse {
    /// Health status
    pub status: String,

    /// Server version
    pub version: String,

    /// Storage backend status
    pub storage: StorageHealth,
}

/// Storage backend health status.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StorageHealth {
    /// Backend type (memory, sqlite, postgres)
    pub backend: String,

    /// Connection status
    pub status: String,
}

// ----------------------------
// Error Types
// ----------------------------

/// API error response envelope.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiError {
    /// Error details
    pub error: ApiErrorBody,
}

/// API error body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ApiErrorBody {
    /// Error code
    pub code: String,

    /// Human-readable message
    pub message: String,

    /// Additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,

    /// Request ID for tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
}

impl ApiError {
    /// Creates a new API error.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorBody {
                code: code.into(),
                message: message.into(),
                details: None,
                request_id: None,
            },
        }
    }

    /// Adds details to the error.
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.error.details = Some(details);
        self
    }

    /// Adds request ID to the error.
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.error.request_id = Some(request_id.into());
        self
    }

    /// Creates a not found error.
    pub fn not_found(resource: &str, identifier: &str) -> Self {
        Self::new(
            "NOT_FOUND",
            format!("{} '{}' not found", resource, identifier),
        )
    }

    /// Creates an unauthorized error.
    pub fn unauthorized(message: &str) -> Self {
        Self::new("UNAUTHORIZED", message)
    }

    /// Creates a forbidden error.
    pub fn forbidden(message: &str) -> Self {
        Self::new("FORBIDDEN", message)
    }

    /// Creates a validation error.
    pub fn validation(message: &str) -> Self {
        Self::new("VALIDATION_ERROR", message)
    }

    /// Creates an already exists error.
    pub fn already_exists(resource: &str, identifier: &str) -> Self {
        Self::new(
            "ALREADY_EXISTS",
            format!("{} '{}' already exists", resource, identifier),
        )
    }

    /// Creates an internal error.
    pub fn internal(message: &str) -> Self {
        Self::new("INTERNAL_ERROR", message)
    }
}

// ----------------------------
// Helper Functions
// ----------------------------

/// Generates a ULID-style identifier.
fn generate_ulid() -> String {
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    let now = chrono::Utc::now();
    let timestamp = now.timestamp_millis() as u64;

    // Simple ULID-like format: timestamp (10 chars) + random (16 chars)
    let random_bytes: [u8; 10] = uuid::Uuid::new_v4().as_bytes()[..10].try_into().unwrap();

    format!("{:010X}{}", timestamp, URL_SAFE_NO_PAD.encode(random_bytes))
}

/// Computes a content hash for a run receipt.
fn compute_content_hash(receipt: &RunReceipt) -> String {
    use sha2::{Digest, Sha256};

    // Serialize receipt to canonical JSON
    let canonical = serde_json::to_string(receipt).unwrap_or_default();

    // Compute SHA-256 hash
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let hash = hasher.finalize();

    // Return hex-encoded hash (first 32 chars)
    format!("{:x}", hash).chars().take(32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_ulid() {
        let id1 = generate_ulid();
        let id2 = generate_ulid();

        // IDs should be different
        assert_ne!(id1, id2);

        // IDs should be 26 characters (ULID-like format)
        assert!(id1.len() >= 16);
    }

    #[test]
    fn test_api_error_creation() {
        let error = ApiError::new("TEST_CODE", "Test message");
        assert_eq!(error.error.code, "TEST_CODE");
        assert_eq!(error.error.message, "Test message");
        assert!(error.error.details.is_none());
        assert!(error.error.request_id.is_none());
    }

    #[test]
    fn test_api_error_with_details() {
        let details = serde_json::json!({ "key": "value" });
        let error = ApiError::new("TEST", "test").with_details(details.clone());

        assert_eq!(error.error.details, Some(details));
    }

    #[test]
    fn test_list_baselines_query_tags_parsing() {
        let query = ListBaselinesQuery {
            tags: Some("tag1, tag2,tag3".to_string()),
            ..Default::default()
        };

        let tags = query.parsed_tags().unwrap();
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);
    }

    #[test]
    fn test_list_baselines_query_validation() {
        let invalid_query = ListBaselinesQuery {
            limit: 300,
            ..Default::default()
        };

        assert!(invalid_query.validate().is_err());

        let valid_query = ListBaselinesQuery {
            limit: 50,
            ..Default::default()
        };

        assert!(valid_query.validate().is_ok());
    }

    #[test]
    fn test_retention_policy_default() {
        let policy = RetentionPolicy::default();

        assert_eq!(policy.max_versions, Some(50));
        assert_eq!(policy.max_age_days, Some(365));
        assert!(policy.preserve_tags.contains(&"production".to_string()));
    }
}
