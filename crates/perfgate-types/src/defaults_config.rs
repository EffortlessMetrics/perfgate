use crate::NoisePolicy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct DefaultsConfig {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub repeat: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub warmup: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub warn_factor: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub noise_threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub noise_policy: Option<NoisePolicy>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub out_dir: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_dir: Option<String>,

    /// Optional baseline discovery pattern. Supports `{bench}` placeholder.
    /// Example: `baselines/{bench}.json`.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub baseline_pattern: Option<String>,

    /// Optional Handlebars template path for markdown comments.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub markdown_template: Option<String>,
}

/// How a tradeoff rule downgrades a failed metric when requirements are met.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum TradeoffDowngradeTo {
    Warn,
    Pass,
}

/// A required improvement metric for a tradeoff rule.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffRequirement {
    pub metric: crate::Metric,
    pub min_improvement_ratio: f64,
}

/// Structured, auditable tradeoff rule.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct TradeoffRule {
    pub name: String,
    pub if_failed: crate::Metric,
    #[serde(default)]
    pub require: Vec<TradeoffRequirement>,
    pub downgrade_to: TradeoffDowngradeTo,
}
