//! Parser for hyperfine benchmark results.
//!
//! hyperfine's `--export-json` output has a top-level `results` array,
//! where each entry contains `command`, `times` (array of seconds),
//! `mean`, `stddev`, `median`, `min`, `max`, etc.

use anyhow::{Context, bail};
use perfgate_types::{
    BenchMeta, HostInfo, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
};
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use super::compute_u64_summary;

/// A single result entry from hyperfine JSON output.
#[derive(Debug, Deserialize)]
struct HyperfineResult {
    command: String,
    /// Raw timing data in seconds.
    times: Vec<f64>,
    #[serde(default)]
    exit_codes: Option<Vec<i32>>,
    mean: f64,
    stddev: f64,
    median: f64,
    min: f64,
    max: f64,
    #[serde(default)]
    user: Option<f64>,
    #[serde(default)]
    system: Option<f64>,
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

    if result.times.is_empty() {
        bail!("hyperfine JSON result requires non-empty raw times");
    }
    if let Some(exit_codes) = &result.exit_codes
        && exit_codes.len() != result.times.len()
    {
        bail!(
            "hyperfine JSON exit_codes length ({}) does not match times length ({})",
            exit_codes.len(),
            result.times.len()
        );
    }

    let mut wall_values = Vec::new();
    let mut samples = Vec::new();

    for (index, &time_seconds) in result.times.iter().enumerate() {
        let wall_ms = seconds_to_ms(time_seconds, "times")?;
        wall_values.push(wall_ms);
        samples.push(Sample {
            wall_ms,
            exit_code: result
                .exit_codes
                .as_ref()
                .and_then(|exit_codes| exit_codes.get(index).copied())
                .unwrap_or(0),
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
    stats.median = seconds_to_ms(result.median, "median")?;
    stats.min = seconds_to_ms(result.min, "min")?;
    stats.max = seconds_to_ms(result.max, "max")?;
    stats.mean = Some(seconds_to_ms_f64(result.mean, "mean")?);
    stats.stddev = Some(seconds_to_ms_f64(result.stddev, "stddev")?);

    let full_stats = Stats {
        wall_ms: stats,
        cpu_ms: cpu_ms_summary(result.user, result.system)?,
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

    Ok(make_hyperfine_receipt(
        &bench_name,
        &result.command,
        samples,
        full_stats,
    ))
}

/// Integer seconds-to-ms conversion for sample `wall_ms` values (u64).
///
/// This intentionally rounds to integer milliseconds. For stats fields
/// (mean, stddev), use direct f64 arithmetic to preserve sub-millisecond
/// precision.
fn seconds_to_ms(seconds: f64, field: &str) -> anyhow::Result<u64> {
    if !seconds.is_finite() || seconds < 0.0 {
        bail!("hyperfine JSON field '{field}' must be finite and non-negative");
    }
    let milliseconds = seconds * 1000.0;
    if milliseconds < 1.0 && milliseconds > 0.0 {
        Ok(1)
    } else {
        Ok(milliseconds.round() as u64)
    }
}

fn seconds_to_ms_f64(seconds: f64, field: &str) -> anyhow::Result<f64> {
    if !seconds.is_finite() || seconds < 0.0 {
        bail!("hyperfine JSON field '{field}' must be finite and non-negative");
    }
    Ok(seconds * 1000.0)
}

fn cpu_ms_summary(user: Option<f64>, system: Option<f64>) -> anyhow::Result<Option<U64Summary>> {
    if user.is_none() && system.is_none() {
        return Ok(None);
    }

    let user_ms = user
        .map(|value| seconds_to_ms_f64(value, "user"))
        .transpose()?
        .unwrap_or(0.0);
    let system_ms = system
        .map(|value| seconds_to_ms_f64(value, "system"))
        .transpose()?
        .unwrap_or(0.0);
    let cpu_ms = user_ms + system_ms;
    let rounded = if cpu_ms < 1.0 && cpu_ms > 0.0 {
        1
    } else {
        cpu_ms.round() as u64
    };

    Ok(Some(U64Summary {
        median: rounded,
        min: rounded,
        max: rounded,
        mean: Some(cpu_ms),
        stddev: None,
    }))
}

fn make_hyperfine_receipt(
    name: &str,
    command: &str,
    samples: Vec<Sample>,
    stats: Stats,
) -> RunReceipt {
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
                os: "unknown".to_string(),
                arch: "unknown".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: name.to_string(),
            cwd: None,
            command: vec![command.to_string()],
            repeat: samples.len() as u32,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples,
        stats,
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
        assert_eq!(receipt.bench.command, vec!["sleep 0.1"]);
        assert_eq!(receipt.samples.len(), 5);
        assert_eq!(receipt.stats.wall_ms.median, 102);
        assert_eq!(receipt.stats.wall_ms.min, 100);
        assert_eq!(receipt.stats.wall_ms.max, 106);
        assert_close(receipt.stats.wall_ms.mean.unwrap(), 102.3);
        assert_close(receipt.stats.wall_ms.stddev.unwrap(), 1.5);
        assert_close(receipt.stats.cpu_ms.as_ref().unwrap().mean.unwrap(), 3.0);
        assert_eq!(receipt.run.host.os, "unknown");
        assert_eq!(receipt.run.host.arch, "unknown");
    }

    #[test]
    fn parse_hyperfine_default_name() {
        let receipt = parse_hyperfine(HYPERFINE_JSON, None).unwrap();
        assert_eq!(receipt.bench.name, "sleep 0.1");
    }

    #[test]
    fn parse_hyperfine_sample_wall_ms_and_exit_codes() {
        let receipt = parse_hyperfine(HYPERFINE_JSON, None).unwrap();
        let wall_values: Vec<u64> = receipt.samples.iter().map(|s| s.wall_ms).collect();
        assert_eq!(wall_values, vec![100, 102, 102, 103, 106]);
        let exit_codes: Vec<i32> = receipt.samples.iter().map(|s| s.exit_code).collect();
        assert_eq!(exit_codes, vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn parse_hyperfine_multiple_results() {
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

    #[test]
    fn parse_hyperfine_preserves_nonzero_exit_codes() {
        let input = r#"{
            "results": [
                {
                    "command": "cmd /c exit 7",
                    "mean": 0.005,
                    "stddev": 0.001,
                    "median": 0.005,
                    "min": 0.004,
                    "max": 0.006,
                    "times": [0.004, 0.005, 0.006],
                    "exit_codes": [0, 7, 0]
                }
            ]
        }"#;
        let receipt = parse_hyperfine(input, None).unwrap();
        let exit_codes: Vec<i32> = receipt
            .samples
            .iter()
            .map(|sample| sample.exit_code)
            .collect();
        assert_eq!(exit_codes, vec![0, 7, 0]);
    }

    #[test]
    fn parse_hyperfine_rejects_empty_times() {
        let input = r#"{
            "results": [
                {
                    "command": "echo empty",
                    "mean": 0.005,
                    "stddev": 0.001,
                    "median": 0.005,
                    "min": 0.004,
                    "max": 0.006,
                    "times": []
                }
            ]
        }"#;
        let err = parse_hyperfine(input, None).unwrap_err();
        assert!(err.to_string().contains("non-empty raw times"));
    }

    #[test]
    fn parse_hyperfine_rejects_exit_code_length_mismatch() {
        let input = r#"{
            "results": [
                {
                    "command": "echo mismatch",
                    "mean": 0.005,
                    "stddev": 0.001,
                    "median": 0.005,
                    "min": 0.004,
                    "max": 0.006,
                    "times": [0.004, 0.005, 0.006],
                    "exit_codes": [0, 0]
                }
            ]
        }"#;
        let err = parse_hyperfine(input, None).unwrap_err();
        assert!(err.to_string().contains("exit_codes length"));
    }

    #[test]
    fn parse_hyperfine_rejects_invalid_timing_values() {
        let input = r#"{
            "results": [
                {
                    "command": "echo bad",
                    "mean": 0.005,
                    "stddev": 0.001,
                    "median": 0.005,
                    "min": 0.004,
                    "max": 0.006,
                    "times": [-0.004]
                }
            ]
        }"#;
        let err = parse_hyperfine(input, None).unwrap_err();
        assert!(err.to_string().contains("finite and non-negative"));
    }

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {actual} to be close to {expected}"
        );
    }
}
