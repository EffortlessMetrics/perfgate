//! Parser for Criterion benchmark results.
//!
//! Criterion stores estimates in `target/criterion/<bench>/new/estimates.json`.
//! The JSON structure contains `mean`, `median`, `std_dev` with `point_estimate`
//! and `confidence_interval` sub-objects, all in nanoseconds.

use anyhow::Context;
use perfgate_types::{RunReceipt, Sample, Stats};
use serde::Deserialize;

use crate::{compute_u64_summary, make_receipt};

/// A Criterion estimate entry (mean, median, std_dev, etc.).
#[derive(Debug, Deserialize)]
struct CriterionEstimate {
    point_estimate: f64,
    // confidence_interval is available but we don't need it for basic ingest
}

/// Top-level Criterion estimates.json structure.
#[derive(Debug, Deserialize)]
struct CriterionEstimates {
    mean: CriterionEstimate,
    median: CriterionEstimate,
    std_dev: CriterionEstimate,
    #[serde(default)]
    slope: Option<CriterionEstimate>,
}

/// Parse a Criterion `estimates.json` file into a `RunReceipt`.
///
/// Criterion reports timing in nanoseconds. We convert to milliseconds
/// for the `wall_ms` metric. Since Criterion provides summary statistics
/// rather than raw samples, we synthesize a sample set from the mean value.
pub fn parse_criterion(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let estimates: CriterionEstimates =
        serde_json::from_str(input).context("failed to parse Criterion estimates.json")?;

    let bench_name = name.unwrap_or("criterion-bench").to_string();

    // Criterion values are in nanoseconds; convert to milliseconds (u64).
    // Use the slope estimate if available (more accurate for iterated benchmarks),
    // otherwise fall back to mean.
    let primary = estimates.slope.as_ref().unwrap_or(&estimates.mean);
    let mean_ns = primary.point_estimate;
    let median_ns = estimates.median.point_estimate;
    let std_dev_ns = estimates.std_dev.point_estimate;

    // Convert ns to ms. For sub-millisecond benchmarks we clamp to 1ms minimum
    // to avoid zero values in the u64 field.
    let mean_ms = ns_to_ms(mean_ns);
    let median_ms = ns_to_ms(median_ns);

    // Synthesize samples from the summary statistics.
    // We generate 5 synthetic samples centered around the mean with the reported std_dev.
    let offsets = [-2.0, -1.0, 0.0, 1.0, 2.0];
    let mut wall_values = Vec::new();
    let mut samples = Vec::new();

    for &offset in &offsets {
        let ns = mean_ns + offset * std_dev_ns;
        let ms = ns_to_ms(ns.max(0.0));
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
    // Override median with the actual Criterion median
    stats.median = median_ms;
    // Override mean with the actual Criterion mean
    stats.mean = Some(mean_ms as f64);
    stats.stddev = Some(ns_to_ms(std_dev_ns) as f64);

    let wall_stats = stats;

    let full_stats = Stats {
        wall_ms: wall_stats,
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

fn ns_to_ms(ns: f64) -> u64 {
    let ms = ns / 1_000_000.0;
    if ms < 1.0 && ms > 0.0 {
        // Sub-millisecond: round to nearest, minimum 1
        1
    } else {
        ms.round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::RUN_SCHEMA_V1;

    const CRITERION_ESTIMATES: &str = r#"{
        "mean": {
            "point_estimate": 5000000.0,
            "standard_error": 50000.0,
            "confidence_interval": {
                "confidence_level": 0.95,
                "lower_bound": 4900000.0,
                "upper_bound": 5100000.0
            }
        },
        "median": {
            "point_estimate": 4950000.0,
            "standard_error": 30000.0,
            "confidence_interval": {
                "confidence_level": 0.95,
                "lower_bound": 4890000.0,
                "upper_bound": 5010000.0
            }
        },
        "std_dev": {
            "point_estimate": 200000.0,
            "standard_error": 10000.0,
            "confidence_interval": {
                "confidence_level": 0.95,
                "lower_bound": 180000.0,
                "upper_bound": 220000.0
            }
        }
    }"#;

    #[test]
    fn parse_criterion_basic() {
        let receipt = parse_criterion(CRITERION_ESTIMATES, Some("my-bench")).unwrap();
        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "my-bench");
        assert_eq!(receipt.samples.len(), 5);
        // 5_000_000 ns = 5 ms
        assert_eq!(receipt.stats.wall_ms.median, 5); // 4_950_000 ns ~ 5ms
        assert!(receipt.stats.wall_ms.min <= receipt.stats.wall_ms.max);
    }

    #[test]
    fn parse_criterion_default_name() {
        let receipt = parse_criterion(CRITERION_ESTIMATES, None).unwrap();
        assert_eq!(receipt.bench.name, "criterion-bench");
    }

    #[test]
    fn parse_criterion_with_slope() {
        let input = r#"{
            "mean": {
                "point_estimate": 10000000.0,
                "standard_error": 100000.0,
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 9800000.0, "upper_bound": 10200000.0}
            },
            "median": {
                "point_estimate": 9900000.0,
                "standard_error": 50000.0,
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 9800000.0, "upper_bound": 10000000.0}
            },
            "std_dev": {
                "point_estimate": 500000.0,
                "standard_error": 25000.0,
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 450000.0, "upper_bound": 550000.0}
            },
            "slope": {
                "point_estimate": 9500000.0,
                "standard_error": 80000.0,
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 9340000.0, "upper_bound": 9660000.0}
            }
        }"#;

        let receipt = parse_criterion(input, Some("slope-bench")).unwrap();
        assert_eq!(receipt.bench.name, "slope-bench");
        // Should use slope (9.5ms) rather than mean (10ms) for sample generation
        assert_eq!(receipt.samples.len(), 5);
    }

    #[test]
    fn parse_criterion_submillisecond() {
        // 500 ns mean, 50 ns stddev
        let input = r#"{
            "mean": {
                "point_estimate": 500.0,
                "standard_error": 5.0,
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 490.0, "upper_bound": 510.0}
            },
            "median": {
                "point_estimate": 498.0,
                "standard_error": 3.0,
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 492.0, "upper_bound": 504.0}
            },
            "std_dev": {
                "point_estimate": 50.0,
                "standard_error": 2.0,
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 46.0, "upper_bound": 54.0}
            }
        }"#;

        let receipt = parse_criterion(input, Some("fast-bench")).unwrap();
        // Sub-millisecond values should clamp to 1ms minimum
        for sample in &receipt.samples {
            assert!(sample.wall_ms >= 1);
        }
    }

    #[test]
    fn parse_criterion_invalid_json() {
        let result = parse_criterion("not json", Some("x"));
        assert!(result.is_err());
    }
}
