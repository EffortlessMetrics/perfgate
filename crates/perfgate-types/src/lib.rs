//! Shared types for perfgate.
//!
//! Design goal: versioned, explicit, boring.
//! These structs are used for receipts, PR comments, and (eventually) long-term baselines.
//!
//! # Feature Flags
//!
//! - `arbitrary`: Enables `Arbitrary` derive for structure-aware fuzzing with cargo-fuzz.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const RUN_SCHEMA_V1: &str = "perfgate.run.v1";
pub const COMPARE_SCHEMA_V1: &str = "perfgate.compare.v1";
pub const REPORT_SCHEMA_V1: &str = "perfgate.report.v1";
pub const CONFIG_SCHEMA_V1: &str = "perfgate.config.v1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct HostInfo {
    /// Operating system (e.g., "linux", "macos", "windows")
    pub os: String,

    /// CPU architecture (e.g., "x86_64", "aarch64")
    pub arch: String,

    /// Number of logical CPUs (best-effort, None if unavailable)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cpu_count: Option<u32>,

    /// Total system memory in bytes (best-effort, None if unavailable)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub memory_bytes: Option<u64>,

    /// Hashed hostname for fingerprinting (opt-in, privacy-preserving).
    /// When present, this is a SHA-256 hash of the actual hostname.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hostname_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RunMeta {
    pub id: String,
    pub started_at: String,
    pub ended_at: String,
    pub host: HostInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct U64Summary {
    pub median: u64,
    pub min: u64,
    pub max: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct F64Summary {
    pub median: f64,
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Stats {
    pub wall_ms: U64Summary,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rss_kb: Option<U64Summary>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub throughput_per_s: Option<F64Summary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Lower,
    Higher,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Budget {
    /// Fail threshold, as a fraction (0.20 = 20% regression allowed).
    pub threshold: f64,

    /// Warn threshold, as a fraction.
    pub warn_threshold: f64,

    pub direction: Direction,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct CompareRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum VerdictStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct VerdictCounts {
    pub pass: u32,
    pub warn: u32,
    pub fail: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Verdict {
    pub status: VerdictStatus,
    pub counts: VerdictCounts,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
// Report types (perfgate.report.v1)
// ----------------------------

/// Severity level for a finding.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Warn,
    Fail,
}

/// Data associated with a metric finding.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct FindingData {
    /// Name of the metric (e.g., "wall_ms", "max_rss_kb").
    #[serde(rename = "metric_name")]
    pub metric_name: String,

    /// Baseline value.
    pub baseline: f64,

    /// Current value.
    pub current: f64,

    /// Regression percentage (positive means regression).
    #[serde(rename = "regression_pct")]
    pub regression_pct: f64,

    /// Threshold that was exceeded (as a fraction, e.g., 0.20 for 20%).
    pub threshold: f64,

    /// Whether lower is better or higher is better.
    pub direction: Direction,
}

/// A single finding from the performance check.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ReportFinding {
    /// Unique identifier for the check type (e.g., "perf.budget", "perf.baseline").
    #[serde(rename = "check_id")]
    pub check_id: String,

    /// Machine-readable code for the finding (e.g., "metric_warn", "metric_fail", "missing").
    pub code: String,

    /// Severity level (warn or fail).
    pub severity: Severity,

    /// Human-readable message describing the finding.
    pub message: String,

    /// Structured data about the finding (present for metric findings, absent for structural findings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<FindingData>,
}

/// Summary counts and key metrics for the report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ReportSummary {
    /// Number of metrics that passed.
    #[serde(rename = "pass_count")]
    pub pass_count: u32,

    /// Number of metrics that warned.
    #[serde(rename = "warn_count")]
    pub warn_count: u32,

    /// Number of metrics that failed.
    #[serde(rename = "fail_count")]
    pub fail_count: u32,

    /// Total number of metrics checked.
    #[serde(rename = "total_count")]
    pub total_count: u32,
}

/// A performance report wrapping compare results in a cockpit-compatible envelope.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct PerfgateReport {
    /// Schema identifier, always "perfgate.report.v1".
    #[serde(rename = "report_type")]
    pub report_type: String,

    /// Overall verdict for the report.
    pub verdict: Verdict,

    /// The full compare receipt (absent when baseline is missing).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compare: Option<CompareReceipt>,

    /// List of findings (warnings and failures).
    pub findings: Vec<ReportFinding>,

    /// Summary counts.
    pub summary: ReportSummary,
}

// ----------------------------
// Optional config file schema
// ----------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ConfigFile {
    #[serde(default)]
    pub defaults: DefaultsConfig,

    #[serde(default, rename = "bench")]
    pub benches: Vec<BenchConfigFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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

    /// Number of measured samples (overrides defaults.repeat).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u32>,

    /// Warmup samples excluded from stats (overrides defaults.warmup).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warmup: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metrics: Option<Vec<Metric>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub budgets: Option<BTreeMap<Metric, BudgetOverride>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
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

    /// Test backward compatibility: receipts without new host fields still parse
    #[test]
    fn backward_compat_host_info_without_new_fields() {
        // Old format with only os and arch
        let json = r#"{"os":"linux","arch":"x86_64"}"#;
        let info: HostInfo = serde_json::from_str(json).expect("should parse old format");
        assert_eq!(info.os, "linux");
        assert_eq!(info.arch, "x86_64");
        assert!(info.cpu_count.is_none());
        assert!(info.memory_bytes.is_none());
        assert!(info.hostname_hash.is_none());
    }

    /// Test that new fields are serialized when present
    #[test]
    fn host_info_with_new_fields_serializes() {
        let info = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: Some(8),
            memory_bytes: Some(16 * 1024 * 1024 * 1024),
            hostname_hash: Some("abc123".to_string()),
        };

        let json = serde_json::to_string(&info).expect("should serialize");
        assert!(json.contains("\"cpu_count\":8"));
        assert!(json.contains("\"memory_bytes\":"));
        assert!(json.contains("\"hostname_hash\":\"abc123\""));
    }

    /// Test that new fields are omitted when None (skip_serializing_if)
    #[test]
    fn host_info_omits_none_fields() {
        let info = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };

        let json = serde_json::to_string(&info).expect("should serialize");
        assert!(!json.contains("cpu_count"));
        assert!(!json.contains("memory_bytes"));
        assert!(!json.contains("hostname_hash"));
    }

    /// Test round-trip serialization with all fields
    #[test]
    fn host_info_round_trip_with_all_fields() {
        let original = HostInfo {
            os: "macos".to_string(),
            arch: "aarch64".to_string(),
            cpu_count: Some(10),
            memory_bytes: Some(32 * 1024 * 1024 * 1024),
            hostname_hash: Some("deadbeef".repeat(8)),
        };

        let json = serde_json::to_string(&original).expect("should serialize");
        let parsed: HostInfo = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(original, parsed);
    }

    /// Test backward compatibility: full RunReceipt without new host fields parses
    #[test]
    fn backward_compat_run_receipt_old_format() {
        let json = r#"{
            "schema": "perfgate.run.v1",
            "tool": {"name": "perfgate", "version": "0.1.0"},
            "run": {
                "id": "test-id",
                "started_at": "2024-01-01T00:00:00Z",
                "ended_at": "2024-01-01T00:01:00Z",
                "host": {"os": "linux", "arch": "x86_64"}
            },
            "bench": {
                "name": "test",
                "command": ["echo", "hello"],
                "repeat": 5,
                "warmup": 0
            },
            "samples": [{"wall_ms": 100, "exit_code": 0}],
            "stats": {
                "wall_ms": {"median": 100, "min": 90, "max": 110}
            }
        }"#;

        let receipt: RunReceipt = serde_json::from_str(json).expect("should parse old format");
        assert_eq!(receipt.run.host.os, "linux");
        assert_eq!(receipt.run.host.arch, "x86_64");
        assert!(receipt.run.host.cpu_count.is_none());
        assert!(receipt.run.host.memory_bytes.is_none());
        assert!(receipt.run.host.hostname_hash.is_none());
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
        (
            non_empty_string(),
            non_empty_string(),
            proptest::option::of(1u32..256),
            proptest::option::of(1u64..68719476736), // Up to 64GB
            proptest::option::of("[a-f0-9]{64}"),    // SHA-256 hex hash
        )
            .prop_map(
                |(os, arch, cpu_count, memory_bytes, hostname_hash)| HostInfo {
                    os,
                    arch,
                    cpu_count,
                    memory_bytes,
                    hostname_hash,
                },
            )
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

    // --- Strategies for CompareReceipt ---

    // Strategy for CompareRef
    fn compare_ref_strategy() -> impl Strategy<Value = CompareRef> {
        (
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
        )
            .prop_map(|(path, run_id)| CompareRef { path, run_id })
    }

    // Strategy for Direction
    fn direction_strategy() -> impl Strategy<Value = Direction> {
        prop_oneof![Just(Direction::Lower), Just(Direction::Higher),]
    }

    // Strategy for Budget - using finite positive floats for thresholds
    fn budget_strategy() -> impl Strategy<Value = Budget> {
        (0.01f64..1.0, 0.01f64..1.0, direction_strategy()).prop_map(
            |(threshold, warn_factor, direction)| {
                // warn_threshold should be <= threshold
                let warn_threshold = threshold * warn_factor;
                Budget {
                    threshold,
                    warn_threshold,
                    direction,
                }
            },
        )
    }

    // Strategy for MetricStatus
    fn metric_status_strategy() -> impl Strategy<Value = MetricStatus> {
        prop_oneof![
            Just(MetricStatus::Pass),
            Just(MetricStatus::Warn),
            Just(MetricStatus::Fail),
        ]
    }

    // Strategy for Delta - using finite positive floats
    fn delta_strategy() -> impl Strategy<Value = Delta> {
        (
            0.1f64..10000.0, // baseline (positive, non-zero)
            0.1f64..10000.0, // current (positive, non-zero)
            metric_status_strategy(),
        )
            .prop_map(|(baseline, current, status)| {
                let ratio = current / baseline;
                let pct = (current - baseline) / baseline;
                let regression = if pct > 0.0 { pct } else { 0.0 };
                Delta {
                    baseline,
                    current,
                    ratio,
                    pct,
                    regression,
                    status,
                }
            })
    }

    // Strategy for VerdictStatus
    fn verdict_status_strategy() -> impl Strategy<Value = VerdictStatus> {
        prop_oneof![
            Just(VerdictStatus::Pass),
            Just(VerdictStatus::Warn),
            Just(VerdictStatus::Fail),
        ]
    }

    // Strategy for VerdictCounts
    fn verdict_counts_strategy() -> impl Strategy<Value = VerdictCounts> {
        (0u32..10, 0u32..10, 0u32..10).prop_map(|(pass, warn, fail)| VerdictCounts {
            pass,
            warn,
            fail,
        })
    }

    // Strategy for Verdict
    fn verdict_strategy() -> impl Strategy<Value = Verdict> {
        (
            verdict_status_strategy(),
            verdict_counts_strategy(),
            proptest::collection::vec("[a-zA-Z0-9 ]{1,50}", 0..5),
        )
            .prop_map(|(status, counts, reasons)| Verdict {
                status,
                counts,
                reasons,
            })
    }

    // Strategy for Metric
    fn metric_strategy() -> impl Strategy<Value = Metric> {
        prop_oneof![
            Just(Metric::WallMs),
            Just(Metric::MaxRssKb),
            Just(Metric::ThroughputPerS),
        ]
    }

    // Strategy for BTreeMap<Metric, Budget>
    fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
        proptest::collection::btree_map(metric_strategy(), budget_strategy(), 0..4)
    }

    // Strategy for BTreeMap<Metric, Delta>
    fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
        proptest::collection::btree_map(metric_strategy(), delta_strategy(), 0..4)
    }

    // Strategy for CompareReceipt
    fn compare_receipt_strategy() -> impl Strategy<Value = CompareReceipt> {
        (
            tool_info_strategy(),
            bench_meta_strategy(),
            compare_ref_strategy(),
            compare_ref_strategy(),
            budgets_map_strategy(),
            deltas_map_strategy(),
            verdict_strategy(),
        )
            .prop_map(
                |(tool, bench, baseline_ref, current_ref, budgets, deltas, verdict)| {
                    CompareReceipt {
                        schema: COMPARE_SCHEMA_V1.to_string(),
                        tool,
                        bench,
                        baseline_ref,
                        current_ref,
                        budgets,
                        deltas,
                        verdict,
                    }
                },
            )
    }

    // Helper function for comparing f64 values with tolerance
    fn f64_approx_eq(a: f64, b: f64) -> bool {
        if a == 0.0 && b == 0.0 {
            true
        } else {
            let max_val = a.abs().max(b.abs());
            if max_val == 0.0 {
                true
            } else {
                (a - b).abs() / max_val < 1e-10
            }
        }
    }

    // **Property 8: Serialization Round-Trip (CompareReceipt)**
    //
    // For any valid CompareReceipt, serializing to JSON then deserializing
    // SHALL produce an equivalent value.
    //
    // **Validates: Requirements 10.2**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn compare_receipt_serialization_round_trip(receipt in compare_receipt_strategy()) {
            // Serialize to JSON
            let json = serde_json::to_string(&receipt)
                .expect("CompareReceipt should serialize to JSON");

            // Deserialize back
            let deserialized: CompareReceipt = serde_json::from_str(&json)
                .expect("JSON should deserialize back to CompareReceipt");

            // Compare non-f64 fields directly
            prop_assert_eq!(&receipt.schema, &deserialized.schema);
            prop_assert_eq!(&receipt.tool, &deserialized.tool);
            prop_assert_eq!(&receipt.bench, &deserialized.bench);
            prop_assert_eq!(&receipt.baseline_ref, &deserialized.baseline_ref);
            prop_assert_eq!(&receipt.current_ref, &deserialized.current_ref);
            prop_assert_eq!(&receipt.verdict, &deserialized.verdict);

            // Compare budgets map - contains f64 fields
            prop_assert_eq!(receipt.budgets.len(), deserialized.budgets.len());
            for (metric, orig_budget) in &receipt.budgets {
                let deser_budget = deserialized.budgets.get(metric)
                    .expect("Budget metric should exist in deserialized");
                prop_assert!(
                    f64_approx_eq(orig_budget.threshold, deser_budget.threshold),
                    "Budget threshold mismatch for {:?}: {} vs {}",
                    metric, orig_budget.threshold, deser_budget.threshold
                );
                prop_assert!(
                    f64_approx_eq(orig_budget.warn_threshold, deser_budget.warn_threshold),
                    "Budget warn_threshold mismatch for {:?}: {} vs {}",
                    metric, orig_budget.warn_threshold, deser_budget.warn_threshold
                );
                prop_assert_eq!(orig_budget.direction, deser_budget.direction);
            }

            // Compare deltas map - contains f64 fields
            prop_assert_eq!(receipt.deltas.len(), deserialized.deltas.len());
            for (metric, orig_delta) in &receipt.deltas {
                let deser_delta = deserialized.deltas.get(metric)
                    .expect("Delta metric should exist in deserialized");
                prop_assert!(
                    f64_approx_eq(orig_delta.baseline, deser_delta.baseline),
                    "Delta baseline mismatch for {:?}: {} vs {}",
                    metric, orig_delta.baseline, deser_delta.baseline
                );
                prop_assert!(
                    f64_approx_eq(orig_delta.current, deser_delta.current),
                    "Delta current mismatch for {:?}: {} vs {}",
                    metric, orig_delta.current, deser_delta.current
                );
                prop_assert!(
                    f64_approx_eq(orig_delta.ratio, deser_delta.ratio),
                    "Delta ratio mismatch for {:?}: {} vs {}",
                    metric, orig_delta.ratio, deser_delta.ratio
                );
                prop_assert!(
                    f64_approx_eq(orig_delta.pct, deser_delta.pct),
                    "Delta pct mismatch for {:?}: {} vs {}",
                    metric, orig_delta.pct, deser_delta.pct
                );
                prop_assert!(
                    f64_approx_eq(orig_delta.regression, deser_delta.regression),
                    "Delta regression mismatch for {:?}: {} vs {}",
                    metric, orig_delta.regression, deser_delta.regression
                );
                prop_assert_eq!(orig_delta.status, deser_delta.status);
            }
        }
    }

    // --- Strategies for ConfigFile ---

    // Strategy for BudgetOverride
    fn budget_override_strategy() -> impl Strategy<Value = BudgetOverride> {
        (
            proptest::option::of(0.01f64..1.0),
            proptest::option::of(direction_strategy()),
            proptest::option::of(0.5f64..1.0),
        )
            .prop_map(|(threshold, direction, warn_factor)| BudgetOverride {
                threshold,
                direction,
                warn_factor,
            })
    }

    // Strategy for BTreeMap<Metric, BudgetOverride>
    fn budget_overrides_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, BudgetOverride>> {
        proptest::collection::btree_map(metric_strategy(), budget_override_strategy(), 0..4)
    }

    // Strategy for BenchConfigFile
    fn bench_config_file_strategy() -> impl Strategy<Value = BenchConfigFile> {
        (
            non_empty_string(),
            proptest::option::of(non_empty_string()),
            proptest::option::of(1u64..10000),
            proptest::option::of("[0-9]+[smh]"), // humantime-like duration strings
            proptest::collection::vec(non_empty_string(), 1..5),
            proptest::option::of(1u32..100),
            proptest::option::of(0u32..10),
            proptest::option::of(proptest::collection::vec(metric_strategy(), 1..4)),
            proptest::option::of(budget_overrides_map_strategy()),
        )
            .prop_map(
                |(name, cwd, work, timeout, command, repeat, warmup, metrics, budgets)| {
                    BenchConfigFile {
                        name,
                        cwd,
                        work,
                        timeout,
                        command,
                        repeat,
                        warmup,
                        metrics,
                        budgets,
                    }
                },
            )
    }

    // Strategy for DefaultsConfig
    fn defaults_config_strategy() -> impl Strategy<Value = DefaultsConfig> {
        (
            proptest::option::of(1u32..100),
            proptest::option::of(0u32..10),
            proptest::option::of(0.01f64..1.0),
            proptest::option::of(0.5f64..1.0),
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
        )
            .prop_map(
                |(repeat, warmup, threshold, warn_factor, out_dir, baseline_dir)| DefaultsConfig {
                    repeat,
                    warmup,
                    threshold,
                    warn_factor,
                    out_dir,
                    baseline_dir,
                },
            )
    }

    // Strategy for ConfigFile
    fn config_file_strategy() -> impl Strategy<Value = ConfigFile> {
        (
            defaults_config_strategy(),
            proptest::collection::vec(bench_config_file_strategy(), 0..5),
        )
            .prop_map(|(defaults, benches)| ConfigFile { defaults, benches })
    }

    // **Feature: comprehensive-test-coverage, Property 1: JSON Serialization Round-Trip**
    //
    // For any valid ConfigFile, serializing to JSON then deserializing
    // SHALL produce an equivalent value.
    //
    // **Validates: Requirements 4.2, 4.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn config_file_json_serialization_round_trip(config in config_file_strategy()) {
            // Serialize to JSON
            let json = serde_json::to_string(&config)
                .expect("ConfigFile should serialize to JSON");

            // Deserialize back
            let deserialized: ConfigFile = serde_json::from_str(&json)
                .expect("JSON should deserialize back to ConfigFile");

            // Compare defaults
            prop_assert_eq!(config.defaults.repeat, deserialized.defaults.repeat);
            prop_assert_eq!(config.defaults.warmup, deserialized.defaults.warmup);
            prop_assert_eq!(&config.defaults.out_dir, &deserialized.defaults.out_dir);
            prop_assert_eq!(&config.defaults.baseline_dir, &deserialized.defaults.baseline_dir);

            // Compare f64 fields in defaults with tolerance
            match (config.defaults.threshold, deserialized.defaults.threshold) {
                (Some(orig), Some(deser)) => {
                    prop_assert!(
                        f64_approx_eq(orig, deser),
                        "defaults.threshold mismatch: {} vs {}",
                        orig, deser
                    );
                }
                (None, None) => {}
                _ => prop_assert!(false, "defaults.threshold presence mismatch"),
            }

            match (config.defaults.warn_factor, deserialized.defaults.warn_factor) {
                (Some(orig), Some(deser)) => {
                    prop_assert!(
                        f64_approx_eq(orig, deser),
                        "defaults.warn_factor mismatch: {} vs {}",
                        orig, deser
                    );
                }
                (None, None) => {}
                _ => prop_assert!(false, "defaults.warn_factor presence mismatch"),
            }

            // Compare benches
            prop_assert_eq!(config.benches.len(), deserialized.benches.len());
            for (orig_bench, deser_bench) in config.benches.iter().zip(deserialized.benches.iter()) {
                prop_assert_eq!(&orig_bench.name, &deser_bench.name);
                prop_assert_eq!(&orig_bench.cwd, &deser_bench.cwd);
                prop_assert_eq!(orig_bench.work, deser_bench.work);
                prop_assert_eq!(&orig_bench.timeout, &deser_bench.timeout);
                prop_assert_eq!(&orig_bench.command, &deser_bench.command);
                prop_assert_eq!(&orig_bench.metrics, &deser_bench.metrics);

                // Compare budgets map with f64 tolerance
                match (&orig_bench.budgets, &deser_bench.budgets) {
                    (Some(orig_budgets), Some(deser_budgets)) => {
                        prop_assert_eq!(orig_budgets.len(), deser_budgets.len());
                        for (metric, orig_override) in orig_budgets {
                            let deser_override = deser_budgets.get(metric)
                                .expect("BudgetOverride metric should exist in deserialized");

                            // Compare threshold with tolerance
                            match (orig_override.threshold, deser_override.threshold) {
                                (Some(orig), Some(deser)) => {
                                    prop_assert!(
                                        f64_approx_eq(orig, deser),
                                        "BudgetOverride threshold mismatch for {:?}: {} vs {}",
                                        metric, orig, deser
                                    );
                                }
                                (None, None) => {}
                                _ => prop_assert!(false, "BudgetOverride threshold presence mismatch for {:?}", metric),
                            }

                            prop_assert_eq!(orig_override.direction, deser_override.direction);

                            // Compare warn_factor with tolerance
                            match (orig_override.warn_factor, deser_override.warn_factor) {
                                (Some(orig), Some(deser)) => {
                                    prop_assert!(
                                        f64_approx_eq(orig, deser),
                                        "BudgetOverride warn_factor mismatch for {:?}: {} vs {}",
                                        metric, orig, deser
                                    );
                                }
                                (None, None) => {}
                                _ => prop_assert!(false, "BudgetOverride warn_factor presence mismatch for {:?}", metric),
                            }
                        }
                    }
                    (None, None) => {}
                    _ => prop_assert!(false, "bench.budgets presence mismatch"),
                }
            }
        }
    }

    // **Feature: comprehensive-test-coverage, Property 1: JSON Serialization Round-Trip (TOML variant)**
    //
    // For any valid ConfigFile, serializing to TOML then deserializing
    // SHALL produce an equivalent value.
    //
    // **Validates: Requirements 4.2, 4.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn config_file_toml_serialization_round_trip(config in config_file_strategy()) {
            // Serialize to TOML
            let toml_str = toml::to_string(&config)
                .expect("ConfigFile should serialize to TOML");

            // Deserialize back
            let deserialized: ConfigFile = toml::from_str(&toml_str)
                .expect("TOML should deserialize back to ConfigFile");

            // Compare defaults
            prop_assert_eq!(config.defaults.repeat, deserialized.defaults.repeat);
            prop_assert_eq!(config.defaults.warmup, deserialized.defaults.warmup);
            prop_assert_eq!(&config.defaults.out_dir, &deserialized.defaults.out_dir);
            prop_assert_eq!(&config.defaults.baseline_dir, &deserialized.defaults.baseline_dir);

            // Compare f64 fields in defaults with tolerance
            match (config.defaults.threshold, deserialized.defaults.threshold) {
                (Some(orig), Some(deser)) => {
                    prop_assert!(
                        f64_approx_eq(orig, deser),
                        "defaults.threshold mismatch: {} vs {}",
                        orig, deser
                    );
                }
                (None, None) => {}
                _ => prop_assert!(false, "defaults.threshold presence mismatch"),
            }

            match (config.defaults.warn_factor, deserialized.defaults.warn_factor) {
                (Some(orig), Some(deser)) => {
                    prop_assert!(
                        f64_approx_eq(orig, deser),
                        "defaults.warn_factor mismatch: {} vs {}",
                        orig, deser
                    );
                }
                (None, None) => {}
                _ => prop_assert!(false, "defaults.warn_factor presence mismatch"),
            }

            // Compare benches
            prop_assert_eq!(config.benches.len(), deserialized.benches.len());
            for (orig_bench, deser_bench) in config.benches.iter().zip(deserialized.benches.iter()) {
                prop_assert_eq!(&orig_bench.name, &deser_bench.name);
                prop_assert_eq!(&orig_bench.cwd, &deser_bench.cwd);
                prop_assert_eq!(orig_bench.work, deser_bench.work);
                prop_assert_eq!(&orig_bench.timeout, &deser_bench.timeout);
                prop_assert_eq!(&orig_bench.command, &deser_bench.command);
                prop_assert_eq!(&orig_bench.metrics, &deser_bench.metrics);

                // Compare budgets map with f64 tolerance
                match (&orig_bench.budgets, &deser_bench.budgets) {
                    (Some(orig_budgets), Some(deser_budgets)) => {
                        prop_assert_eq!(orig_budgets.len(), deser_budgets.len());
                        for (metric, orig_override) in orig_budgets {
                            let deser_override = deser_budgets.get(metric)
                                .expect("BudgetOverride metric should exist in deserialized");

                            // Compare threshold with tolerance
                            match (orig_override.threshold, deser_override.threshold) {
                                (Some(orig), Some(deser)) => {
                                    prop_assert!(
                                        f64_approx_eq(orig, deser),
                                        "BudgetOverride threshold mismatch for {:?}: {} vs {}",
                                        metric, orig, deser
                                    );
                                }
                                (None, None) => {}
                                _ => prop_assert!(false, "BudgetOverride threshold presence mismatch for {:?}", metric),
                            }

                            prop_assert_eq!(orig_override.direction, deser_override.direction);

                            // Compare warn_factor with tolerance
                            match (orig_override.warn_factor, deser_override.warn_factor) {
                                (Some(orig), Some(deser)) => {
                                    prop_assert!(
                                        f64_approx_eq(orig, deser),
                                        "BudgetOverride warn_factor mismatch for {:?}: {} vs {}",
                                        metric, orig, deser
                                    );
                                }
                                (None, None) => {}
                                _ => prop_assert!(false, "BudgetOverride warn_factor presence mismatch for {:?}", metric),
                            }
                        }
                    }
                    (None, None) => {}
                    _ => prop_assert!(false, "bench.budgets presence mismatch"),
                }
            }
        }
    }

    // **Feature: comprehensive-test-coverage, Property 1: JSON Serialization Round-Trip**
    //
    // For any valid BenchConfigFile, serializing to JSON then deserializing
    // SHALL produce an equivalent value. This tests the BenchConfigFile type
    // in isolation with all optional fields.
    //
    // **Validates: Requirements 4.2, 4.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn bench_config_file_json_serialization_round_trip(bench_config in bench_config_file_strategy()) {
            // Serialize to JSON
            let json = serde_json::to_string(&bench_config)
                .expect("BenchConfigFile should serialize to JSON");

            // Deserialize back
            let deserialized: BenchConfigFile = serde_json::from_str(&json)
                .expect("JSON should deserialize back to BenchConfigFile");

            // Compare required fields
            prop_assert_eq!(&bench_config.name, &deserialized.name);
            prop_assert_eq!(&bench_config.command, &deserialized.command);

            // Compare optional fields
            prop_assert_eq!(&bench_config.cwd, &deserialized.cwd);
            prop_assert_eq!(bench_config.work, deserialized.work);
            prop_assert_eq!(&bench_config.timeout, &deserialized.timeout);
            prop_assert_eq!(&bench_config.metrics, &deserialized.metrics);

            // Compare budgets map with f64 tolerance
            match (&bench_config.budgets, &deserialized.budgets) {
                (Some(orig_budgets), Some(deser_budgets)) => {
                    prop_assert_eq!(orig_budgets.len(), deser_budgets.len());
                    for (metric, orig_override) in orig_budgets {
                        let deser_override = deser_budgets.get(metric)
                            .expect("BudgetOverride metric should exist in deserialized");

                        // Compare threshold with tolerance
                        match (orig_override.threshold, deser_override.threshold) {
                            (Some(orig), Some(deser)) => {
                                prop_assert!(
                                    f64_approx_eq(orig, deser),
                                    "BudgetOverride threshold mismatch for {:?}: {} vs {}",
                                    metric, orig, deser
                                );
                            }
                            (None, None) => {}
                            _ => prop_assert!(false, "BudgetOverride threshold presence mismatch for {:?}", metric),
                        }

                        prop_assert_eq!(orig_override.direction, deser_override.direction);

                        // Compare warn_factor with tolerance
                        match (orig_override.warn_factor, deser_override.warn_factor) {
                            (Some(orig), Some(deser)) => {
                                prop_assert!(
                                    f64_approx_eq(orig, deser),
                                    "BudgetOverride warn_factor mismatch for {:?}: {} vs {}",
                                    metric, orig, deser
                                );
                            }
                            (None, None) => {}
                            _ => prop_assert!(false, "BudgetOverride warn_factor presence mismatch for {:?}", metric),
                        }
                    }
                }
                (None, None) => {}
                _ => prop_assert!(false, "budgets presence mismatch"),
            }
        }
    }

    // **Feature: comprehensive-test-coverage, Property 1: JSON Serialization Round-Trip (TOML variant)**
    //
    // For any valid BenchConfigFile, serializing to TOML then deserializing
    // SHALL produce an equivalent value. This tests the BenchConfigFile type
    // in isolation with all optional fields.
    //
    // **Validates: Requirements 4.2, 4.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn bench_config_file_toml_serialization_round_trip(bench_config in bench_config_file_strategy()) {
            // Serialize to TOML
            let toml_str = toml::to_string(&bench_config)
                .expect("BenchConfigFile should serialize to TOML");

            // Deserialize back
            let deserialized: BenchConfigFile = toml::from_str(&toml_str)
                .expect("TOML should deserialize back to BenchConfigFile");

            // Compare required fields
            prop_assert_eq!(&bench_config.name, &deserialized.name);
            prop_assert_eq!(&bench_config.command, &deserialized.command);

            // Compare optional fields
            prop_assert_eq!(&bench_config.cwd, &deserialized.cwd);
            prop_assert_eq!(bench_config.work, deserialized.work);
            prop_assert_eq!(&bench_config.timeout, &deserialized.timeout);
            prop_assert_eq!(&bench_config.metrics, &deserialized.metrics);

            // Compare budgets map with f64 tolerance
            match (&bench_config.budgets, &deserialized.budgets) {
                (Some(orig_budgets), Some(deser_budgets)) => {
                    prop_assert_eq!(orig_budgets.len(), deser_budgets.len());
                    for (metric, orig_override) in orig_budgets {
                        let deser_override = deser_budgets.get(metric)
                            .expect("BudgetOverride metric should exist in deserialized");

                        // Compare threshold with tolerance
                        match (orig_override.threshold, deser_override.threshold) {
                            (Some(orig), Some(deser)) => {
                                prop_assert!(
                                    f64_approx_eq(orig, deser),
                                    "BudgetOverride threshold mismatch for {:?}: {} vs {}",
                                    metric, orig, deser
                                );
                            }
                            (None, None) => {}
                            _ => prop_assert!(false, "BudgetOverride threshold presence mismatch for {:?}", metric),
                        }

                        prop_assert_eq!(orig_override.direction, deser_override.direction);

                        // Compare warn_factor with tolerance
                        match (orig_override.warn_factor, deser_override.warn_factor) {
                            (Some(orig), Some(deser)) => {
                                prop_assert!(
                                    f64_approx_eq(orig, deser),
                                    "BudgetOverride warn_factor mismatch for {:?}: {} vs {}",
                                    metric, orig, deser
                                );
                            }
                            (None, None) => {}
                            _ => prop_assert!(false, "BudgetOverride warn_factor presence mismatch for {:?}", metric),
                        }
                    }
                }
                (None, None) => {}
                _ => prop_assert!(false, "budgets presence mismatch"),
            }
        }
    }

    // **Feature: comprehensive-test-coverage, Property 1: JSON Serialization Round-Trip**
    //
    // For any valid Budget, serializing to JSON then deserializing
    // SHALL produce an equivalent value.
    //
    // **Validates: Requirements 4.2, 4.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn budget_json_serialization_round_trip(budget in budget_strategy()) {
            // Serialize to JSON
            let json = serde_json::to_string(&budget)
                .expect("Budget should serialize to JSON");

            // Deserialize back
            let deserialized: Budget = serde_json::from_str(&json)
                .expect("JSON should deserialize back to Budget");

            // Compare f64 fields with tolerance
            prop_assert!(
                f64_approx_eq(budget.threshold, deserialized.threshold),
                "Budget threshold mismatch: {} vs {}",
                budget.threshold, deserialized.threshold
            );
            prop_assert!(
                f64_approx_eq(budget.warn_threshold, deserialized.warn_threshold),
                "Budget warn_threshold mismatch: {} vs {}",
                budget.warn_threshold, deserialized.warn_threshold
            );
            prop_assert_eq!(budget.direction, deserialized.direction);
        }
    }

    // **Feature: comprehensive-test-coverage, Property 1: JSON Serialization Round-Trip**
    //
    // For any valid BudgetOverride, serializing to JSON then deserializing
    // SHALL produce an equivalent value.
    //
    // **Validates: Requirements 4.2, 4.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn budget_override_json_serialization_round_trip(budget_override in budget_override_strategy()) {
            // Serialize to JSON
            let json = serde_json::to_string(&budget_override)
                .expect("BudgetOverride should serialize to JSON");

            // Deserialize back
            let deserialized: BudgetOverride = serde_json::from_str(&json)
                .expect("JSON should deserialize back to BudgetOverride");

            // Compare threshold with tolerance
            match (budget_override.threshold, deserialized.threshold) {
                (Some(orig), Some(deser)) => {
                    prop_assert!(
                        f64_approx_eq(orig, deser),
                        "BudgetOverride threshold mismatch: {} vs {}",
                        orig, deser
                    );
                }
                (None, None) => {}
                _ => prop_assert!(false, "BudgetOverride threshold presence mismatch"),
            }

            // Compare direction
            prop_assert_eq!(budget_override.direction, deserialized.direction);

            // Compare warn_factor with tolerance
            match (budget_override.warn_factor, deserialized.warn_factor) {
                (Some(orig), Some(deser)) => {
                    prop_assert!(
                        f64_approx_eq(orig, deser),
                        "BudgetOverride warn_factor mismatch: {} vs {}",
                        orig, deser
                    );
                }
                (None, None) => {}
                _ => prop_assert!(false, "BudgetOverride warn_factor presence mismatch"),
            }
        }
    }

    // **Feature: comprehensive-test-coverage, Property 1: JSON Serialization Round-Trip**
    //
    // For any valid Budget, the threshold relationship SHALL be preserved:
    // warn_threshold <= threshold. This property verifies that the Budget
    // strategy generates valid budgets and that serialization preserves
    // this invariant.
    //
    // **Validates: Requirements 4.2, 4.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn budget_threshold_relationship_preserved(budget in budget_strategy()) {
            // Verify the invariant holds for the generated budget
            prop_assert!(
                budget.warn_threshold <= budget.threshold,
                "Budget invariant violated: warn_threshold ({}) should be <= threshold ({})",
                budget.warn_threshold, budget.threshold
            );

            // Serialize to JSON and back
            let json = serde_json::to_string(&budget)
                .expect("Budget should serialize to JSON");
            let deserialized: Budget = serde_json::from_str(&json)
                .expect("JSON should deserialize back to Budget");

            // Verify the invariant is preserved after round-trip
            prop_assert!(
                deserialized.warn_threshold <= deserialized.threshold,
                "Budget invariant violated after round-trip: warn_threshold ({}) should be <= threshold ({})",
                deserialized.warn_threshold, deserialized.threshold
            );
        }
    }
}
