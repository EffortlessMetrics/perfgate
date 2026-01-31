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
        m.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.18,
                direction: Direction::Lower,
            },
        );
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"wall_ms\""));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for generating valid non-empty strings (for names, IDs, etc.)
    fn non_empty_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,20}".prop_map(|s| s)
    }

    // Strategy for generating valid RFC3339 timestamps
    fn rfc3339_timestamp() -> impl Strategy<Value = String> {
        (
            2020u32..2030,
            1u32..13,
            1u32..29,
            0u32..24,
            0u32..60,
            0u32..60,
        )
            .prop_map(|(year, month, day, hour, min, sec)| {
                format!(
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                    year, month, day, hour, min, sec
                )
            })
    }

    // Strategy for ToolInfo
    fn tool_info_strategy() -> impl Strategy<Value = ToolInfo> {
        (non_empty_string(), non_empty_string())
            .prop_map(|(name, version)| ToolInfo { name, version })
    }

    // Strategy for HostInfo
    fn host_info_strategy() -> impl Strategy<Value = HostInfo> {
        (non_empty_string(), non_empty_string()).prop_map(|(os, arch)| HostInfo { os, arch })
    }

    // Strategy for RunMeta
    fn run_meta_strategy() -> impl Strategy<Value = RunMeta> {
        (
            non_empty_string(),
            rfc3339_timestamp(),
            rfc3339_timestamp(),
            host_info_strategy(),
        )
            .prop_map(|(id, started_at, ended_at, host)| RunMeta {
                id,
                started_at,
                ended_at,
                host,
            })
    }

    // Strategy for BenchMeta
    fn bench_meta_strategy() -> impl Strategy<Value = BenchMeta> {
        (
            non_empty_string(),
            proptest::option::of(non_empty_string()),
            proptest::collection::vec(non_empty_string(), 1..5),
            1u32..100,
            0u32..10,
            proptest::option::of(1u64..10000),
            proptest::option::of(100u64..60000),
        )
            .prop_map(
                |(name, cwd, command, repeat, warmup, work_units, timeout_ms)| BenchMeta {
                    name,
                    cwd,
                    command,
                    repeat,
                    warmup,
                    work_units,
                    timeout_ms,
                },
            )
    }

    // Strategy for Sample
    fn sample_strategy() -> impl Strategy<Value = Sample> {
        (
            0u64..100000,
            -128i32..128,
            any::<bool>(),
            any::<bool>(),
            proptest::option::of(0u64..1000000),
            proptest::option::of("[a-zA-Z0-9 ]{0,50}"),
            proptest::option::of("[a-zA-Z0-9 ]{0,50}"),
        )
            .prop_map(
                |(wall_ms, exit_code, warmup, timed_out, max_rss_kb, stdout, stderr)| Sample {
                    wall_ms,
                    exit_code,
                    warmup,
                    timed_out,
                    max_rss_kb,
                    stdout,
                    stderr,
                },
            )
    }

    // Strategy for U64Summary
    fn u64_summary_strategy() -> impl Strategy<Value = U64Summary> {
        (0u64..1000000, 0u64..1000000, 0u64..1000000).prop_map(|(a, b, c)| {
            let mut vals = [a, b, c];
            vals.sort();
            U64Summary {
                min: vals[0],
                median: vals[1],
                max: vals[2],
            }
        })
    }

    // Strategy for F64Summary - using finite positive floats
    fn f64_summary_strategy() -> impl Strategy<Value = F64Summary> {
        (0.0f64..1000000.0, 0.0f64..1000000.0, 0.0f64..1000000.0).prop_map(|(a, b, c)| {
            let mut vals = [a, b, c];
            vals.sort_by(|x, y| x.partial_cmp(y).unwrap());
            F64Summary {
                min: vals[0],
                median: vals[1],
                max: vals[2],
            }
        })
    }

    // Strategy for Stats
    fn stats_strategy() -> impl Strategy<Value = Stats> {
        (
            u64_summary_strategy(),
            proptest::option::of(u64_summary_strategy()),
            proptest::option::of(f64_summary_strategy()),
        )
            .prop_map(|(wall_ms, max_rss_kb, throughput_per_s)| Stats {
                wall_ms,
                max_rss_kb,
                throughput_per_s,
            })
    }

    // Strategy for RunReceipt
    fn run_receipt_strategy() -> impl Strategy<Value = RunReceipt> {
        (
            tool_info_strategy(),
            run_meta_strategy(),
            bench_meta_strategy(),
            proptest::collection::vec(sample_strategy(), 1..10),
            stats_strategy(),
        )
            .prop_map(|(tool, run, bench, samples, stats)| RunReceipt {
                schema: RUN_SCHEMA_V1.to_string(),
                tool,
                run,
                bench,
                samples,
                stats,
            })
    }

    // **Property 8: Serialization Round-Trip (RunReceipt)**
    //
    // For any valid RunReceipt, serializing to JSON then deserializing
    // SHALL produce an equivalent value.
    //
    // **Validates: Requirements 10.1**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn run_receipt_serialization_round_trip(receipt in run_receipt_strategy()) {
            // Serialize to JSON
            let json = serde_json::to_string(&receipt)
                .expect("RunReceipt should serialize to JSON");

            // Deserialize back
            let deserialized: RunReceipt = serde_json::from_str(&json)
                .expect("JSON should deserialize back to RunReceipt");

            // Compare - for f64 fields we need to handle floating point comparison
            prop_assert_eq!(&receipt.schema, &deserialized.schema);
            prop_assert_eq!(&receipt.tool, &deserialized.tool);
            prop_assert_eq!(&receipt.run, &deserialized.run);
            prop_assert_eq!(&receipt.bench, &deserialized.bench);
            prop_assert_eq!(receipt.samples.len(), deserialized.samples.len());

            // Compare samples
            for (orig, deser) in receipt.samples.iter().zip(deserialized.samples.iter()) {
                prop_assert_eq!(orig.wall_ms, deser.wall_ms);
                prop_assert_eq!(orig.exit_code, deser.exit_code);
                prop_assert_eq!(orig.warmup, deser.warmup);
                prop_assert_eq!(orig.timed_out, deser.timed_out);
                prop_assert_eq!(orig.max_rss_kb, deser.max_rss_kb);
                prop_assert_eq!(&orig.stdout, &deser.stdout);
                prop_assert_eq!(&orig.stderr, &deser.stderr);
            }

            // Compare stats
            prop_assert_eq!(&receipt.stats.wall_ms, &deserialized.stats.wall_ms);
            prop_assert_eq!(&receipt.stats.max_rss_kb, &deserialized.stats.max_rss_kb);

            // For f64 throughput, compare with tolerance for floating point
            // JSON serialization may lose some precision for large floats
            match (&receipt.stats.throughput_per_s, &deserialized.stats.throughput_per_s) {
                (Some(orig), Some(deser)) => {
                    // Use relative tolerance for floating point comparison
                    let rel_tol = |a: f64, b: f64| {
                        if a == 0.0 && b == 0.0 {
                            true
                        } else {
                            let max_val = a.abs().max(b.abs());
                            (a - b).abs() / max_val < 1e-10
                        }
                    };
                    prop_assert!(rel_tol(orig.min, deser.min), "min mismatch: {} vs {}", orig.min, deser.min);
                    prop_assert!(rel_tol(orig.median, deser.median), "median mismatch: {} vs {}", orig.median, deser.median);
                    prop_assert!(rel_tol(orig.max, deser.max), "max mismatch: {} vs {}", orig.max, deser.max);
                }
                (None, None) => {}
                _ => prop_assert!(false, "throughput_per_s presence mismatch"),
            }
        }
    }
}
