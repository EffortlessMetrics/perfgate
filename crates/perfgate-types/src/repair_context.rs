use crate::{Metric, Verdict, VerdictStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A machine-readable failure package for automated triage.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RepairContextReceipt {
    /// Schema identifier, always "perfgate.repair_context.v1".
    pub schema: String,
    /// Benchmark name associated with the check.
    pub benchmark: String,
    /// Verdict from the check/compare workflow.
    pub verdict: Verdict,
    /// Optional top-level status for quick routing.
    pub status: VerdictStatus,
    /// Breached metrics (warn/fail/skip due to policy) with thresholds and values.
    pub breached_metrics: Vec<RepairMetricBreach>,
    /// Path to compare receipt when available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare_receipt_path: Option<String>,
    /// Path to report artifact.
    pub report_path: String,
    /// Optional profile artifact path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_path: Option<String>,
    /// Git metadata (if available).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<RepairGitMetadata>,
    /// Changed files summary from git working tree.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_files: Option<ChangedFilesSummary>,
    /// Optional OTel span identifiers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub otel_span: Option<OtelSpanIdentifiers>,
    /// Suggested next actions for humans/agents.
    pub recommended_next_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RepairMetricBreach {
    pub metric: Metric,
    pub status: String,
    pub baseline: f64,
    pub current: f64,
    pub regression: f64,
    pub fail_threshold: f64,
    pub warn_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RepairGitMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ChangedFilesSummary {
    pub file_count: u32,
    pub files: Vec<String>,
    pub file_count_by_top_level: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct OtelSpanIdentifiers {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_id: Option<String>,
}
