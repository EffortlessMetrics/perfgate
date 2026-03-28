//! Paired mode types for perfgate.

use crate::{F64Summary, RunMeta, Significance, ToolInfo, U64Summary};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub significance: Option<Significance>,
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

/// Noise level classification for paired benchmark results.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum NoiseLevel {
    /// CV <= 0.10 (10%)
    Low,
    /// 0.10 < CV <= 0.30 (30%)
    Moderate,
    /// CV > 0.30
    High,
}

impl NoiseLevel {
    /// Classify a coefficient of variation into a noise level.
    pub fn from_cv(cv: f64) -> Self {
        if cv <= 0.10 {
            NoiseLevel::Low
        } else if cv <= 0.30 {
            NoiseLevel::Moderate
        } else {
            NoiseLevel::High
        }
    }
}

impl fmt::Display for NoiseLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NoiseLevel::Low => write!(f, "low"),
            NoiseLevel::Moderate => write!(f, "moderate"),
            NoiseLevel::High => write!(f, "high"),
        }
    }
}

/// Diagnostics about noise in paired benchmark measurements.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct NoiseDiagnostics {
    /// Coefficient of variation of the wall-time differences.
    pub cv: f64,
    /// Classified noise level.
    pub noise_level: NoiseLevel,
    /// Number of retries used to achieve significance.
    pub retries_used: u32,
    /// Whether the retry loop terminated early due to excessive CV.
    pub early_termination: bool,
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
    /// Noise diagnostics from the paired run (present when retries were configured).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub noise_diagnostics: Option<NoiseDiagnostics>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HostInfo, RunMeta, ToolInfo, U64Summary};

    fn make_receipt() -> PairedRunReceipt {
        PairedRunReceipt {
            schema: PAIRED_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "run-id".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
                runner: None,
            },
            bench: PairedBenchMeta {
                name: "bench".to_string(),
                cwd: None,
                baseline_command: vec!["echo".to_string(), "baseline".to_string()],
                current_command: vec!["echo".to_string(), "current".to_string()],
                repeat: 2,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![PairedSample {
                pair_index: 0,
                warmup: false,
                baseline: PairedSampleHalf {
                    wall_ms: 100,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: None,
                    stdout: None,
                    stderr: None,
                },
                current: PairedSampleHalf {
                    wall_ms: 110,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: None,
                    stdout: None,
                    stderr: None,
                },
                wall_diff_ms: 10,
                rss_diff_kb: None,
            }],
            stats: PairedStats {
                baseline_wall_ms: U64Summary::new(100, 100, 100),
                current_wall_ms: U64Summary::new(110, 110, 110),
                wall_diff_ms: PairedDiffSummary {
                    mean: 10.0,
                    median: 10.0,
                    std_dev: 0.0,
                    min: 10.0,
                    max: 10.0,
                    count: 1,
                    significance: None,
                },
                baseline_max_rss_kb: None,
                current_max_rss_kb: None,
                rss_diff_kb: None,
                baseline_throughput_per_s: None,
                current_throughput_per_s: None,
                throughput_diff_per_s: None,
            },
            noise_diagnostics: None,
        }
    }

    #[test]
    fn paired_receipt_json_round_trip() {
        let receipt = make_receipt();
        let json = serde_json::to_string(&receipt).expect("serialize paired receipt");
        let decoded: PairedRunReceipt =
            serde_json::from_str(&json).expect("deserialize paired receipt");
        assert_eq!(decoded.schema, PAIRED_SCHEMA_V1);
        assert_eq!(decoded.bench.name, "bench");
        assert_eq!(decoded.samples.len(), 1);
        assert_eq!(decoded.stats.wall_diff_ms.count, 1);
    }

    #[test]
    fn paired_receipt_omits_optional_fields_when_none() {
        let receipt = make_receipt();
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&receipt).unwrap()).unwrap();

        let bench = &json["bench"];
        assert!(bench.get("cwd").is_none());
        assert!(bench.get("work_units").is_none());
        assert!(bench.get("timeout_ms").is_none());

        let sample = &json["samples"][0];
        assert!(sample["baseline"].get("max_rss_kb").is_none());
        assert!(sample["baseline"].get("stdout").is_none());
        assert!(sample["baseline"].get("stderr").is_none());

        assert!(json.get("noise_diagnostics").is_none());
    }

    #[test]
    fn noise_level_from_cv_classifies_correctly() {
        assert_eq!(NoiseLevel::from_cv(0.0), NoiseLevel::Low);
        assert_eq!(NoiseLevel::from_cv(0.05), NoiseLevel::Low);
        assert_eq!(NoiseLevel::from_cv(0.10), NoiseLevel::Low);
        assert_eq!(NoiseLevel::from_cv(0.15), NoiseLevel::Moderate);
        assert_eq!(NoiseLevel::from_cv(0.30), NoiseLevel::Moderate);
        assert_eq!(NoiseLevel::from_cv(0.31), NoiseLevel::High);
        assert_eq!(NoiseLevel::from_cv(0.50), NoiseLevel::High);
        assert_eq!(NoiseLevel::from_cv(1.0), NoiseLevel::High);
    }

    #[test]
    fn noise_level_display() {
        assert_eq!(NoiseLevel::Low.to_string(), "low");
        assert_eq!(NoiseLevel::Moderate.to_string(), "moderate");
        assert_eq!(NoiseLevel::High.to_string(), "high");
    }

    #[test]
    fn noise_diagnostics_json_round_trip() {
        let diag = NoiseDiagnostics {
            cv: 0.25,
            noise_level: NoiseLevel::Moderate,
            retries_used: 2,
            early_termination: false,
        };
        let json = serde_json::to_string(&diag).unwrap();
        let decoded: NoiseDiagnostics = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.cv, 0.25);
        assert_eq!(decoded.noise_level, NoiseLevel::Moderate);
        assert_eq!(decoded.retries_used, 2);
        assert!(!decoded.early_termination);
    }

    #[test]
    fn paired_receipt_with_noise_diagnostics_round_trip() {
        let mut receipt = make_receipt();
        receipt.noise_diagnostics = Some(NoiseDiagnostics {
            cv: 0.60,
            noise_level: NoiseLevel::High,
            retries_used: 3,
            early_termination: true,
        });
        let json = serde_json::to_string(&receipt).unwrap();
        let decoded: PairedRunReceipt = serde_json::from_str(&json).unwrap();
        let diag = decoded.noise_diagnostics.expect("should have diagnostics");
        assert_eq!(diag.cv, 0.60);
        assert_eq!(diag.noise_level, NoiseLevel::High);
        assert_eq!(diag.retries_used, 3);
        assert!(diag.early_termination);
    }
}
