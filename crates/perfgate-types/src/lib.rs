//! Shared types for perfgate.
//!
//! Design goal: versioned, explicit, boring.
//! These structs are used for receipts, PR comments, and (eventually) long-term baselines.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Examples
//!
//! Round-trip a [`ToolInfo`] through JSON:
//!
//! ```
//! use perfgate_types::ToolInfo;
//!
//! let tool = ToolInfo { name: "perfgate".into(), version: "1.0.0".into() };
//! let json = serde_json::to_string(&tool).unwrap();
//! let back: ToolInfo = serde_json::from_str(&json).unwrap();
//! assert_eq!(tool, back);
//! ```
//!
//! # Feature Flags
//!
//! - `arbitrary`: Enables `Arbitrary` derive for structure-aware fuzzing with cargo-fuzz.

mod defaults_config;
mod paired;

pub use paired::{
    NoiseDiagnostics, NoiseLevel, PAIRED_SCHEMA_V1, PairedBenchMeta, PairedDiffSummary,
    PairedRunReceipt, PairedSample, PairedSampleHalf, PairedStats,
};

pub use defaults_config::*;

pub use perfgate_validation::{
    BENCH_NAME_MAX_LEN, BENCH_NAME_PATTERN, ValidationError as BenchNameValidationError,
    validate_bench_name,
};

pub use perfgate_error::{ConfigValidationError, PerfgateError};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const RUN_SCHEMA_V1: &str = "perfgate.run.v1";
pub const BASELINE_SCHEMA_V1: &str = "perfgate.baseline.v1";
pub const COMPARE_SCHEMA_V1: &str = "perfgate.compare.v1";
pub const REPORT_SCHEMA_V1: &str = "perfgate.report.v1";
pub const CONFIG_SCHEMA_V1: &str = "perfgate.config.v1";
pub const RATCHET_SCHEMA_V1: &str = "perfgate.ratchet.v1";

// Stable contract identifiers and tokens.
pub const CHECK_ID_BUDGET: &str = "perf.budget";
pub const CHECK_ID_BASELINE: &str = "perf.baseline";
pub const CHECK_ID_HOST: &str = "perf.host";
pub const CHECK_ID_TOOL_RUNTIME: &str = "tool.runtime";
pub const FINDING_CODE_METRIC_WARN: &str = "metric_warn";
pub const FINDING_CODE_METRIC_FAIL: &str = "metric_fail";
pub const FINDING_CODE_BASELINE_MISSING: &str = "missing";
pub const FINDING_CODE_HOST_MISMATCH: &str = "host_mismatch";
pub const FINDING_CODE_RUNTIME_ERROR: &str = "runtime_error";
pub const VERDICT_REASON_NO_BASELINE: &str = "no_baseline";
pub const VERDICT_REASON_HOST_MISMATCH: &str = "host_mismatch";
pub const VERDICT_REASON_TOOL_ERROR: &str = "tool_error";
pub const VERDICT_REASON_TRUNCATED: &str = "truncated";

// Error classification stages.
pub const STAGE_CONFIG_PARSE: &str = "config_parse";
pub const STAGE_BASELINE_RESOLVE: &str = "baseline_resolve";
pub const STAGE_RUN_COMMAND: &str = "run_command";
pub const STAGE_WRITE_ARTIFACTS: &str = "write_artifacts";

// Error kind constants.
pub const ERROR_KIND_IO: &str = "io_error";
pub const ERROR_KIND_PARSE: &str = "parse_error";
pub const ERROR_KIND_EXEC: &str = "exec_error";

// Baseline reason tokens.
pub const BASELINE_REASON_NO_BASELINE: &str = "no_baseline";

// Truncation signaling constants.
pub const CHECK_ID_TOOL_TRUNCATION: &str = "tool.truncation";
pub const FINDING_CODE_TRUNCATED: &str = "truncated";
pub const MAX_FINDINGS_DEFAULT: usize = 100;

// Sensor report schema for cockpit integration
pub const SENSOR_REPORT_SCHEMA_V1: &str = "sensor.report.v1";

// ----------------------------
// Sensor report types (sensor.report.v1)
// ----------------------------

/// Capability status for "No Green By Omission" principle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum CapabilityStatus {
    Available,
    Unavailable,
    Skipped,
}

/// A capability with its status and optional reason.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Capability {
    pub status: CapabilityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Capabilities available to the sensor.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SensorCapabilities {
    pub baseline: Capability,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub engine: Option<Capability>,
}

/// Run metadata for the sensor report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SensorRunMeta {
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub capabilities: SensorCapabilities,
}

/// Verdict status for the sensor report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum SensorVerdictStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

/// Verdict counts for the sensor report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SensorVerdictCounts {
    pub info: u32,
    pub warn: u32,
    pub error: u32,
}

/// Verdict for the sensor report.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SensorVerdict {
    pub status: SensorVerdictStatus,
    pub counts: SensorVerdictCounts,
    pub reasons: Vec<String>,
}

/// Severity level for sensor findings (cockpit vocabulary).
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum SensorSeverity {
    Info,
    Warn,
    Error,
}

/// A finding from the sensor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SensorFinding {
    pub check_id: String,
    pub code: String,
    pub severity: SensorSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// An artifact produced by the sensor.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SensorArtifact {
    pub path: String,
    #[serde(rename = "type")]
    pub artifact_type: String,
}

/// The sensor.report.v1 envelope for cockpit integration.
///
/// This wraps PerfgateReport in a cockpit-compatible format with:
/// - Run metadata including capabilities
/// - Verdict using cockpit vocabulary (error instead of fail)
/// - Artifacts list
/// - Native perfgate data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SensorReport {
    pub schema: String,
    pub tool: ToolInfo,
    pub run: SensorRunMeta,
    pub verdict: SensorVerdict,
    pub findings: Vec<SensorFinding>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub artifacts: Vec<SensorArtifact>,
    pub data: serde_json::Value,
}

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

/// Policy for handling host mismatches when comparing receipts from different machines.
///
/// Host mismatches are detected when:
/// - Different `os` or `arch`
/// - Significant difference in `cpu_count` (> 2x)
/// - Significant difference in `memory_bytes` (> 2x)
/// - Different `hostname_hash` (if both present)
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum HostMismatchPolicy {
    /// Warn about host mismatch but continue with comparison (default).
    #[default]
    Warn,
    /// Treat host mismatch as an error (exit 1).
    Error,
    /// Ignore host mismatches completely (suppress warnings).
    Ignore,
}

impl HostMismatchPolicy {
    /// Returns the string representation of this policy.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::HostMismatchPolicy;
    ///
    /// assert_eq!(HostMismatchPolicy::Warn.as_str(), "warn");
    /// assert_eq!(HostMismatchPolicy::default().as_str(), "warn");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            HostMismatchPolicy::Warn => "warn",
            HostMismatchPolicy::Error => "error",
            HostMismatchPolicy::Ignore => "ignore",
        }
    }
}

/// Details about a detected host mismatch between baseline and current runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostMismatchInfo {
    /// Human-readable description of the mismatch.
    pub reasons: Vec<String>,
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

    /// CPU time (user + system) in milliseconds (Unix only).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cpu_ms: Option<u64>,

    /// Major page faults (Unix only).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub page_faults: Option<u64>,

    /// Voluntary + involuntary context switches (Unix only).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ctx_switches: Option<u64>,
    /// Peak resident set size in KB.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_rss_kb: Option<u64>,

    /// Bytes read from disk (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub io_read_bytes: Option<u64>,

    /// Bytes written to disk (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub io_write_bytes: Option<u64>,

    /// Total network packets (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub network_packets: Option<u64>,

    /// CPU energy used in microjoules (RAPL on Linux).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_uj: Option<u64>,

    /// Size of executed binary in bytes (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub binary_bytes: Option<u64>,

    /// Truncated stdout (bytes interpreted as UTF-8 lossily).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stdout: Option<String>,

    /// Truncated stderr (bytes interpreted as UTF-8 lossily).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct U64Summary {
    pub median: u64,
    pub min: u64,
    pub max: u64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stddev: Option<f64>,
}

impl U64Summary {
    pub fn new(median: u64, min: u64, max: u64) -> Self {
        Self {
            median,
            min,
            max,
            mean: None,
            stddev: None,
        }
    }

    /// Returns the coefficient of variation (stddev / mean).
    /// Returns None if mean is 0 or stddev/mean are not present.
    pub fn cv(&self) -> Option<f64> {
        match (self.mean, self.stddev) {
            (Some(mean), Some(stddev)) if mean > 0.0 => Some(stddev / mean),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct F64Summary {
    pub median: f64,
    pub min: f64,
    pub max: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub mean: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stddev: Option<f64>,
}

impl F64Summary {
    pub fn new(median: f64, min: f64, max: f64) -> Self {
        Self {
            median,
            min,
            max,
            mean: None,
            stddev: None,
        }
    }

    /// Returns the coefficient of variation (stddev / mean).
    /// Returns None if mean is 0 or stddev/mean are not present.
    pub fn cv(&self) -> Option<f64> {
        match (self.mean, self.stddev) {
            (Some(mean), Some(stddev)) if mean > 0.0 => Some(stddev / mean),
            _ => None,
        }
    }
}

/// Aggregated statistics for a benchmark run.
///
/// # Examples
///
/// ```
/// use perfgate_types::{Stats, U64Summary};
///
/// let stats = Stats {
///     wall_ms: U64Summary::new(100, 90, 120 ),
///     cpu_ms: None,
///     page_faults: None,
///     ctx_switches: None,
///     max_rss_kb: Some(U64Summary::new(4096, 4000, 4200 )),
///     io_read_bytes: None,
///     io_write_bytes: None,
///     network_packets: None,
///     energy_uj: None,
///     binary_bytes: None,
///     throughput_per_s: None,
/// };
/// assert_eq!(stats.wall_ms.median, 100);
/// assert_eq!(stats.max_rss_kb.unwrap().median, 4096);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Stats {
    pub wall_ms: U64Summary,

    /// CPU time (user + system) summary in milliseconds (Unix only).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cpu_ms: Option<U64Summary>,

    /// Major page faults summary (Unix only).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub page_faults: Option<U64Summary>,

    /// Voluntary + involuntary context switches summary (Unix only).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ctx_switches: Option<U64Summary>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub max_rss_kb: Option<U64Summary>,

    /// Bytes read from disk summary (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub io_read_bytes: Option<U64Summary>,

    /// Bytes written to disk summary (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub io_write_bytes: Option<U64Summary>,

    /// Total network packets summary (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub network_packets: Option<U64Summary>,

    /// CPU energy used summary in microjoules (RAPL on Linux).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub energy_uj: Option<U64Summary>,

    /// Size of executed binary in bytes (best-effort).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub binary_bytes: Option<U64Summary>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub throughput_per_s: Option<F64Summary>,
}

/// A versioned receipt from a single benchmark run (`perfgate.run.v1`).
///
/// # Examples
///
/// ```
/// use perfgate_types::*;
///
/// let receipt = RunReceipt {
///     schema: RUN_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     run: RunMeta {
///         id: "run-1".into(),
///         started_at: "2024-01-01T00:00:00Z".into(),
///         ended_at: "2024-01-01T00:00:01Z".into(),
///         host: HostInfo {
///             os: "linux".into(), arch: "x86_64".into(),
///             cpu_count: None, memory_bytes: None, hostname_hash: None,
///         },
///     },
///     bench: BenchMeta {
///         name: "my-bench".into(), cwd: None,
///         command: vec!["echo".into(), "hello".into()],
///         repeat: 3, warmup: 0, work_units: None, timeout_ms: None,
///     },
///     samples: vec![],
///     stats: Stats {
///         wall_ms: U64Summary::new(100, 90, 120 ),
///         cpu_ms: None, page_faults: None, ctx_switches: None,
///         max_rss_kb: None, io_read_bytes: None, io_write_bytes: None,
///         network_packets: None, energy_uj: None, binary_bytes: None, throughput_per_s: None,
///     },
/// };
///
/// // Serialize to JSON
/// let json = serde_json::to_string(&receipt).unwrap();
/// assert!(json.contains("perfgate.run.v1"));
/// ```
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
    BinaryBytes,
    CpuMs,
    CtxSwitches,
    EnergyUj,
    IoReadBytes,
    IoWriteBytes,
    MaxRssKb,
    NetworkPackets,
    PageFaults,
    ThroughputPerS,
    WallMs,
}

impl Metric {
    /// Returns the snake_case string key for this metric.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::Metric;
    ///
    /// assert_eq!(Metric::WallMs.as_str(), "wall_ms");
    /// assert_eq!(Metric::ThroughputPerS.as_str(), "throughput_per_s");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            Metric::BinaryBytes => "binary_bytes",
            Metric::CpuMs => "cpu_ms",
            Metric::CtxSwitches => "ctx_switches",
            Metric::EnergyUj => "energy_uj",
            Metric::IoReadBytes => "io_read_bytes",
            Metric::IoWriteBytes => "io_write_bytes",
            Metric::MaxRssKb => "max_rss_kb",
            Metric::NetworkPackets => "network_packets",
            Metric::PageFaults => "page_faults",
            Metric::ThroughputPerS => "throughput_per_s",
            Metric::WallMs => "wall_ms",
        }
    }

    /// Parses a snake_case string key into a [`Metric`], returning `None` for unknown keys.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::Metric;
    ///
    /// assert_eq!(Metric::parse_key("wall_ms"), Some(Metric::WallMs));
    /// assert_eq!(Metric::parse_key("max_rss_kb"), Some(Metric::MaxRssKb));
    /// assert_eq!(Metric::parse_key("unknown"), None);
    /// ```
    pub fn parse_key(key: &str) -> Option<Self> {
        match key {
            "binary_bytes" => Some(Metric::BinaryBytes),
            "cpu_ms" => Some(Metric::CpuMs),
            "ctx_switches" => Some(Metric::CtxSwitches),
            "energy_uj" => Some(Metric::EnergyUj),
            "io_read_bytes" => Some(Metric::IoReadBytes),
            "io_write_bytes" => Some(Metric::IoWriteBytes),
            "max_rss_kb" => Some(Metric::MaxRssKb),
            "network_packets" => Some(Metric::NetworkPackets),
            "page_faults" => Some(Metric::PageFaults),
            "throughput_per_s" => Some(Metric::ThroughputPerS),
            "wall_ms" => Some(Metric::WallMs),
            _ => None,
        }
    }

    /// Returns the default comparison direction for this metric.
    ///
    /// Most metrics use [`Direction::Lower`] (lower is better), except
    /// throughput which uses [`Direction::Higher`].
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::{Metric, Direction};
    ///
    /// assert_eq!(Metric::WallMs.default_direction(), Direction::Lower);
    /// assert_eq!(Metric::ThroughputPerS.default_direction(), Direction::Higher);
    /// ```
    pub fn default_direction(self) -> Direction {
        match self {
            Metric::BinaryBytes => Direction::Lower,
            Metric::CpuMs => Direction::Lower,
            Metric::CtxSwitches => Direction::Lower,
            Metric::EnergyUj => Direction::Lower,
            Metric::IoReadBytes => Direction::Lower,
            Metric::IoWriteBytes => Direction::Lower,
            Metric::MaxRssKb => Direction::Lower,
            Metric::NetworkPackets => Direction::Lower,
            Metric::PageFaults => Direction::Lower,
            Metric::ThroughputPerS => Direction::Higher,
            Metric::WallMs => Direction::Lower,
        }
    }

    pub fn default_warn_factor(self) -> f64 {
        // Near-budget warnings are useful in PRs, but they should not fail by default.
        0.9
    }

    /// Returns the human-readable display unit for this metric.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::Metric;
    ///
    /// assert_eq!(Metric::WallMs.display_unit(), "ms");
    /// assert_eq!(Metric::MaxRssKb.display_unit(), "KB");
    /// assert_eq!(Metric::ThroughputPerS.display_unit(), "/s");
    /// ```
    pub fn display_unit(self) -> &'static str {
        match self {
            Metric::BinaryBytes => "bytes",
            Metric::CpuMs => "ms",
            Metric::CtxSwitches => "count",
            Metric::EnergyUj => "uj",
            Metric::IoReadBytes => "bytes",
            Metric::IoWriteBytes => "bytes",
            Metric::MaxRssKb => "KB",
            Metric::NetworkPackets => "count",
            Metric::PageFaults => "count",
            Metric::ThroughputPerS => "/s",
            Metric::WallMs => "ms",
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum MetricStatistic {
    #[default]
    Median,
    P95,
}

impl MetricStatistic {
    /// Returns the string representation of this statistic.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::MetricStatistic;
    ///
    /// assert_eq!(MetricStatistic::Median.as_str(), "median");
    /// assert_eq!(MetricStatistic::P95.as_str(), "p95");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            MetricStatistic::Median => "median",
            MetricStatistic::P95 => "p95",
        }
    }
}

fn is_default_metric_statistic(stat: &MetricStatistic) -> bool {
    *stat == MetricStatistic::Median
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
    /// Noise threshold (coefficient of variation).
    /// If CV exceeds this, the metric is considered flaky/noisy.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub noise_threshold: Option<f64>,

    /// Policy for handling noisy metrics.
    #[serde(default, skip_serializing_if = "is_default_noise_policy")]
    pub noise_policy: NoisePolicy,

    /// Regression direction.
    pub direction: Direction,
}

fn is_default_noise_policy(policy: &NoisePolicy) -> bool {
    *policy == NoisePolicy::Ignore
}

impl Budget {
    /// Creates a new budget with defaults for noise_threshold.
    pub fn new(threshold: f64, warn_threshold: f64, direction: Direction) -> Self {
        Self {
            threshold,
            warn_threshold,
            noise_threshold: None,
            noise_policy: NoisePolicy::Ignore,
            direction,
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum SignificanceTest {
    WelchT,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Significance {
    pub test: SignificanceTest,
    pub p_value: Option<f64>,
    pub alpha: f64,
    pub significant: bool,
    pub baseline_samples: u32,
    pub current_samples: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_lower: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_upper: Option<f64>,
}

/// Policy for statistical significance testing.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct SignificancePolicy {
    /// Significance level (e.g. 0.05).
    pub alpha: Option<f64>,
    /// Minimum number of samples required for a significant result.
    pub min_samples: Option<u32>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum NoisePolicy {
    /// No change to status based on noise.
    #[default]
    Ignore,
    /// Escalate Pass to Warn, and demote Fail to Warn.
    Warn,
    /// Demote Pass and Fail to Skip.
    Skip,
}

impl NoisePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            NoisePolicy::Ignore => "ignore",
            NoisePolicy::Warn => "warn",
            NoisePolicy::Skip => "skip",
        }
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum MetricStatus {
    Pass,
    Warn,
    Fail,
    Skip,
}

impl MetricStatus {
    /// Returns the string representation of this status.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::MetricStatus;
    ///
    /// assert_eq!(MetricStatus::Pass.as_str(), "pass");
    /// assert_eq!(MetricStatus::Warn.as_str(), "warn");
    /// assert_eq!(MetricStatus::Fail.as_str(), "fail");
    /// ```
    pub fn as_str(self) -> &'static str {
        match self {
            MetricStatus::Pass => "pass",
            MetricStatus::Warn => "warn",
            MetricStatus::Fail => "fail",
            MetricStatus::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
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

    /// Coefficient of variation for the current run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cv: Option<f64>,

    /// Noise threshold used for this comparison.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_threshold: Option<f64>,

    #[serde(default, skip_serializing_if = "is_default_metric_statistic")]
    pub statistic: MetricStatistic,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub significance: Option<Significance>,

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
    Skip,
}

impl VerdictStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            VerdictStatus::Pass => "pass",
            VerdictStatus::Warn => "warn",
            VerdictStatus::Fail => "fail",
            VerdictStatus::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct VerdictCounts {
    pub pass: u32,
    pub warn: u32,
    pub fail: u32,
    pub skip: u32,
}

/// Overall verdict for a comparison, with pass/warn/fail counts.
///
/// # Examples
///
/// ```
/// use perfgate_types::{Verdict, VerdictStatus, VerdictCounts};
///
/// let verdict = Verdict {
///     status: VerdictStatus::Pass,
///     counts: VerdictCounts { pass: 2, warn: 0, fail: 0, skip: 0 },
///     reasons: vec![],
/// };
/// assert_eq!(verdict.status, VerdictStatus::Pass);
///
/// let failing = Verdict {
///     status: VerdictStatus::Fail,
///     counts: VerdictCounts { pass: 1, warn: 0, fail: 1, skip: 0 },
///     reasons: vec!["wall_ms.fail".into()],
/// };
/// assert_eq!(failing.status, VerdictStatus::Fail);
/// assert!(!failing.reasons.is_empty());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Verdict {
    pub status: VerdictStatus,
    pub counts: VerdictCounts,
    pub reasons: Vec<String>,
}

/// A versioned receipt comparing baseline vs current (`perfgate.compare.v1`).
///
/// # Examples
///
/// ```
/// use perfgate_types::*;
/// use std::collections::BTreeMap;
///
/// let receipt = CompareReceipt {
///     schema: COMPARE_SCHEMA_V1.to_string(),
///     tool: ToolInfo { name: "perfgate".into(), version: "0.1.0".into() },
///     bench: BenchMeta {
///         name: "my-bench".into(), cwd: None,
///         command: vec!["echo".into()], repeat: 5, warmup: 0,
///         work_units: None, timeout_ms: None,
///     },
///     baseline_ref: CompareRef { path: Some("base.json".into()), run_id: None },
///     current_ref: CompareRef { path: Some("cur.json".into()), run_id: None },
///     budgets: BTreeMap::new(),
///     deltas: BTreeMap::new(),
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts { pass: 0, warn: 0, fail: 0, skip: 0 },
///         reasons: vec![],
///     },
/// };
/// assert_eq!(receipt.schema, "perfgate.compare.v1");
/// ```
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

    /// Number of metrics that were skipped.
    #[serde(rename = "skip_count", default, skip_serializing_if = "is_zero_u32")]
    pub skip_count: u32,

    /// Total number of metrics checked.
    #[serde(rename = "total_count")]
    pub total_count: u32,
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
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

    /// Path to a flamegraph SVG captured when regression was detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_path: Option<String>,
}

// ----------------------------
// Optional config file schema
// ----------------------------

/// Top-level configuration file (`perfgate.toml`).
///
/// # Examples
///
/// ```
/// use perfgate_types::ConfigFile;
///
/// let toml_str = r#"
/// [defaults]
/// repeat = 5
/// threshold = 0.20
///
/// [[bench]]
/// name = "my-bench"
/// command = ["echo", "hello"]
/// "#;
///
/// let config: ConfigFile = toml::from_str(toml_str).unwrap();
/// assert_eq!(config.benches.len(), 1);
/// assert_eq!(config.benches[0].name, "my-bench");
/// assert_eq!(config.defaults.repeat, Some(5));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ConfigFile {
    #[serde(default)]
    pub defaults: DefaultsConfig,

    /// Optional baseline server configuration for centralized baseline management.
    #[serde(default)]
    pub baseline_server: BaselineServerConfig,

    /// Optional automated budget ratcheting policy.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub ratchet: Option<RatchetConfig>,

    #[serde(default, rename = "bench")]
    pub benches: Vec<BenchConfigFile>,
}

impl ConfigFile {
    /// Validate all bench names in this config. Returns an error if any name is invalid.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_types::ConfigFile;
    ///
    /// let good: ConfigFile = toml::from_str(r#"
    /// [[bench]]
    /// name = "valid-bench"
    /// command = ["echo"]
    /// "#).unwrap();
    /// assert!(good.validate().is_ok());
    ///
    /// let bad: ConfigFile = toml::from_str(r#"
    /// [[bench]]
    /// name = "INVALID|name"
    /// command = ["echo"]
    /// "#).unwrap();
    /// assert!(bad.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<(), String> {
        for bench in &self.benches {
            validate_bench_name(&bench.name).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// Configuration for the baseline server connection.
///
/// When configured, the CLI can use a centralized baseline server
/// for storing and retrieving baselines instead of local files.
///
/// # Examples
///
/// ```toml
/// [baseline_server]
/// url = "http://localhost:3000/api/v1"
/// api_key = "pg_live_xxx"
/// project = "my-project"
/// fallback_to_local = true
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct BaselineServerConfig {
    /// URL of the baseline server (e.g., "http://localhost:3000/api/v1").
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub url: Option<String>,

    /// API key for authentication.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub api_key: Option<String>,

    /// Project name for multi-tenancy.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub project: Option<String>,

    /// Fall back to local storage when server is unavailable.
    #[serde(default = "default_fallback_to_local")]
    pub fallback_to_local: bool,
}

fn default_fallback_to_local() -> bool {
    true
}

impl BaselineServerConfig {
    /// Returns true if server is configured (has a URL).
    pub fn is_configured(&self) -> bool {
        self.url.is_some() && !self.url.as_ref().unwrap().is_empty()
    }

    /// Resolves the server URL from config or environment variable.
    pub fn resolved_url(&self) -> Option<String> {
        std::env::var("PERFGATE_SERVER_URL")
            .ok()
            .or_else(|| self.url.clone())
    }

    /// Resolves the API key from config or environment variable.
    pub fn resolved_api_key(&self) -> Option<String> {
        std::env::var("PERFGATE_API_KEY")
            .ok()
            .or_else(|| self.api_key.clone())
    }

    /// Resolves the project name from config or environment variable.
    pub fn resolved_project(&self) -> Option<String> {
        std::env::var("PERFGATE_PROJECT")
            .ok()
            .or_else(|| self.project.clone())
    }
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

    /// Optional scaling validation configuration.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scaling: Option<ScalingConfig>,
}

/// How ratcheting should update budgets.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "snake_case")]
pub enum RatchetMode {
    /// Tighten the configured threshold value.
    #[default]
    Threshold,
    /// Reserved for future baseline-value ratcheting mode.
    BaselineValue,
}

/// Configuration for conservative automated budget ratcheting.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RatchetConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: RatchetMode,
    #[serde(default = "default_ratchet_min_improvement")]
    pub min_improvement: f64,
    #[serde(default = "default_ratchet_max_tightening")]
    pub max_tightening: f64,
    #[serde(default = "default_ratchet_require_significance")]
    pub require_significance: bool,
    #[serde(default = "default_ratchet_allow_metrics")]
    pub allow_metrics: Vec<Metric>,
}

impl Default for RatchetConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: RatchetMode::Threshold,
            min_improvement: default_ratchet_min_improvement(),
            max_tightening: default_ratchet_max_tightening(),
            require_significance: default_ratchet_require_significance(),
            allow_metrics: default_ratchet_allow_metrics(),
        }
    }
}

fn default_ratchet_min_improvement() -> f64 {
    0.05
}

fn default_ratchet_max_tightening() -> f64 {
    0.10
}

fn default_ratchet_require_significance() -> bool {
    true
}

fn default_ratchet_allow_metrics() -> Vec<Metric> {
    vec![Metric::WallMs, Metric::CpuMs]
}

/// Per-metric ratchet change.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RatchetChange {
    pub metric: Metric,
    pub field: String,
    pub old_value: f64,
    pub new_value: f64,
    pub reason: String,
}

/// Machine-readable ratchet artifact.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct RatchetReceipt {
    pub schema: String,
    pub tool: ToolInfo,
    pub bench_name: String,
    pub compare_path: Option<String>,
    pub changes: Vec<RatchetChange>,
}

/// Configuration for computational complexity validation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct ScalingConfig {
    /// Input sizes to test.
    pub sizes: Vec<u64>,

    /// Expected complexity class (e.g., "O(n)", "O(n^2)").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,

    /// Number of repetitions per input size (default: 5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u32>,

    /// Minimum R-squared for a valid fit (default: 0.90).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r_squared_threshold: Option<f64>,
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

    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_threshold: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub noise_policy: Option<NoisePolicy>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub statistic: Option<MetricStatistic>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_serde_keys_are_snake_case() {
        let mut m = BTreeMap::new();
        m.insert(Metric::WallMs, Budget::new(0.2, 0.18, Direction::Lower));
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("\"wall_ms\""));
    }

    #[test]
    fn metric_metadata_and_parsing_are_consistent() {
        let cases = [
            (
                Metric::BinaryBytes,
                "binary_bytes",
                Direction::Lower,
                "bytes",
            ),
            (Metric::WallMs, "wall_ms", Direction::Lower, "ms"),
            (Metric::CpuMs, "cpu_ms", Direction::Lower, "ms"),
            (
                Metric::CtxSwitches,
                "ctx_switches",
                Direction::Lower,
                "count",
            ),
            (Metric::MaxRssKb, "max_rss_kb", Direction::Lower, "KB"),
            (Metric::PageFaults, "page_faults", Direction::Lower, "count"),
            (
                Metric::ThroughputPerS,
                "throughput_per_s",
                Direction::Higher,
                "/s",
            ),
        ];

        for (metric, key, direction, unit) in cases {
            assert_eq!(metric.as_str(), key);
            assert_eq!(Metric::parse_key(key), Some(metric));
            assert_eq!(metric.default_direction(), direction);
            assert_eq!(metric.display_unit(), unit);
            assert!((metric.default_warn_factor() - 0.9).abs() < f64::EPSILON);
        }

        assert!(Metric::parse_key("unknown").is_none());

        assert_eq!(MetricStatistic::Median.as_str(), "median");
        assert_eq!(MetricStatistic::P95.as_str(), "p95");
        assert!(is_default_metric_statistic(&MetricStatistic::Median));
        assert!(!is_default_metric_statistic(&MetricStatistic::P95));
    }

    #[test]
    fn status_and_policy_as_str_values() {
        assert_eq!(MetricStatus::Pass.as_str(), "pass");
        assert_eq!(MetricStatus::Warn.as_str(), "warn");
        assert_eq!(MetricStatus::Fail.as_str(), "fail");

        assert_eq!(HostMismatchPolicy::Warn.as_str(), "warn");
        assert_eq!(HostMismatchPolicy::Error.as_str(), "error");
        assert_eq!(HostMismatchPolicy::Ignore.as_str(), "ignore");
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

    #[test]
    fn host_info_minimal_json_snapshot() {
        let info = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };

        let value = serde_json::to_value(&info).expect("serialize HostInfo");
        insta::assert_json_snapshot!(value, @r###"
        {
          "arch": "x86_64",
          "os": "linux"
        }
        "###);
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

    #[test]
    fn validate_bench_name_valid() {
        assert!(validate_bench_name("my-bench").is_ok());
        assert!(validate_bench_name("bench_a").is_ok());
        assert!(validate_bench_name("path/to/bench").is_ok());
        assert!(validate_bench_name("bench.v2").is_ok());
        assert!(validate_bench_name("a").is_ok());
        assert!(validate_bench_name("123").is_ok());
    }

    #[test]
    fn validate_bench_name_invalid() {
        assert!(validate_bench_name("bench|name").is_err());
        assert!(validate_bench_name("").is_err());
        assert!(validate_bench_name("bench name").is_err());
        assert!(validate_bench_name("bench@name").is_err());
    }

    #[test]
    fn validate_bench_name_path_traversal() {
        assert!(validate_bench_name("../bench").is_err());
        assert!(validate_bench_name("bench/../x").is_err());
        assert!(validate_bench_name("./bench").is_err());
        assert!(validate_bench_name("bench/.").is_err());
    }

    #[test]
    fn validate_bench_name_empty_segments() {
        assert!(validate_bench_name("/bench").is_err());
        assert!(validate_bench_name("bench/").is_err());
        assert!(validate_bench_name("bench//x").is_err());
        assert!(validate_bench_name("/").is_err());
    }

    #[test]
    fn validate_bench_name_length_cap() {
        // Exactly 64 chars -> ok
        let name_64 = "a".repeat(BENCH_NAME_MAX_LEN);
        assert!(validate_bench_name(&name_64).is_ok());

        // 65 chars -> rejected
        let name_65 = "a".repeat(BENCH_NAME_MAX_LEN + 1);
        assert!(validate_bench_name(&name_65).is_err());
    }

    #[test]
    fn validate_bench_name_case() {
        assert!(validate_bench_name("MyBench").is_err());
        assert!(validate_bench_name("BENCH").is_err());
        assert!(validate_bench_name("benchA").is_err());
    }

    #[test]
    fn config_file_validate_catches_bad_bench_name() {
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            ratchet: None,
            benches: vec![BenchConfigFile {
                name: "bad|name".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,

                scaling: None,
            }],
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn perfgate_error_display_baseline_resolve() {
        use perfgate_error::IoError;
        let err = PerfgateError::Io(IoError::BaselineResolve("file not found".to_string()));
        assert_eq!(format!("{}", err), "baseline resolve: file not found");
    }

    #[test]
    fn perfgate_error_display_artifact_write() {
        use perfgate_error::IoError;
        let err = PerfgateError::Io(IoError::ArtifactWrite("permission denied".to_string()));
        assert_eq!(format!("{}", err), "write artifacts: permission denied");
    }

    #[test]
    fn perfgate_error_display_run_command() {
        use perfgate_error::IoError;
        let err = PerfgateError::Io(IoError::RunCommand {
            command: "echo".to_string(),
            reason: "spawn failed".to_string(),
        });
        assert_eq!(
            format!("{}", err),
            "failed to execute command \"echo\": spawn failed"
        );
    }

    #[test]
    fn sensor_capabilities_backward_compat_without_engine() {
        let json = r#"{"baseline":{"status":"available"}}"#;
        let caps: SensorCapabilities =
            serde_json::from_str(json).expect("should parse without engine");
        assert_eq!(caps.baseline.status, CapabilityStatus::Available);
        assert!(caps.engine.is_none());
    }

    #[test]
    fn sensor_capabilities_with_engine() {
        let caps = SensorCapabilities {
            baseline: Capability {
                status: CapabilityStatus::Available,
                reason: None,
            },
            engine: Some(Capability {
                status: CapabilityStatus::Available,
                reason: None,
            }),
        };
        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("\"engine\""));
        let parsed: SensorCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, parsed);
    }

    #[test]
    fn sensor_capabilities_engine_omitted_when_none() {
        let caps = SensorCapabilities {
            baseline: Capability {
                status: CapabilityStatus::Available,
                reason: None,
            },
            engine: None,
        };
        let json = serde_json::to_string(&caps).unwrap();
        assert!(!json.contains("engine"));
    }

    #[test]
    fn config_file_validate_passes_good_bench_names() {
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            ratchet: None,
            benches: vec![BenchConfigFile {
                name: "my-bench".to_string(),
                cwd: None,
                work: None,
                timeout: None,
                command: vec!["echo".to_string()],
                repeat: None,
                warmup: None,
                metrics: None,
                budgets: None,

                scaling: None,
            }],
        };
        assert!(config.validate().is_ok());
    }

    // ---- Serde round-trip unit tests ----

    #[test]
    fn run_receipt_serde_roundtrip_typical() {
        let receipt = RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "1.2.3".into(),
            },
            run: RunMeta {
                id: "abc-123".into(),
                started_at: "2024-06-15T10:00:00Z".into(),
                ended_at: "2024-06-15T10:00:05Z".into(),
                host: HostInfo {
                    os: "linux".into(),
                    arch: "x86_64".into(),
                    cpu_count: Some(8),
                    memory_bytes: Some(16_000_000_000),
                    hostname_hash: Some("cafebabe".into()),
                },
            },
            bench: BenchMeta {
                name: "my-bench".into(),
                cwd: Some("/tmp".into()),
                command: vec!["echo".into(), "hello".into()],
                repeat: 5,
                warmup: 1,
                work_units: Some(1000),
                timeout_ms: Some(30000),
            },
            samples: vec![
                Sample {
                    wall_ms: 100,
                    exit_code: 0,
                    warmup: true,
                    timed_out: false,
                    cpu_ms: Some(80),
                    page_faults: Some(10),
                    ctx_switches: Some(5),
                    max_rss_kb: Some(2048),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: Some(4096),
                    stdout: Some("ok".into()),
                    stderr: None,
                },
                Sample {
                    wall_ms: 95,
                    exit_code: 0,
                    warmup: false,
                    timed_out: false,
                    cpu_ms: Some(75),
                    page_faults: None,
                    ctx_switches: None,
                    max_rss_kb: Some(2000),
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj: None,
                    binary_bytes: None,
                    stdout: None,
                    stderr: Some("warn".into()),
                },
            ],
            stats: Stats {
                wall_ms: U64Summary::new(95, 90, 100),
                cpu_ms: Some(U64Summary::new(75, 70, 80)),
                page_faults: Some(U64Summary::new(10, 10, 10)),
                ctx_switches: Some(U64Summary::new(5, 5, 5)),
                max_rss_kb: Some(U64Summary::new(2048, 2000, 2100)),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: Some(U64Summary::new(4096, 4096, 4096)),
                throughput_per_s: Some(F64Summary::new(10.526, 10.0, 11.111)),
            },
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let back: RunReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, back);
    }

    #[test]
    fn run_receipt_serde_roundtrip_edge_empty_samples() {
        let receipt = RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "p".into(),
                version: "0".into(),
            },
            run: RunMeta {
                id: "".into(),
                started_at: "".into(),
                ended_at: "".into(),
                host: HostInfo {
                    os: "".into(),
                    arch: "".into(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "b".into(),
                cwd: None,
                command: vec![],
                repeat: 0,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![],
            stats: Stats {
                wall_ms: U64Summary::new(0, 0, 0),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let back: RunReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, back);
    }

    #[test]
    fn run_receipt_serde_roundtrip_edge_large_values() {
        let receipt = RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "99.99.99".into(),
            },
            run: RunMeta {
                id: "max-run".into(),
                started_at: "2099-12-31T23:59:59Z".into(),
                ended_at: "2099-12-31T23:59:59Z".into(),
                host: HostInfo {
                    os: "linux".into(),
                    arch: "aarch64".into(),
                    cpu_count: Some(u32::MAX),
                    memory_bytes: Some(u64::MAX),
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "big".into(),
                cwd: None,
                command: vec!["run".into()],
                repeat: u32::MAX,
                warmup: u32::MAX,
                work_units: Some(u64::MAX),
                timeout_ms: Some(u64::MAX),
            },
            samples: vec![Sample {
                wall_ms: u64::MAX,
                exit_code: i32::MIN,
                warmup: false,
                timed_out: true,
                cpu_ms: Some(u64::MAX),
                page_faults: Some(u64::MAX),
                ctx_switches: Some(u64::MAX),
                max_rss_kb: Some(u64::MAX),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: Some(u64::MAX),
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary::new(u64::MAX, 0, u64::MAX),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: Some(F64Summary::new(f64::MAX, 0.0, f64::MAX)),
            },
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let back: RunReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, back);
    }

    #[test]
    fn compare_receipt_serde_roundtrip_typical() {
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::WallMs, Budget::new(0.2, 0.18, Direction::Lower));
        budgets.insert(Metric::MaxRssKb, Budget::new(0.15, 0.1, Direction::Lower));

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 1000.0,
                current: 1100.0,
                ratio: 1.1,
                pct: 0.1,
                regression: 0.1,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 2048.0,
                current: 2500.0,
                ratio: 1.2207,
                pct: 0.2207,
                regression: 0.2207,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Fail,
            },
        );

        let receipt = CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "1.0.0".into(),
            },
            bench: BenchMeta {
                name: "test".into(),
                cwd: None,
                command: vec!["echo".into()],
                repeat: 5,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("base.json".into()),
                run_id: Some("r1".into()),
            },
            current_ref: CompareRef {
                path: Some("cur.json".into()),
                run_id: Some("r2".into()),
            },
            budgets,
            deltas,
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 1,
                    skip: 0,
                },
                reasons: vec!["max_rss_kb_fail".into()],
            },
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let back: CompareReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, back);
    }

    #[test]
    fn compare_receipt_serde_roundtrip_edge_empty_maps() {
        let receipt = CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "p".into(),
                version: "0".into(),
            },
            bench: BenchMeta {
                name: "b".into(),
                cwd: None,
                command: vec![],
                repeat: 0,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            budgets: BTreeMap::new(),
            deltas: BTreeMap::new(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
        };
        let json = serde_json::to_string(&receipt).unwrap();
        let back: CompareReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, back);
    }

    #[test]
    fn report_receipt_serde_roundtrip() {
        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 1,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec!["wall_ms_warn".into()],
            },
            compare: None,
            findings: vec![ReportFinding {
                check_id: CHECK_ID_BUDGET.into(),
                code: FINDING_CODE_METRIC_WARN.into(),
                severity: Severity::Warn,
                message: "Performance regression near threshold for wall_ms".into(),
                data: Some(FindingData {
                    metric_name: "wall_ms".into(),
                    baseline: 100.0,
                    current: 119.0,
                    regression_pct: 0.19,
                    threshold: 0.2,
                    direction: Direction::Lower,
                }),
            }],
            summary: ReportSummary {
                pass_count: 1,
                warn_count: 1,
                fail_count: 0,
                skip_count: 0,
                total_count: 2,
            },
            profile_path: None,
        };
        let json = serde_json::to_string(&report).unwrap();
        let back: PerfgateReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn config_file_serde_roundtrip_typical() {
        let config = ConfigFile {
            defaults: DefaultsConfig {
                noise_threshold: None,
                noise_policy: None,
                repeat: Some(10),
                warmup: Some(2),
                threshold: Some(0.2),
                warn_factor: Some(0.9),
                out_dir: Some("artifacts/perfgate".into()),
                baseline_dir: Some("baselines".into()),
                baseline_pattern: Some("baselines/{bench}.json".into()),
                markdown_template: None,
            },
            baseline_server: BaselineServerConfig::default(),
            ratchet: None,
            benches: vec![BenchConfigFile {
                name: "my-bench".into(),
                cwd: Some("/home/user/project".into()),
                work: Some(1000),
                timeout: Some("5s".into()),
                command: vec!["cargo".into(), "bench".into()],
                repeat: Some(20),
                warmup: Some(3),
                metrics: Some(vec![Metric::WallMs, Metric::MaxRssKb]),
                budgets: Some({
                    let mut m = BTreeMap::new();
                    m.insert(
                        Metric::WallMs,
                        BudgetOverride {
                            noise_threshold: None,
                            noise_policy: None,
                            threshold: Some(0.15),
                            direction: Some(Direction::Lower),
                            warn_factor: Some(0.85),
                            statistic: Some(MetricStatistic::P95),
                        },
                    );
                    m
                }),
                scaling: None,
            }],
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: ConfigFile = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn config_file_serde_roundtrip_edge_empty() {
        let config = ConfigFile {
            defaults: DefaultsConfig::default(),
            baseline_server: BaselineServerConfig::default(),
            ratchet: None,
            benches: vec![],
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: ConfigFile = serde_json::from_str(&json).unwrap();
        assert_eq!(config, back);
    }

    #[test]
    fn stats_serde_roundtrip_all_fields() {
        let stats = Stats {
            wall_ms: U64Summary::new(500, 100, 900),
            cpu_ms: Some(U64Summary::new(400, 80, 800)),
            page_faults: Some(U64Summary::new(50, 10, 100)),
            ctx_switches: Some(U64Summary::new(20, 5, 40)),
            max_rss_kb: Some(U64Summary::new(4096, 2048, 8192)),
            io_read_bytes: Some(U64Summary::new(1000, 500, 1500)),
            io_write_bytes: Some(U64Summary::new(500, 200, 800)),
            network_packets: Some(U64Summary::new(10, 5, 15)),
            energy_uj: None,
            binary_bytes: Some(U64Summary::new(1024, 1024, 1024)),
            throughput_per_s: Some(F64Summary::new(2.0, 1.111, 10.0)),
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: Stats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats, back);
    }

    #[test]
    fn stats_serde_roundtrip_edge_zeros() {
        let stats = Stats {
            wall_ms: U64Summary::new(0, 0, 0),
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: Some(F64Summary::new(0.0, 0.0, 0.0)),
        };
        let json = serde_json::to_string(&stats).unwrap();
        let back: Stats = serde_json::from_str(&json).unwrap();
        assert_eq!(stats, back);
    }

    #[test]
    fn backward_compat_run_receipt_missing_host_extensions() {
        // Old format: host has only os/arch, no cpu_count/memory_bytes/hostname_hash
        let json = r#"{
            "schema": "perfgate.run.v1",
            "tool": {"name": "perfgate", "version": "0.0.1"},
            "run": {
                "id": "old-run",
                "started_at": "2023-06-01T00:00:00Z",
                "ended_at": "2023-06-01T00:01:00Z",
                "host": {"os": "macos", "arch": "aarch64"}
            },
            "bench": {
                "name": "legacy",
                "command": ["./bench"],
                "repeat": 1,
                "warmup": 0
            },
            "samples": [{"wall_ms": 50, "exit_code": 0}],
            "stats": {
                "wall_ms": {"median": 50, "min": 50, "max": 50}
            }
        }"#;

        let receipt: RunReceipt =
            serde_json::from_str(json).expect("old format without host extensions");
        assert_eq!(receipt.run.host.os, "macos");
        assert_eq!(receipt.run.host.arch, "aarch64");
        assert!(receipt.run.host.cpu_count.is_none());
        assert!(receipt.run.host.memory_bytes.is_none());
        assert!(receipt.run.host.hostname_hash.is_none());
        assert_eq!(receipt.bench.name, "legacy");
        assert_eq!(receipt.samples.len(), 1);
        assert!(!receipt.samples[0].warmup);
        assert!(!receipt.samples[0].timed_out);
    }

    #[test]
    fn backward_compat_compare_receipt_without_significance() {
        let json = r#"{
            "schema": "perfgate.compare.v1",
            "tool": {"name": "perfgate", "version": "0.0.1"},
            "bench": {
                "name": "old-cmp",
                "command": ["echo"],
                "repeat": 3,
                "warmup": 0
            },
            "baseline_ref": {"path": "base.json"},
            "current_ref": {"path": "cur.json"},
            "budgets": {
                "wall_ms": {"threshold": 0.2, "warn_threshold": 0.1, "direction": "lower"}
            },
            "deltas": {
                "wall_ms": {
                    "baseline": 100.0,
                    "current": 105.0,
                    "ratio": 1.05,
                    "pct": 0.05,
                    "regression": 0.05,
                    "status": "pass"
                }
            },
            "verdict": {
                "status": "pass",
                "counts": {"pass": 1, "warn": 0, "fail": 0, "skip": 0},
                "reasons": []
            }
        }"#;

        let receipt: CompareReceipt =
            serde_json::from_str(json).expect("compare without significance");
        assert_eq!(receipt.deltas.len(), 1);
        let delta = receipt.deltas.get(&Metric::WallMs).unwrap();
        assert!(delta.significance.is_none());
        assert_eq!(delta.statistic, MetricStatistic::Median); // default
        assert_eq!(delta.status, MetricStatus::Pass);
    }

    #[test]
    fn backward_compat_unknown_fields_are_ignored() {
        let json = r#"{
            "schema": "perfgate.run.v1",
            "tool": {"name": "perfgate", "version": "0.1.0"},
            "run": {
                "id": "test",
                "started_at": "2024-01-01T00:00:00Z",
                "ended_at": "2024-01-01T00:01:00Z",
                "host": {"os": "linux", "arch": "x86_64", "future_field": "ignored"}
            },
            "bench": {
                "name": "test",
                "command": ["echo"],
                "repeat": 1,
                "warmup": 0,
                "some_new_option": true
            },
            "samples": [{"wall_ms": 10, "exit_code": 0, "extra_metric": 42}],
            "stats": {
                "wall_ms": {"median": 10, "min": 10, "max": 10},
                "new_metric": {"median": 1, "min": 1, "max": 1}
            },
            "new_top_level_field": "should be ignored"
        }"#;

        let receipt: RunReceipt =
            serde_json::from_str(json).expect("unknown fields should be ignored");
        assert_eq!(receipt.bench.name, "test");
        assert_eq!(receipt.samples.len(), 1);
    }

    #[test]
    fn roundtrip_run_receipt_all_optionals_none() {
        let receipt = RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            run: RunMeta {
                id: "rt".into(),
                started_at: "2024-01-01T00:00:00Z".into(),
                ended_at: "2024-01-01T00:01:00Z".into(),
                host: HostInfo {
                    os: "linux".into(),
                    arch: "x86_64".into(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "minimal".into(),
                cwd: None,
                command: vec!["true".into()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![Sample {
                wall_ms: 1,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            }],
            stats: Stats {
                wall_ms: U64Summary::new(1, 1, 1),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        };

        let json = serde_json::to_string(&receipt).unwrap();
        let back: RunReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, back);

        // Verify optional fields are omitted from JSON
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let host = &value["run"]["host"];
        assert!(host.get("cpu_count").is_none());
        assert!(host.get("memory_bytes").is_none());
        assert!(host.get("hostname_hash").is_none());
    }

    #[test]
    fn roundtrip_compare_receipt_all_optionals_none() {
        let receipt = CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".into(),
                version: "0.1.0".into(),
            },
            bench: BenchMeta {
                name: "minimal".into(),
                cwd: None,
                command: vec!["true".into()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: None,
                run_id: None,
            },
            current_ref: CompareRef {
                path: None,
                run_id: None,
            },
            budgets: BTreeMap::new(),
            deltas: BTreeMap::new(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
        };

        let json = serde_json::to_string(&receipt).unwrap();
        let back: CompareReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, back);
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
            proptest::option::of(0u64..1000000),   // cpu_ms
            proptest::option::of(0u64..1000000),   // page_faults
            proptest::option::of(0u64..1000000),   // ctx_switches
            proptest::option::of(0u64..1000000),   // max_rss_kb
            proptest::option::of(0u64..1000000),   // energy_uj
            proptest::option::of(0u64..100000000), // binary_bytes
            proptest::option::of("[a-zA-Z0-9 ]{0,50}"),
            proptest::option::of("[a-zA-Z0-9 ]{0,50}"),
        )
            .prop_map(
                |(
                    wall_ms,
                    exit_code,
                    warmup,
                    timed_out,
                    cpu_ms,
                    page_faults,
                    ctx_switches,
                    max_rss_kb,
                    energy_uj,
                    binary_bytes,
                    stdout,
                    stderr,
                )| Sample {
                    wall_ms,
                    exit_code,
                    warmup,
                    timed_out,
                    cpu_ms,
                    page_faults,
                    ctx_switches,
                    max_rss_kb,
                    io_read_bytes: None,
                    io_write_bytes: None,
                    network_packets: None,
                    energy_uj,
                    binary_bytes,
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
            U64Summary::new(vals[1], vals[0], vals[2])
        })
    }

    // Strategy for F64Summary - using finite positive floats
    fn f64_summary_strategy() -> impl Strategy<Value = F64Summary> {
        (0.0f64..1000000.0, 0.0f64..1000000.0, 0.0f64..1000000.0).prop_map(|(a, b, c)| {
            let mut vals = [a, b, c];
            vals.sort_by(|x, y| x.partial_cmp(y).unwrap());
            F64Summary::new(vals[1], vals[0], vals[2])
        })
    }

    // Strategy for Stats
    fn stats_strategy() -> impl Strategy<Value = Stats> {
        (
            u64_summary_strategy(),
            proptest::option::of(u64_summary_strategy()), // cpu_ms
            proptest::option::of(u64_summary_strategy()), // page_faults
            proptest::option::of(u64_summary_strategy()), // ctx_switches
            proptest::option::of(u64_summary_strategy()), // max_rss_kb
            proptest::option::of(u64_summary_strategy()), // io_read_bytes
            proptest::option::of(u64_summary_strategy()), // io_write_bytes
            proptest::option::of(u64_summary_strategy()), // network_packets
            proptest::option::of(u64_summary_strategy()), // energy_uj
            proptest::option::of(u64_summary_strategy()), // binary_bytes
            proptest::option::of(f64_summary_strategy()),
        )
            .prop_map(
                |(
                    wall_ms,
                    cpu_ms,
                    page_faults,
                    ctx_switches,
                    max_rss_kb,
                    io_read_bytes,
                    io_write_bytes,
                    network_packets,
                    energy_uj,
                    binary_bytes,
                    throughput_per_s,
                )| Stats {
                    wall_ms,
                    cpu_ms,
                    page_faults,
                    ctx_switches,
                    max_rss_kb,
                    io_read_bytes,
                    io_write_bytes,
                    network_packets,
                    energy_uj,
                    binary_bytes,
                    throughput_per_s,
                },
            )
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
                prop_assert_eq!(orig.cpu_ms, deser.cpu_ms);
                prop_assert_eq!(orig.page_faults, deser.page_faults);
                prop_assert_eq!(orig.ctx_switches, deser.ctx_switches);
                prop_assert_eq!(orig.max_rss_kb, deser.max_rss_kb);
                prop_assert_eq!(orig.binary_bytes, deser.binary_bytes);
                prop_assert_eq!(&orig.stdout, &deser.stdout);
                prop_assert_eq!(&orig.stderr, &deser.stderr);
            }

            // Compare stats
            prop_assert_eq!(&receipt.stats.wall_ms, &deserialized.stats.wall_ms);
            prop_assert_eq!(&receipt.stats.cpu_ms, &deserialized.stats.cpu_ms);
            prop_assert_eq!(&receipt.stats.page_faults, &deserialized.stats.page_faults);
            prop_assert_eq!(&receipt.stats.ctx_switches, &deserialized.stats.ctx_switches);
            prop_assert_eq!(&receipt.stats.max_rss_kb, &deserialized.stats.max_rss_kb);
            prop_assert_eq!(&receipt.stats.binary_bytes, &deserialized.stats.binary_bytes);

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
                    noise_threshold: None,
                    noise_policy: NoisePolicy::Ignore,
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
                    cv: None,
                    noise_threshold: None,
                    statistic: MetricStatistic::Median,
                    significance: None,
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
        (0u32..10, 0u32..10, 0u32..10, 0u32..10).prop_map(|(pass, warn, fail, skip)| {
            VerdictCounts {
                pass,
                warn,
                fail,
                skip,
            }
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
            Just(Metric::BinaryBytes),
            Just(Metric::CpuMs),
            Just(Metric::CtxSwitches),
            Just(Metric::MaxRssKb),
            Just(Metric::PageFaults),
            Just(Metric::ThroughputPerS),
            Just(Metric::WallMs),
        ]
    }

    // Strategy for BTreeMap<Metric, Budget>
    fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
        proptest::collection::btree_map(metric_strategy(), budget_strategy(), 0..8)
    }

    // Strategy for BTreeMap<Metric, Delta>
    fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
        proptest::collection::btree_map(metric_strategy(), delta_strategy(), 0..8)
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
                noise_threshold: None,
                noise_policy: None,
                threshold,
                direction,
                warn_factor,
                statistic: None,
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
                        scaling: None,
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
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
        )
            .prop_map(
                |(
                    repeat,
                    warmup,
                    threshold,
                    warn_factor,
                    out_dir,
                    baseline_dir,
                    baseline_pattern,
                    markdown_template,
                )| DefaultsConfig {
                    noise_threshold: None,
                    noise_policy: None,
                    repeat,
                    warmup,
                    threshold,
                    warn_factor,
                    out_dir,
                    baseline_dir,
                    baseline_pattern,
                    markdown_template,
                },
            )
    }

    // Strategy for BaselineServerConfig
    fn baseline_server_config_strategy() -> impl Strategy<Value = BaselineServerConfig> {
        (
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
            proptest::bool::ANY,
        )
            .prop_map(
                |(url, api_key, project, fallback_to_local)| BaselineServerConfig {
                    url,
                    api_key,
                    project,
                    fallback_to_local,
                },
            )
    }

    // Strategy for ConfigFile
    fn config_file_strategy() -> impl Strategy<Value = ConfigFile> {
        (
            defaults_config_strategy(),
            baseline_server_config_strategy(),
            proptest::collection::vec(bench_config_file_strategy(), 0..5),
        )
            .prop_map(|(defaults, baseline_server, benches)| ConfigFile {
                defaults,
                baseline_server,
                ratchet: None,
                benches,
            })
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
            prop_assert_eq!(
                &config.defaults.baseline_pattern,
                &deserialized.defaults.baseline_pattern
            );
            prop_assert_eq!(
                &config.defaults.markdown_template,
                &deserialized.defaults.markdown_template
            );

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
            prop_assert_eq!(
                &config.defaults.baseline_pattern,
                &deserialized.defaults.baseline_pattern
            );
            prop_assert_eq!(
                &config.defaults.markdown_template,
                &deserialized.defaults.markdown_template
            );

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

    // --- Leaf-type round-trip tests ---

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn host_info_serialization_round_trip(info in host_info_strategy()) {
            let json = serde_json::to_string(&info).expect("HostInfo should serialize");
            let back: HostInfo = serde_json::from_str(&json).expect("should deserialize");
            prop_assert_eq!(info, back);
        }

        #[test]
        fn sample_serialization_round_trip(sample in sample_strategy()) {
            let json = serde_json::to_string(&sample).expect("Sample should serialize");
            let back: Sample = serde_json::from_str(&json).expect("should deserialize");
            prop_assert_eq!(sample, back);
        }

        #[test]
        fn u64_summary_serialization_round_trip(summary in u64_summary_strategy()) {
            let json = serde_json::to_string(&summary).expect("U64Summary should serialize");
            let back: U64Summary = serde_json::from_str(&json).expect("should deserialize");
            prop_assert_eq!(summary, back);
        }

        #[test]
        fn f64_summary_serialization_round_trip(summary in f64_summary_strategy()) {
            let json = serde_json::to_string(&summary).expect("F64Summary should serialize");
            let back: F64Summary = serde_json::from_str(&json).expect("should deserialize");
            prop_assert!(f64_approx_eq(summary.min, back.min));
            prop_assert!(f64_approx_eq(summary.median, back.median));
            prop_assert!(f64_approx_eq(summary.max, back.max));
        }

        #[test]
        fn stats_serialization_round_trip(stats in stats_strategy()) {
            let json = serde_json::to_string(&stats).expect("Stats should serialize");
            let back: Stats = serde_json::from_str(&json).expect("should deserialize");
            prop_assert_eq!(&stats.wall_ms, &back.wall_ms);
            prop_assert_eq!(&stats.cpu_ms, &back.cpu_ms);
            prop_assert_eq!(&stats.page_faults, &back.page_faults);
            prop_assert_eq!(&stats.ctx_switches, &back.ctx_switches);
            prop_assert_eq!(&stats.max_rss_kb, &back.max_rss_kb);
            prop_assert_eq!(&stats.binary_bytes, &back.binary_bytes);
            match (&stats.throughput_per_s, &back.throughput_per_s) {
                (Some(orig), Some(deser)) => {
                    prop_assert!(f64_approx_eq(orig.min, deser.min));
                    prop_assert!(f64_approx_eq(orig.median, deser.median));
                    prop_assert!(f64_approx_eq(orig.max, deser.max));
                }
                (None, None) => {}
                _ => prop_assert!(false, "throughput_per_s presence mismatch"),
            }
        }

        #[test]
        fn delta_serialization_round_trip(delta in delta_strategy()) {
            let json = serde_json::to_string(&delta).expect("Delta should serialize");
            let back: Delta = serde_json::from_str(&json).expect("should deserialize");
            prop_assert!(f64_approx_eq(delta.baseline, back.baseline));
            prop_assert!(f64_approx_eq(delta.current, back.current));
            prop_assert!(f64_approx_eq(delta.ratio, back.ratio));
            prop_assert!(f64_approx_eq(delta.pct, back.pct));
            prop_assert!(f64_approx_eq(delta.regression, back.regression));
            prop_assert_eq!(delta.statistic, back.statistic);
            prop_assert_eq!(delta.significance, back.significance);
            prop_assert_eq!(delta.status, back.status);
        }

        #[test]
        fn verdict_serialization_round_trip(verdict in verdict_strategy()) {
            let json = serde_json::to_string(&verdict).expect("Verdict should serialize");
            let back: Verdict = serde_json::from_str(&json).expect("should deserialize");
            prop_assert_eq!(verdict, back);
        }
    }

    // --- PerfgateReport round-trip ---

    fn severity_strategy() -> impl Strategy<Value = Severity> {
        prop_oneof![Just(Severity::Warn), Just(Severity::Fail),]
    }

    fn finding_data_strategy() -> impl Strategy<Value = FindingData> {
        (
            non_empty_string(),
            0.1f64..10000.0,
            0.1f64..10000.0,
            0.0f64..100.0,
            0.01f64..1.0,
            direction_strategy(),
        )
            .prop_map(
                |(metric_name, baseline, current, regression_pct, threshold, direction)| {
                    FindingData {
                        metric_name,
                        baseline,
                        current,
                        regression_pct,
                        threshold,
                        direction,
                    }
                },
            )
    }

    fn report_finding_strategy() -> impl Strategy<Value = ReportFinding> {
        (
            non_empty_string(),
            non_empty_string(),
            severity_strategy(),
            non_empty_string(),
            proptest::option::of(finding_data_strategy()),
        )
            .prop_map(|(check_id, code, severity, message, data)| ReportFinding {
                check_id,
                code,
                severity,
                message,
                data,
            })
    }

    fn report_summary_strategy() -> impl Strategy<Value = ReportSummary> {
        (0u32..100, 0u32..100, 0u32..100, 0u32..100).prop_map(
            |(pass_count, warn_count, fail_count, skip_count)| ReportSummary {
                pass_count,
                warn_count,
                fail_count,
                skip_count,
                total_count: pass_count + warn_count + fail_count + skip_count,
            },
        )
    }

    fn perfgate_report_strategy() -> impl Strategy<Value = PerfgateReport> {
        (
            verdict_strategy(),
            proptest::option::of(compare_receipt_strategy()),
            proptest::collection::vec(report_finding_strategy(), 0..5),
            report_summary_strategy(),
        )
            .prop_map(|(verdict, compare, findings, summary)| PerfgateReport {
                report_type: REPORT_SCHEMA_V1.to_string(),
                verdict,
                compare,
                findings,
                summary,
                profile_path: None,
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        #[test]
        fn perfgate_report_serialization_round_trip(report in perfgate_report_strategy()) {
            let json = serde_json::to_string(&report)
                .expect("PerfgateReport should serialize to JSON");
            let back: PerfgateReport = serde_json::from_str(&json)
                .expect("JSON should deserialize back to PerfgateReport");

            prop_assert_eq!(&report.report_type, &back.report_type);
            prop_assert_eq!(&report.verdict, &back.verdict);
            prop_assert_eq!(&report.summary, &back.summary);
            prop_assert_eq!(report.findings.len(), back.findings.len());
            for (orig, deser) in report.findings.iter().zip(back.findings.iter()) {
                prop_assert_eq!(&orig.check_id, &deser.check_id);
                prop_assert_eq!(&orig.code, &deser.code);
                prop_assert_eq!(orig.severity, deser.severity);
                prop_assert_eq!(&orig.message, &deser.message);
                match (&orig.data, &deser.data) {
                    (Some(o), Some(d)) => {
                        prop_assert_eq!(&o.metric_name, &d.metric_name);
                        prop_assert!(f64_approx_eq(o.baseline, d.baseline));
                        prop_assert!(f64_approx_eq(o.current, d.current));
                        prop_assert!(f64_approx_eq(o.regression_pct, d.regression_pct));
                        prop_assert!(f64_approx_eq(o.threshold, d.threshold));
                        prop_assert_eq!(o.direction, d.direction);
                    }
                    (None, None) => {}
                    _ => prop_assert!(false, "finding data presence mismatch"),
                }
            }
            // CompareReceipt equality is covered by compare_receipt round-trip;
            // just verify presence matches.
            prop_assert_eq!(report.compare.is_some(), back.compare.is_some());
        }
    }
}

// --- Golden fixture tests for sensor.report.v1 ---

#[cfg(test)]
mod golden_tests {
    use super::*;

    const FIXTURE_PASS: &str = include_str!("../../../contracts/fixtures/sensor_report_pass.json");
    const FIXTURE_FAIL: &str = include_str!("../../../contracts/fixtures/sensor_report_fail.json");
    const FIXTURE_WARN: &str = include_str!("../../../contracts/fixtures/sensor_report_warn.json");
    const FIXTURE_NO_BASELINE: &str =
        include_str!("../../../contracts/fixtures/sensor_report_no_baseline.json");
    const FIXTURE_ERROR: &str =
        include_str!("../../../contracts/fixtures/sensor_report_error.json");
    const FIXTURE_MULTI_BENCH: &str =
        include_str!("../../../contracts/fixtures/sensor_report_multi_bench.json");

    #[test]
    fn golden_sensor_report_pass() {
        let report: SensorReport =
            serde_json::from_str(FIXTURE_PASS).expect("fixture should parse");
        assert_eq!(report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(report.tool.name, "perfgate");
        assert_eq!(report.verdict.status, SensorVerdictStatus::Pass);
        assert_eq!(report.verdict.counts.warn, 0);
        assert_eq!(report.verdict.counts.error, 0);
        assert!(report.findings.is_empty());
        assert_eq!(report.artifacts.len(), 4);

        // Round-trip the parsed fixture
        let json = serde_json::to_string(&report).unwrap();
        let back: SensorReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn golden_sensor_report_fail() {
        let report: SensorReport =
            serde_json::from_str(FIXTURE_FAIL).expect("fixture should parse");
        assert_eq!(report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(report.verdict.counts.error, 1);
        assert_eq!(report.verdict.reasons, vec!["wall_ms_fail"]);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].check_id, CHECK_ID_BUDGET);
        assert_eq!(report.findings[0].code, FINDING_CODE_METRIC_FAIL);
        assert_eq!(report.findings[0].severity, SensorSeverity::Error);

        let json = serde_json::to_string(&report).unwrap();
        let back: SensorReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn golden_sensor_report_warn() {
        let report: SensorReport =
            serde_json::from_str(FIXTURE_WARN).expect("fixture should parse");
        assert_eq!(report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(report.verdict.status, SensorVerdictStatus::Warn);
        assert_eq!(report.verdict.counts.warn, 1);
        assert_eq!(report.verdict.reasons, vec!["wall_ms_warn"]);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, SensorSeverity::Warn);

        let json = serde_json::to_string(&report).unwrap();
        let back: SensorReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn golden_sensor_report_no_baseline() {
        let report: SensorReport =
            serde_json::from_str(FIXTURE_NO_BASELINE).expect("fixture should parse");
        assert_eq!(report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(report.verdict.status, SensorVerdictStatus::Warn);
        assert_eq!(report.verdict.reasons, vec!["no_baseline"]);
        assert_eq!(
            report.run.capabilities.baseline.status,
            CapabilityStatus::Unavailable
        );
        assert_eq!(
            report.run.capabilities.baseline.reason.as_deref(),
            Some("no_baseline")
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].code, FINDING_CODE_BASELINE_MISSING);

        let json = serde_json::to_string(&report).unwrap();
        let back: SensorReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn golden_sensor_report_error() {
        let report: SensorReport =
            serde_json::from_str(FIXTURE_ERROR).expect("fixture should parse");
        assert_eq!(report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(report.verdict.reasons, vec!["tool_error"]);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].check_id, CHECK_ID_TOOL_RUNTIME);
        assert_eq!(report.findings[0].code, FINDING_CODE_RUNTIME_ERROR);
        assert!(report.artifacts.is_empty());

        let json = serde_json::to_string(&report).unwrap();
        let back: SensorReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }

    #[test]
    fn golden_sensor_report_multi_bench() {
        let report: SensorReport =
            serde_json::from_str(FIXTURE_MULTI_BENCH).expect("fixture should parse");
        assert_eq!(report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(report.verdict.status, SensorVerdictStatus::Warn);
        assert_eq!(report.verdict.counts.warn, 2);
        assert_eq!(report.findings.len(), 2);
        // Both findings are baseline-missing for different benches
        for finding in &report.findings {
            assert_eq!(finding.code, FINDING_CODE_BASELINE_MISSING);
        }
        assert_eq!(report.artifacts.len(), 5);

        let json = serde_json::to_string(&report).unwrap();
        let back: SensorReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, back);
    }
}
