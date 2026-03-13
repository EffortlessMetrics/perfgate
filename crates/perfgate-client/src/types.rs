//! Request and response types for the baseline service API.
//!
//! These types mirror the server's API contract defined in `perfgate-server`.

use chrono::{DateTime, Utc};
use perfgate_types::RunReceipt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ----------------------------
// Core Storage Models
// ----------------------------

/// The primary storage model for baselines.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BaselineRecord {
    /// Schema identifier (perfgate.baseline.v1).
    pub schema: String,
    /// Unique baseline identifier (ULID format).
    pub id: String,
    /// Project/namespace identifier.
    pub project: String,
    /// Benchmark name.
    pub benchmark: String,
    /// Semantic version for this baseline.
    pub version: String,
    /// Git reference (branch, tag, or ref).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Git commit SHA.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Full run receipt (perfgate.run.v1).
    pub receipt: RunReceipt,
    /// User-provided metadata.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// Tags for filtering.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creation timestamp (RFC 3339).
    pub created_at: DateTime<Utc>,
    /// Last modification timestamp.
    pub updated_at: DateTime<Utc>,
    /// Content hash for ETag/optimistic locking.
    pub content_hash: String,
    /// Creation source (upload, promote, migrate).
    pub source: BaselineSource,
    /// Soft delete flag.
    #[serde(default)]
    pub deleted: bool,
}

/// Source of baseline creation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum BaselineSource {
    /// Uploaded directly via API.
    #[default]
    Upload,
    /// Created via promote operation.
    Promote,
    /// Migrated from external storage.
    Migrate,
    /// Created via rollback operation.
    Rollback,
}

// ----------------------------
// API Request Types
// ----------------------------

/// Request to upload a new baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBaselineRequest {
    /// Benchmark name.
    pub benchmark: String,
    /// Version identifier (defaults to timestamp if not provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Git reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Git commit SHA.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Run receipt (perfgate.run.v1).
    pub receipt: RunReceipt,
    /// Optional metadata.
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
    /// Optional tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Normalize receipt before storing (strip run_id, timestamps).
    #[serde(default)]
    pub normalize: bool,
}

/// Request to promote a baseline version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteBaselineRequest {
    /// Source version to promote from.
    pub from_version: String,
    /// Target version identifier.
    pub to_version: String,
    /// Git reference for the promoted version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Git commit SHA for the promoted version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Normalize receipt during promotion.
    #[serde(default)]
    pub normalize: bool,
}

/// Query parameters for listing baselines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBaselinesQuery {
    /// Exact benchmark name match.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<String>,
    /// Benchmark name prefix filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark_prefix: Option<String>,
    /// Git reference filter (supports glob patterns).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Exact git SHA filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,
    /// Filter by tags (comma-separated, AND logic).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
    /// Filter baselines created after this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    /// Filter baselines created before this time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
    /// Maximum results (default: 50, max: 200).
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Pagination offset.
    #[serde(default)]
    pub offset: u64,
    /// Include full receipt in response.
    #[serde(default)]
    pub include_receipt: bool,
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

fn default_limit() -> u32 {
    50
}

impl ListBaselinesQuery {
    /// Creates a new query with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filters by benchmark name.
    pub fn with_benchmark(mut self, benchmark: impl Into<String>) -> Self {
        self.benchmark = Some(benchmark.into());
        self
    }

    /// Filters by benchmark name prefix.
    pub fn with_benchmark_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.benchmark_prefix = Some(prefix.into());
        self
    }

    /// Filters by git reference.
    pub fn with_git_ref(mut self, git_ref: impl Into<String>) -> Self {
        self.git_ref = Some(git_ref.into());
        self
    }

    /// Filters by tags (comma-separated).
    pub fn with_tags(mut self, tags: impl Into<String>) -> Self {
        self.tags = Some(tags.into());
        self
    }

    /// Sets the maximum number of results.
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit.min(200);
        self
    }

    /// Sets the pagination offset.
    pub fn with_offset(mut self, offset: u64) -> Self {
        self.offset = offset;
        self
    }

    /// Includes full receipts in the response.
    pub fn with_receipts(mut self) -> Self {
        self.include_receipt = true;
        self
    }

    /// Converts the query to URL query parameters.
    pub fn to_query_params(&self) -> Vec<(String, String)> {
        let mut params = Vec::new();

        if let Some(ref v) = self.benchmark {
            params.push(("benchmark".to_string(), v.clone()));
        }
        if let Some(ref v) = self.benchmark_prefix {
            params.push(("benchmark_prefix".to_string(), v.clone()));
        }
        if let Some(ref v) = self.git_ref {
            params.push(("git_ref".to_string(), v.clone()));
        }
        if let Some(ref v) = self.git_sha {
            params.push(("git_sha".to_string(), v.clone()));
        }
        if let Some(ref v) = self.tags {
            params.push(("tags".to_string(), v.clone()));
        }
        if let Some(ref v) = self.since {
            params.push(("since".to_string(), v.to_rfc3339()));
        }
        if let Some(ref v) = self.until {
            params.push(("until".to_string(), v.to_rfc3339()));
        }
        params.push(("limit".to_string(), self.limit.to_string()));
        params.push(("offset".to_string(), self.offset.to_string()));
        if self.include_receipt {
            params.push(("include_receipt".to_string(), "true".to_string()));
        }

        params
    }
}

// ----------------------------
// API Response Types
// ----------------------------

/// Response for successful baseline upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBaselineResponse {
    /// Unique baseline identifier.
    pub id: String,
    /// Benchmark name.
    pub benchmark: String,
    /// Version identifier.
    pub version: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// ETag for caching.
    pub etag: String,
}

/// Response for listing baselines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListBaselinesResponse {
    /// List of baseline summaries.
    pub baselines: Vec<BaselineSummary>,
    /// Pagination information.
    pub pagination: PaginationInfo,
}

/// Summary of a baseline (without full receipt by default).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineSummary {
    /// Unique baseline identifier.
    pub id: String,
    /// Benchmark name.
    pub benchmark: String,
    /// Version identifier.
    pub version: String,
    /// Git reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Full receipt (only included when include_receipt=true).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<RunReceipt>,
}

/// Pagination information for list responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationInfo {
    /// Total number of results.
    pub total: u64,
    /// Maximum results per page.
    pub limit: u32,
    /// Current offset.
    pub offset: u64,
    /// Whether more results exist.
    pub has_more: bool,
}

/// Response for baseline deletion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteBaselineResponse {
    /// Whether deletion was successful.
    pub deleted: bool,
    /// ID of the deleted baseline.
    pub id: String,
}

/// Response for baseline promotion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteBaselineResponse {
    /// Unique baseline identifier.
    pub id: String,
    /// Benchmark name.
    pub benchmark: String,
    /// New version identifier.
    pub version: String,
    /// Source version that was promoted.
    pub promoted_from: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

// ----------------------------
// Health Check Types
// ----------------------------

/// Health check response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Health status.
    pub status: String,
    /// Server version.
    pub version: String,
    /// Storage backend status.
    pub storage: StorageHealth,
}

/// Storage backend health status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageHealth {
    /// Backend type (memory, sqlite, postgres).
    pub backend: String,
    /// Connection status.
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_baselines_query_params() {
        let query = ListBaselinesQuery::new()
            .with_benchmark("my-bench")
            .with_limit(100)
            .with_offset(50)
            .with_receipts();

        let params = query.to_query_params();
        assert!(
            params
                .iter()
                .any(|(k, v)| k == "benchmark" && v == "my-bench")
        );
        assert!(params.iter().any(|(k, v)| k == "limit" && v == "100"));
        assert!(params.iter().any(|(k, v)| k == "offset" && v == "50"));
        assert!(
            params
                .iter()
                .any(|(k, v)| k == "include_receipt" && v == "true")
        );
    }

    #[test]
    fn test_list_baselines_query_limit_capped() {
        let query = ListBaselinesQuery::new().with_limit(500);
        assert_eq!(query.limit, 200); // Capped at max
    }

    #[test]
    fn test_baseline_source_serde() {
        let source = BaselineSource::Promote;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, "\"promote\"");
        let parsed: BaselineSource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, source);
    }
}
