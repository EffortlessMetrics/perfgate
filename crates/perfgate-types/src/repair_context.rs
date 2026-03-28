use crate::{Direction, Verdict};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct DiagnosticsConfig {
    /// Emit `repair_context.json` artifacts for non-pass verdicts.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub emit_repair_context: Option<bool>,

    /// Emit `repair_context.json` for warn verdicts when diagnostics are enabled.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub emit_repair_context_on_warn: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RepairContextReceipt {
    pub schema: String,
    pub benchmark: String,
    pub verdict: Verdict,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breached_metrics: Vec<RepairMetricBreach>,
    pub artifacts: RepairArtifacts,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub profile_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub changed_files: Option<ChangedFilesSummary>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub git: Option<GitContext>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub spans: Option<SpanIdentifiers>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recommended_next_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RepairArtifacts {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub compare_receipt_path: Option<String>,
    pub report_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RepairMetricBreach {
    pub metric: String,
    pub status: String,
    pub baseline: f64,
    pub current: f64,
    pub threshold: f64,
    pub warn_threshold: f64,
    pub direction: Direction,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ChangedFilesSummary {
    pub total_files: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub count_by_root: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct GitContext {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SpanIdentifiers {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub span_id: Option<String>,
}

/// Redacts likely secrets from diagnostic command strings.
pub fn redact_sensitive_tokens(input: &str) -> String {
    const NEEDLES: [&str; 4] = ["token", "secret", "password", "apikey"];
    input
        .split_whitespace()
        .map(|segment| {
            if let Some((k, _)) = segment.split_once('=') {
                if NEEDLES
                    .iter()
                    .any(|needle| k.to_ascii_lowercase().contains(needle))
                {
                    return format!("{}=<redacted>", k);
                }
            }
            segment.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_sensitive_tokens_masks_common_secret_keys() {
        let cmd = "perfgate check API_TOKEN=abc PASSWORD=xyz SAFE=value";
        let redacted = redact_sensitive_tokens(cmd);
        assert!(redacted.contains("API_TOKEN=<redacted>"));
        assert!(redacted.contains("PASSWORD=<redacted>"));
        assert!(redacted.contains("SAFE=value"));
        assert!(!redacted.contains("abc"));
        assert!(!redacted.contains("xyz"));
    }
}
