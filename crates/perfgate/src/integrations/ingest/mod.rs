//! Import benchmark results from external frameworks into perfgate's native format.
//!
//! Supports:
//! - **Criterion** (`target/criterion/**/new/estimates.json`)
//! - **hyperfine** (`--export-json` output)
//! - **Go benchmark** (`go test -bench . -benchmem` text output)
//! - **pytest-benchmark** (`.benchmarks/*.json`)

mod criterion;
mod gobench;
mod hyperfine;
mod otel;
mod pytest;

use perfgate_types::{
    BenchMeta, HostInfo, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
};
use time::OffsetDateTime;
use uuid::Uuid;

pub use criterion::parse_criterion;
pub use gobench::parse_gobench;
pub use hyperfine::parse_hyperfine;
pub use otel::parse_otel_json;
pub use pytest::parse_pytest_benchmark;

/// Supported ingest formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestFormat {
    Criterion,
    Hyperfine,
    GoBench,
    PytestBenchmark,
    Otel,
}

impl IngestFormat {
    /// Parse a format string (case-insensitive) into an `IngestFormat`.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "criterion" => Some(Self::Criterion),
            "hyperfine" => Some(Self::Hyperfine),
            "gobench" | "go" => Some(Self::GoBench),
            "pytest" | "pytest-benchmark" | "pytest_benchmark" => Some(Self::PytestBenchmark),
            "otel" | "opentelemetry" => Some(Self::Otel),
            _ => None,
        }
    }
}

/// Request to ingest external benchmark data.
pub struct IngestRequest {
    /// The format of the input data.
    pub format: IngestFormat,
    /// Raw input content (file contents).
    pub input: String,
    /// Benchmark name override. If None, derived from the input data.
    pub name: Option<String>,
    /// Optional include filter for span names (exact match).
    pub include_spans: Vec<String>,
    /// Optional exclude filter for span names (exact match).
    pub exclude_spans: Vec<String>,
}

/// Perform an ingest operation, returning a `RunReceipt`.
pub fn ingest(request: &IngestRequest) -> anyhow::Result<RunReceipt> {
    match request.format {
        IngestFormat::Criterion => parse_criterion(&request.input, request.name.as_deref()),
        IngestFormat::Hyperfine => parse_hyperfine(&request.input, request.name.as_deref()),
        IngestFormat::GoBench => parse_gobench(&request.input, request.name.as_deref()),
        IngestFormat::PytestBenchmark => {
            parse_pytest_benchmark(&request.input, request.name.as_deref())
        }
        IngestFormat::Otel => parse_otel_json(
            &request.input,
            request.name.as_deref(),
            &request.include_spans,
            &request.exclude_spans,
        ),
    }
}

/// Build scaffolding for a `RunReceipt` with sensible defaults.
fn make_receipt(name: &str, samples: Vec<Sample>, stats: Stats) -> RunReceipt {
    let now = OffsetDateTime::now_utc();
    let timestamp = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate-ingest".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        run: RunMeta {
            id: Uuid::new_v4().to_string(),
            started_at: timestamp.clone(),
            ended_at: timestamp,
            host: HostInfo {
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: name.to_string(),
            cwd: None,
            command: vec!["(ingested)".to_string()],
            repeat: samples.len() as u32,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples,
        stats,
    }
}

/// Compute a `U64Summary` from a slice of u64 values.
fn compute_u64_summary(values: &[u64]) -> U64Summary {
    if values.is_empty() {
        return U64Summary {
            median: 0,
            min: 0,
            max: 0,
            mean: None,
            stddev: None,
        };
    }

    let mut sorted = values.to_vec();
    sorted.sort_unstable();

    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2
    } else {
        sorted[sorted.len() / 2]
    };

    let sum: f64 = values.iter().map(|&v| v as f64).sum();
    let mean = sum / values.len() as f64;

    let variance = values
        .iter()
        .map(|&v| (v as f64 - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64;
    let stddev = variance.sqrt();

    U64Summary {
        median,
        min,
        max,
        mean: Some(mean),
        stddev: Some(stddev),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingest_format_parse() {
        assert_eq!(
            IngestFormat::parse("criterion"),
            Some(IngestFormat::Criterion)
        );
        assert_eq!(
            IngestFormat::parse("Criterion"),
            Some(IngestFormat::Criterion)
        );
        assert_eq!(
            IngestFormat::parse("hyperfine"),
            Some(IngestFormat::Hyperfine)
        );
        assert_eq!(IngestFormat::parse("gobench"), Some(IngestFormat::GoBench));
        assert_eq!(IngestFormat::parse("go"), Some(IngestFormat::GoBench));
        assert_eq!(
            IngestFormat::parse("pytest"),
            Some(IngestFormat::PytestBenchmark)
        );
        assert_eq!(
            IngestFormat::parse("pytest-benchmark"),
            Some(IngestFormat::PytestBenchmark)
        );
        assert_eq!(
            IngestFormat::parse("pytest_benchmark"),
            Some(IngestFormat::PytestBenchmark)
        );
        assert_eq!(IngestFormat::parse("otel"), Some(IngestFormat::Otel));
        assert_eq!(
            IngestFormat::parse("opentelemetry"),
            Some(IngestFormat::Otel)
        );
        assert_eq!(IngestFormat::parse("unknown"), None);
    }

    #[test]
    fn test_compute_u64_summary_basic() {
        let values = vec![100, 200, 300, 400, 500];
        let summary = compute_u64_summary(&values);
        assert_eq!(summary.median, 300);
        assert_eq!(summary.min, 100);
        assert_eq!(summary.max, 500);
        assert!(summary.mean.is_some());
        assert!((summary.mean.unwrap() - 300.0).abs() < 0.001);
    }

    #[test]
    fn test_compute_u64_summary_even_count() {
        let values = vec![100, 200, 300, 400];
        let summary = compute_u64_summary(&values);
        assert_eq!(summary.median, 250);
        assert_eq!(summary.min, 100);
        assert_eq!(summary.max, 400);
    }

    #[test]
    fn test_compute_u64_summary_empty() {
        let summary = compute_u64_summary(&[]);
        assert_eq!(summary.median, 0);
        assert_eq!(summary.min, 0);
        assert_eq!(summary.max, 0);
        assert!(summary.mean.is_none());
    }

    #[test]
    fn test_compute_u64_summary_single() {
        let values = vec![42];
        let summary = compute_u64_summary(&values);
        assert_eq!(summary.median, 42);
        assert_eq!(summary.min, 42);
        assert_eq!(summary.max, 42);
        assert!((summary.mean.unwrap() - 42.0).abs() < 0.001);
        assert!((summary.stddev.unwrap()).abs() < 0.001);
    }

    #[test]
    fn test_make_receipt_structure() {
        let samples = vec![Sample {
            wall_ms: 100,
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
        }];
        let stats = Stats {
            wall_ms: U64Summary::new(100, 100, 100),
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
        };
        let receipt = make_receipt("test-bench", samples, stats);
        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "test-bench");
        assert_eq!(receipt.bench.repeat, 1);
        assert_eq!(receipt.tool.name, "perfgate-ingest");
    }
}
