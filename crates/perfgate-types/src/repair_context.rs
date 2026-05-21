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
    /// Optional agent-facing failure classification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<String>,
    /// Receipt and review artifacts relevant to this repair context.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifact_paths: Vec<String>,
    /// Copyable local reproduction command when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_reproduction_command: Option<String>,
    /// Commands an agent may suggest or run after review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub safe_commands: Vec<String>,
    /// Changes that require explicit human review and must not be made as a blind fix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forbidden_changes: Vec<String>,
    /// Policy or evidence decisions that require human review.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub human_review_required: Vec<String>,
    /// Commands to prove the repair after code changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proof_commands_after_repair: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_context_deserializes_without_agent_safe_fields() {
        let value = serde_json::json!({
            "schema": "perfgate.repair_context.v1",
            "benchmark": "bench-a",
            "verdict": {
                "status": "warn",
                "counts": {
                    "pass": 0,
                    "warn": 1,
                    "fail": 0,
                    "skip": 0
                },
                "reasons": ["no_baseline"]
            },
            "status": "warn",
            "breached_metrics": [],
            "report_path": "artifacts/perfgate/bench-a/report.json",
            "recommended_next_commands": [
                "rerun current command: cargo run -- --help"
            ]
        });

        let receipt: RepairContextReceipt =
            serde_json::from_value(value).expect("old repair context should deserialize");

        assert_eq!(receipt.schema, "perfgate.repair_context.v1");
        assert_eq!(receipt.benchmark, "bench-a");
        assert_eq!(receipt.failure_class, None);
        assert!(receipt.artifact_paths.is_empty());
        assert_eq!(receipt.local_reproduction_command, None);
        assert!(receipt.safe_commands.is_empty());
        assert!(receipt.forbidden_changes.is_empty());
        assert!(receipt.human_review_required.is_empty());
        assert!(receipt.proof_commands_after_repair.is_empty());
    }
}
