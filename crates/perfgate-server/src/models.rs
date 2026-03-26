//! Data models for the perfgate baseline service.
//!
//! These types represent the core domain objects and API request/response types
//! for the baseline storage service.

use chrono::Utc;
pub use perfgate_api::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Extends BaselineRecord with server-side logic.
pub trait BaselineRecordExt {
    #[allow(clippy::new_ret_no_self, clippy::too_many_arguments)]
    fn new(
        project: String,
        benchmark: String,
        version: String,
        receipt: perfgate_types::RunReceipt,
        git_ref: Option<String>,
        git_sha: Option<String>,
        metadata: BTreeMap<String, String>,
        tags: Vec<String>,
        source: BaselineSource,
    ) -> BaselineRecord;
}

impl BaselineRecordExt for BaselineRecord {
    fn new(
        project: String,
        benchmark: String,
        version: String,
        receipt: perfgate_types::RunReceipt,
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
}

/// Computes a content hash for a run receipt.
pub fn compute_content_hash(receipt: &perfgate_types::RunReceipt) -> String {
    let mut hasher = Sha256::new();
    let json = serde_json::to_string(receipt).unwrap_or_default();
    hasher.update(json.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generates a new unique identifier (ULID-like format).
pub fn generate_ulid() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
