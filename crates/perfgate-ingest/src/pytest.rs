//! Parser for pytest-benchmark JSON output.
//!
//! pytest-benchmark stores results in `.benchmarks/*.json` with a structure
//! containing `benchmarks[]` with `name`, `stats` (min, max, mean, median,
//! stddev, rounds, iterations), etc. All timing values are in seconds.

use anyhow::Context;
use perfgate_types::{RunReceipt, Sample, Stats};
use serde::Deserialize;

use crate::{compute_u64_summary, make_receipt};

/// Statistics from a single pytest-benchmark entry.
#[derive(Debug, Deserialize)]
struct PytestStats {
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    stddev: f64,
    rounds: u64,
}

/// A single benchmark entry from pytest-benchmark JSON.
#[derive(Debug, Deserialize)]
struct PytestBenchmark {
    name: String,
    stats: PytestStats,
}

/// Top-level pytest-benchmark JSON structure.
#[derive(Debug, Deserialize)]
struct PytestOutput {
    benchmarks: Vec<PytestBenchmark>,
}

/// Parse a pytest-benchmark JSON file into a `RunReceipt`.
///
/// If the JSON contains multiple benchmarks, only the first is used.
/// All timing values in the input are in seconds and are converted
/// to milliseconds for the `wall_ms` metric.
pub fn parse_pytest_benchmark(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let output: PytestOutput =
        serde_json::from_str(input).context("failed to parse pytest-benchmark JSON")?;

    let bench = output
        .benchmarks
        .first()
        .context("pytest-benchmark JSON contains no benchmarks")?;

    let bench_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| bench.name.clone());

    let stats = &bench.stats;

    // pytest-benchmark times are in seconds; convert to milliseconds.
    // Generate synthetic samples: we create `rounds` samples spread around the
    // mean with the reported stddev. If rounds is large, we cap at 30 samples.
    let num_samples = stats.rounds.min(30) as usize;
    let num_samples = num_samples.max(1);

    let mut wall_values = Vec::new();
    let mut samples = Vec::new();

    if num_samples == 1 {
        let ms = seconds_to_ms(stats.mean);
        wall_values.push(ms);
        samples.push(make_sample(ms));
    } else {
        // Generate evenly-spaced samples between min and max
        for i in 0..num_samples {
            let t = if num_samples > 1 {
                let frac = i as f64 / (num_samples - 1) as f64;
                stats.min + frac * (stats.max - stats.min)
            } else {
                stats.mean
            };
            let ms = seconds_to_ms(t);
            wall_values.push(ms);
            samples.push(make_sample(ms));
        }
    }

    let mut computed = compute_u64_summary(&wall_values);
    // Override with the actual pytest-benchmark statistics
    computed.median = seconds_to_ms(stats.median);
    computed.min = seconds_to_ms(stats.min);
    computed.max = seconds_to_ms(stats.max);
    computed.mean = Some(stats.mean * 1000.0);
    computed.stddev = Some(stats.stddev * 1000.0);

    let full_stats = Stats {
        wall_ms: computed,
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

fn make_sample(wall_ms: u64) -> Sample {
    Sample {
        wall_ms,
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
    }
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

    const PYTEST_JSON: &str = r#"{
        "machine_info": {
            "node": "test-host",
            "processor": "x86_64",
            "machine": "x86_64",
            "python_implementation": "CPython",
            "python_version": "3.11.0",
            "python_compiler": "GCC 12.2.0",
            "release": "6.1.0",
            "system": "Linux"
        },
        "commit_info": {
            "id": "abc123"
        },
        "benchmarks": [
            {
                "group": null,
                "name": "test_sort",
                "fullname": "tests/test_perf.py::test_sort",
                "params": null,
                "param": null,
                "extra_info": {},
                "options": {
                    "disable_gc": false,
                    "timer": "perf_counter",
                    "min_rounds": 5,
                    "max_time": 1.0,
                    "min_time": 0.000005,
                    "warmup": false
                },
                "stats": {
                    "min": 0.0234,
                    "max": 0.0312,
                    "mean": 0.0256,
                    "stddev": 0.0021,
                    "rounds": 10,
                    "iterations": 1,
                    "median": 0.0250,
                    "iqr": 0.0030,
                    "q1": 0.0240,
                    "q3": 0.0270,
                    "iqr_outliers": 0,
                    "stddev_outliers": 1,
                    "outliers": "1;0",
                    "ld15iqr": 0.0234,
                    "hd15iqr": 0.0312,
                    "ops": 39.0625,
                    "total": 0.256
                }
            }
        ],
        "datetime": "2024-01-15T10:30:00.000000",
        "version": "4.0.0"
    }"#;

    #[test]
    fn parse_pytest_basic() {
        let receipt = parse_pytest_benchmark(PYTEST_JSON, Some("sort-bench")).unwrap();
        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "sort-bench");
        // 10 rounds -> 10 samples
        assert_eq!(receipt.samples.len(), 10);
        // median 0.025s = 25ms (rounded)
        assert_eq!(receipt.stats.wall_ms.median, 25);
        assert_eq!(receipt.stats.wall_ms.min, 23);
        assert_eq!(receipt.stats.wall_ms.max, 31);
    }

    #[test]
    fn parse_pytest_default_name() {
        let receipt = parse_pytest_benchmark(PYTEST_JSON, None).unwrap();
        assert_eq!(receipt.bench.name, "test_sort");
    }

    #[test]
    fn parse_pytest_sample_count_capped() {
        // If rounds is very large, should cap at 30
        let input = r#"{
            "benchmarks": [
                {
                    "name": "test_big",
                    "stats": {
                        "min": 0.010,
                        "max": 0.020,
                        "mean": 0.015,
                        "stddev": 0.002,
                        "rounds": 1000,
                        "iterations": 1,
                        "median": 0.015
                    }
                }
            ]
        }"#;
        let receipt = parse_pytest_benchmark(input, None).unwrap();
        assert_eq!(receipt.samples.len(), 30);
    }

    #[test]
    fn parse_pytest_single_round() {
        let input = r#"{
            "benchmarks": [
                {
                    "name": "test_single",
                    "stats": {
                        "min": 0.100,
                        "max": 0.100,
                        "mean": 0.100,
                        "stddev": 0.0,
                        "rounds": 1,
                        "iterations": 1,
                        "median": 0.100
                    }
                }
            ]
        }"#;
        let receipt = parse_pytest_benchmark(input, None).unwrap();
        assert_eq!(receipt.samples.len(), 1);
        assert_eq!(receipt.samples[0].wall_ms, 100);
    }

    #[test]
    fn parse_pytest_empty_benchmarks() {
        let input = r#"{"benchmarks": []}"#;
        let result = parse_pytest_benchmark(input, None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_pytest_invalid_json() {
        let result = parse_pytest_benchmark("not json", None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_pytest_submillisecond() {
        let input = r#"{
            "benchmarks": [
                {
                    "name": "test_fast",
                    "stats": {
                        "min": 0.0001,
                        "max": 0.0003,
                        "mean": 0.0002,
                        "stddev": 0.00005,
                        "rounds": 5,
                        "iterations": 100,
                        "median": 0.0002
                    }
                }
            ]
        }"#;
        let receipt = parse_pytest_benchmark(input, None).unwrap();
        // All sub-millisecond values should clamp to 1
        for sample in &receipt.samples {
            assert!(sample.wall_ms >= 1, "wall_ms was {} < 1", sample.wall_ms);
        }
    }
}
