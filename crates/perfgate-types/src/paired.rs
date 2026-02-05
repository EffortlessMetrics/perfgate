//! Paired mode types for perfgate.

use crate::{F64Summary, RunMeta, ToolInfo, U64Summary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const PAIRED_SCHEMA_V1: &str = "perfgate.paired.v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PairedBenchMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub baseline_command: Vec<String>,
    pub current_command: Vec<String>,
    pub repeat: u32,
    pub warmup: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_units: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PairedSampleHalf {
    pub wall_ms: u64,
    pub exit_code: i32,
    pub timed_out: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rss_kb: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PairedSample {
    pub pair_index: u32,
    #[serde(default)]
    pub warmup: bool,
    pub baseline: PairedSampleHalf,
    pub current: PairedSampleHalf,
    pub wall_diff_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_diff_kb: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PairedDiffSummary {
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PairedStats {
    pub baseline_wall_ms: U64Summary,
    pub current_wall_ms: U64Summary,
    pub wall_diff_ms: PairedDiffSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_max_rss_kb: Option<U64Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_max_rss_kb: Option<U64Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rss_diff_kb: Option<PairedDiffSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_throughput_per_s: Option<F64Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_throughput_per_s: Option<F64Summary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_diff_per_s: Option<PairedDiffSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PairedRunReceipt {
    pub schema: String,
    pub tool: ToolInfo,
    pub run: RunMeta,
    pub bench: PairedBenchMeta,
    pub samples: Vec<PairedSample>,
    pub stats: PairedStats,
}
