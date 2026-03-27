//! Parser for hyperfine benchmark results.
//!
//! hyperfine's `--export-json` output has a top-level `results` array,
//! where each entry contains `command`, `times` (array of seconds),
//! `mean`, `stddev`, `median`, `min`, `max`, etc.

use anyhow::Context;
use perfgate_types::{RunReceipt, Sample, Stats};
use serde::Deserialize;

use crate::{compute_u64_summary, make_receipt};

/// A single result entry from hyperfine JSON output.
#[derive(Debug, Deserialize)]
struct HyperfineResult {
    command: String,
    /// Raw timing data in seconds.
    times: Vec<f64>,
    mean: f64,
    stddev: f64,
    median: f64,
    min: f64,
    max: f64,
}

/// Top-level hyperfine JSON structure.
#[derive(Debug, Deserialize)]
struct HyperfineOutput {
    results: Vec<HyperfineResult>,
}

/// Parse a hyperfine JSON export into a `RunReceipt`.
///
/// If the export contains multiple results (multiple commands benchmarked),
/// only the first result is used. Use the `name` parameter to override
/// the benchmark name (defaults to the command string).
pub fn parse_hyperfine(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let output: HyperfineOutput =
        serde_json::from_str(input).context("failed to parse hyperfine JSON")?;

    let result = output
        .results
        .first()
        .context("hyperfine JSON contains no results")?;

    let bench_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| result.command.clone());

    // hyperfine times are in seconds; convert to milliseconds.
    let mut wall_values = Vec::new();
    let mut samples = Vec::new();

    for &t in &result.times {
        let ms = seconds_to_ms(t);
        wall_values.push(ms);
        samples.push(Sample {
            wall_ms: ms,
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
        });
    }

    let mut stats = compute_u64_summary(&wall_values);
    // Override with hyperfine's own statistics (more precise)
    stats.median = seconds_to_ms(result.median);
    stats.min = seconds_to_ms(result.min);
    stats.max = seconds_to_ms(result.max);
    stats.mean = Some(result.mean * 1000.0);
    stats.stddev = Some(result.stddev * 1000.0);

    let full_stats = Stats {
        wall_ms: stats,
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

    Ok(make_receipt(&bench_name, samples, full_stats))
}

fn seconds_to_ms(s: f64) -> u64 {
    let ms = s * 1000.0;
    if ms < 1.0 && ms > 0.0 {
        1
    } else {
        ms.round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::RUN_SCHEMA_V1;

    const HYPERFINE_JSON: &str = r#"{
        "results": [
            {
                "command": "sleep 0.1",
                "mean": 0.1023,
                "stddev": 0.0015,
                "median": 0.1020,
                "user": 0.001,
                "system": 0.002,
                "min": 0.1001,
                "max": 0.1056,
                "times": [0.1001, 0.1015, 0.1020, 0.1030, 0.1056],
                "exit_codes": [0, 0, 0, 0, 0]
            }
        ]
    }"#;

    #[test]
    fn parse_hyperfine_basic() {
        let receipt = parse_hyperfine(HYPERFINE_JSON, Some("sleep-bench")).unwrap();
        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "sleep-bench");
        assert_eq!(receipt.samples.len(), 5);
        // 0.102 seconds = 102 ms
        assert_eq!(receipt.stats.wall_ms.median, 102);
        assert_eq!(receipt.stats.wall_ms.min, 100);
        assert_eq!(receipt.stats.wall_ms.max, 106);
    }

    #[test]
    fn parse_hyperfine_default_name() {
        let receipt = parse_hyperfine(HYPERFINE_JSON, None).unwrap();
        assert_eq!(receipt.bench.name, "sleep 0.1");
    }

    #[test]
    fn parse_hyperfine_sample_wall_ms() {
        let receipt = parse_hyperfine(HYPERFINE_JSON, None).unwrap();
        // Each sample should have its own wall_ms from the times array
        let wall_values: Vec<u64> = receipt.samples.iter().map(|s| s.wall_ms).collect();
        assert_eq!(wall_values, vec![100, 102, 102, 103, 106]);
    }

    #[test]
    fn parse_hyperfine_multiple_results() {
        // Only the first result should be used
        let input = r#"{
            "results": [
                {
                    "command": "echo first",
                    "mean": 0.005,
                    "stddev": 0.001,
                    "median": 0.005,
                    "user": 0.001,
                    "system": 0.001,
                    "min": 0.004,
                    "max": 0.006,
                    "times": [0.004, 0.005, 0.006],
                    "exit_codes": [0, 0, 0]
                },
                {
                    "command": "echo second",
                    "mean": 0.010,
                    "stddev": 0.002,
                    "median": 0.010,
                    "user": 0.001,
                    "system": 0.001,
                    "min": 0.008,
                    "max": 0.012,
                    "times": [0.008, 0.010, 0.012],
                    "exit_codes": [0, 0, 0]
                }
            ]
        }"#;
        let receipt = parse_hyperfine(input, None).unwrap();
        assert_eq!(receipt.bench.name, "echo first");
    }

    #[test]
    fn parse_hyperfine_empty_results() {
        let input = r#"{"results": []}"#;
        let result = parse_hyperfine(input, None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_hyperfine_invalid_json() {
        let result = parse_hyperfine("{bad json", None);
        assert!(result.is_err());
    }
}
