//! Shared types for perfgate.
//!
//! Design goal: versioned, explicit, boring.
//! These structs are used for receipts, PR comments, and (eventually) long-term baselines.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const RUN_SCHEMA_V1: &str = "perfgate.run.v1";
pub const COMPARE_SCHEMA_V1: &str = "perfgate.compare.v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct RunMeta {
    pub id: String,
    pub started_at: String,
    pub ended_at: String,
    pub host: HostInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct BenchMeta {
    pub name: String,

    /// Optional working directory (stringified path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// argv vector (no shell parsing).
    pub command: Vec<String>,

    pub repeat: u32,
    pub warmup: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_units: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Sample {
    pub wall_ms: u64,
    pub exit_code: i32,

    #[serde(default)]
    pub warmup: bool,

    #[serde(default)]
    pub timed_out: bool,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rss_kb: Option<u64>,

    /// Truncated stdout (bytes interpreted as UTF-8 lossily).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,

    /// Truncated stderr (bytes interpreted as UTF-8 lossily).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct U64Summary {
    pub median: u64,
    pub min: u64,
    pub max: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct F64Summary {
    pub median: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Stats {
    pub wall_ms: U64Summary,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rss_kb: Option<U64Summary>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_per_s: Option<F64Summary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct RunReceipt {
    pub schema: String,
    pub tool: ToolInfo,
    pub run: RunMeta,
    pub bench: BenchMeta,
    pub samples: Vec<Sample>,
    pub stats: Stats,
}

#[derive(
    Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
#[serde(rename_all = "snake_case")]
pub enum Metric {
    WallMs,
    MaxRssKb,
    ThroughputPerS,
}

impl Metric {
    pub fn default_direction(self) -> Direction {
        match self {
            Metric::WallMs => Direction::Lower,
            Metric::MaxRssKb => Direction::Lower,
            Metric::ThroughputPerS => Direction::Higher,
        }
    }

    pub fn default_warn_factor(self) -> f64 {
        // Near-budget warnings are useful in PRs, but they should not fail by default.
        0.9
    }

    pub fn display_unit(self) -> &'static str {
        match self {
            Metric::WallMs => "ms",
            Metric::MaxRssKb => "KB",
            Metric::ThroughputPerS => "/s",
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Lower,
    Higher,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Budget {
    /// Fail threshold, as a fraction (0.20 = 20% regression allowed).
    pub threshold: f64,

    /// Warn threshold, as a fraction.
    pub warn_threshold: f64,

    pub direction: Direction,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct Delta {
    pub baseline: f64,
    pub current: f64,

    /// current / baseline
    pub ratio: f64,

    /// (current - baseline) / baseline
    pub pct: f64,

    /// Positive regression amount, normalized as a fraction.
    pub regression: f64,

    pub status: MetricStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CompareRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct VerdictCounts {
    pub pass: u32,
    pub warn: u32,
    pub fail: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Verdict {
    pub status: VerdictStatus,
    pub counts: VerdictCounts,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct CompareReceipt {
    pub schema: String,
    pub tool: ToolInfo,

    pub bench: BenchMeta,

    pub baseline_ref: CompareRef,
    pub current_ref: CompareRef,

    pub budgets: BTreeMap<Metric, Budget>,
    pub deltas: BTreeMap<Metric, Delta>,

    pub verdict: Verdict,
}

// ----------------------------
// Optional config file schema
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct ConfigFile {
    #[serde(default)]
    pub defaults: DefaultsConfig,

    #[serde(default, rename = "bench")]
    pub benches: Vec<BenchConfigFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct DefaultsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub warmup: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_factor: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub out_dir: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct BenchConfigFile {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<u64>,

    /// Duration string parseable by humantime, e.g. "2s".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,

    /// argv vector (no shell parsing).
    pub command: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Vec<Metric>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub budgets: Option<BTreeMap<Metric, BudgetOverride>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct BudgetOverride {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<Direction>,

    /// Warn fraction (warn_threshold = threshold * warn_factor).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warn_factor: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_serde_keys_are_snake_case() {
        let mut m = BTreeMap::new();
        m.insert(Metric::WallMs, Budget { threshold: 0.2, warn_threshold: 0.18, direction: Direction::Lower });
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"wall_ms\""));
    }
}
