//! Parser for pytest-benchmark JSON output.
//!
//! pytest-benchmark JSON reports timing values in seconds. When `stats.data`
//! is present, perfgate preserves those raw timing samples as `wall_ms`
//! samples. Without `stats.data`, the import is summary-only; perfgate does not
//! synthesize samples from min/max/mean.

use anyhow::{Context, bail};
use perfgate_types::{
    BenchMeta, F64Summary, HostInfo, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, Stats, ToolInfo,
    U64Summary,
};
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use super::compute_u64_summary;

#[derive(Debug, Deserialize)]
struct PytestOutput {
    #[serde(default)]
    machine_info: Option<MachineInfo>,
    benchmarks: Vec<PytestBenchmark>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MachineInfo {
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    machine: Option<String>,
    #[serde(default)]
    processor: Option<String>,
    #[serde(default)]
    python_implementation: Option<String>,
    #[serde(default)]
    python_version: Option<String>,
    #[serde(default)]
    cpu: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct PytestBenchmark {
    name: String,
    #[serde(default)]
    fullname: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    options: Option<PytestOptions>,
    stats: PytestStats,
}

#[derive(Debug, Deserialize)]
struct PytestOptions {
    #[serde(default)]
    warmup: Option<bool>,
    #[serde(default)]
    timer: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PytestStats {
    min: f64,
    max: f64,
    mean: f64,
    median: f64,
    stddev: f64,
    #[serde(default)]
    rounds: Option<u64>,
    #[serde(default)]
    iterations: Option<u64>,
    #[serde(default)]
    data: Option<Vec<f64>>,
    #[serde(default)]
    ops: Option<f64>,
}

/// Parse a pytest-benchmark JSON file into a `RunReceipt`.
///
/// If the JSON contains multiple benchmarks, only the first benchmark is used.
/// Use `--name` at the CLI layer to make that selection explicit in the
/// resulting receipt.
pub fn parse_pytest_benchmark(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let output: PytestOutput =
        serde_json::from_str(input).context("failed to parse pytest-benchmark JSON")?;

    let bench = output
        .benchmarks
        .first()
        .context("pytest-benchmark JSON contains no benchmarks")?;
    validate_stats(&bench.stats)?;

    let bench_name = name
        .map(str::to_string)
        .or_else(|| bench.fullname.clone())
        .unwrap_or_else(|| bench.name.clone());

    let samples = raw_samples(&bench.stats)?;
    let wall_summary = wall_summary(&bench.stats, &samples)?;
    let throughput = bench.stats.ops.map(throughput_summary).transpose()?;

    Ok(make_pytest_receipt(PytestReceiptInput {
        name: bench_name,
        group: bench.group.clone(),
        samples,
        stats: Stats {
            wall_ms: wall_summary,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: throughput,
        },
        host: host_info(output.machine_info.as_ref()),
        python_runtime: python_runtime(output.machine_info.as_ref()),
        repeat: repeat_count(&bench.stats),
        warmup: warmup_count(bench.options.as_ref()),
        source_version: output.version,
        timer: bench
            .options
            .as_ref()
            .and_then(|options| options.timer.clone()),
    }))
}

struct PytestReceiptInput {
    name: String,
    group: Option<String>,
    samples: Vec<Sample>,
    stats: Stats,
    host: HostInfo,
    python_runtime: Option<String>,
    repeat: u32,
    warmup: u32,
    source_version: Option<String>,
    timer: Option<String>,
}

fn validate_stats(stats: &PytestStats) -> anyhow::Result<()> {
    for (field, value) in [
        ("min", stats.min),
        ("max", stats.max),
        ("mean", stats.mean),
        ("median", stats.median),
        ("stddev", stats.stddev),
    ] {
        validate_seconds(value, field)?;
    }

    if stats.min > stats.max {
        bail!("pytest-benchmark stats min must be <= max");
    }

    if let Some(rounds) = stats.rounds
        && rounds == 0
    {
        bail!("pytest-benchmark stats rounds must be greater than zero when present");
    }

    if let Some(iterations) = stats.iterations
        && iterations == 0
    {
        bail!("pytest-benchmark stats iterations must be greater than zero when present");
    }

    if let Some(data) = &stats.data {
        if data.is_empty() {
            bail!("pytest-benchmark stats.data must be non-empty when present");
        }
        for (index, value) in data.iter().enumerate() {
            validate_seconds(*value, &format!("data[{}]", index))?;
        }
        if let Some(rounds) = stats.rounds
            && rounds as usize != data.len()
        {
            bail!(
                "pytest-benchmark stats.data length ({}) does not match rounds ({})",
                data.len(),
                rounds
            );
        }
    }

    if let Some(ops) = stats.ops {
        validate_non_negative_finite(ops, "ops")?;
    }

    Ok(())
}

fn raw_samples(stats: &PytestStats) -> anyhow::Result<Vec<Sample>> {
    let Some(data) = &stats.data else {
        return Ok(Vec::new());
    };

    data.iter()
        .map(|seconds| {
            let wall_ms = seconds_to_u64_ms(*seconds, "data")?;
            Ok(Sample {
                wall_ms,
                exit_code: 0,
                // pytest-benchmark reports measured data; warmup settings are
                // preserved on bench metadata instead of marking measured
                // samples as warmup samples.
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
            })
        })
        .collect()
}

fn wall_summary(stats: &PytestStats, samples: &[Sample]) -> anyhow::Result<U64Summary> {
    let sample_wall_values: Vec<u64> = samples.iter().map(|sample| sample.wall_ms).collect();
    let mut summary = if sample_wall_values.is_empty() {
        U64Summary {
            median: seconds_to_u64_ms(stats.median, "median")?,
            min: seconds_to_u64_ms(stats.min, "min")?,
            max: seconds_to_u64_ms(stats.max, "max")?,
            mean: None,
            stddev: None,
        }
    } else {
        compute_u64_summary(&sample_wall_values)
    };

    summary.median = seconds_to_u64_ms(stats.median, "median")?;
    summary.min = seconds_to_u64_ms(stats.min, "min")?;
    summary.max = seconds_to_u64_ms(stats.max, "max")?;
    summary.mean = Some(seconds_to_f64_ms(stats.mean, "mean")?);
    summary.stddev = Some(seconds_to_f64_ms(stats.stddev, "stddev")?);
    Ok(summary)
}

fn throughput_summary(ops: f64) -> anyhow::Result<F64Summary> {
    validate_non_negative_finite(ops, "ops")?;
    Ok(F64Summary {
        median: ops,
        min: ops,
        max: ops,
        mean: Some(ops),
        stddev: None,
    })
}

fn make_pytest_receipt(input: PytestReceiptInput) -> RunReceipt {
    let now = OffsetDateTime::now_utc();
    let timestamp = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    let mut command = vec!["(ingested pytest-benchmark JSON)".to_string()];
    if let Some(version) = input.source_version {
        command.push(format!("pytest-benchmark {version}"));
    }
    if let Some(python_runtime) = input.python_runtime {
        command.push(format!("python={python_runtime}"));
    }
    if let Some(timer) = input.timer {
        command.push(format!("timer={timer}"));
    }
    if let Some(group) = input.group {
        command.push(format!("group={group}"));
    }

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
            host: input.host,
        },
        bench: BenchMeta {
            name: input.name,
            cwd: None,
            command,
            repeat: input.repeat,
            warmup: input.warmup,
            work_units: None,
            timeout_ms: None,
        },
        samples: input.samples,
        stats: input.stats,
    }
}

fn host_info(machine_info: Option<&MachineInfo>) -> HostInfo {
    let Some(machine_info) = machine_info else {
        return unknown_host();
    };

    HostInfo {
        os: machine_info
            .system
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        arch: machine_info
            .machine
            .clone()
            .or_else(|| machine_info.processor.clone())
            .unwrap_or_else(|| "unknown".to_string()),
        cpu_count: cpu_count(machine_info.cpu.as_ref()),
        memory_bytes: None,
        hostname_hash: None,
    }
}

fn unknown_host() -> HostInfo {
    HostInfo {
        os: "unknown".to_string(),
        arch: "unknown".to_string(),
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    }
}

fn cpu_count(cpu: Option<&Value>) -> Option<u32> {
    let cpu = cpu?;
    let value = cpu
        .get("count")
        .or_else(|| cpu.get("logical_count"))
        .or_else(|| cpu.get("cpu_count"))?;
    value
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .or_else(|| value.as_str().and_then(|value| value.parse::<u32>().ok()))
}

fn python_runtime(machine_info: Option<&MachineInfo>) -> Option<String> {
    let machine_info = machine_info?;
    match (
        machine_info.python_implementation.as_deref(),
        machine_info.python_version.as_deref(),
    ) {
        (Some(implementation), Some(version)) => Some(format!("{implementation} {version}")),
        (Some(implementation), None) => Some(implementation.to_string()),
        (None, Some(version)) => Some(version.to_string()),
        (None, None) => None,
    }
}

fn repeat_count(stats: &PytestStats) -> u32 {
    stats
        .data
        .as_ref()
        .map(|data| data.len() as u32)
        .or_else(|| stats.rounds.and_then(|rounds| u32::try_from(rounds).ok()))
        .unwrap_or(0)
}

fn warmup_count(options: Option<&PytestOptions>) -> u32 {
    if options.and_then(|options| options.warmup).unwrap_or(false) {
        1
    } else {
        0
    }
}

fn validate_seconds(value: f64, field: &str) -> anyhow::Result<()> {
    validate_non_negative_finite(value, field)
}

fn validate_non_negative_finite(value: f64, field: &str) -> anyhow::Result<()> {
    if !value.is_finite() || value < 0.0 {
        bail!("pytest-benchmark field '{field}' must be finite and non-negative");
    }
    Ok(())
}

fn seconds_to_u64_ms(seconds: f64, field: &str) -> anyhow::Result<u64> {
    let ms = seconds_to_f64_ms(seconds, field)?;
    if ms > u64::MAX as f64 {
        bail!("pytest-benchmark field '{field}' is too large for perfgate.run.v1");
    }
    if ms < 1.0 && ms > 0.0 {
        Ok(1)
    } else {
        Ok(ms.round() as u64)
    }
}

fn seconds_to_f64_ms(seconds: f64, field: &str) -> anyhow::Result<f64> {
    validate_seconds(seconds, field)?;
    Ok(seconds * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::RUN_SCHEMA_V1;

    const PYTEST_JSON_WITH_DATA: &str = r#"{
        "machine_info": {
            "node": "test-host",
            "processor": "x86_64",
            "machine": "x86_64",
            "python_implementation": "CPython",
            "python_version": "3.11.0",
            "python_compiler": "GCC 12.2.0",
            "release": "6.1.0",
            "system": "Linux",
            "cpu": {"count": 8}
        },
        "benchmarks": [
            {
                "group": "parser",
                "name": "test_sort",
                "fullname": "tests/test_perf.py::test_sort",
                "options": {
                    "timer": "perf_counter",
                    "warmup": false
                },
                "stats": {
                    "min": 0.0234,
                    "max": 0.0312,
                    "mean": 0.0256,
                    "stddev": 0.0021,
                    "rounds": 3,
                    "iterations": 1,
                    "median": 0.0250,
                    "ops": 39.0625,
                    "data": [0.0234, 0.0250, 0.0312]
                }
            }
        ],
        "datetime": "2024-01-15T10:30:00.000000",
        "version": "4.0.0"
    }"#;

    const PYTEST_JSON_SUMMARY_ONLY: &str = r#"{
        "benchmarks": [
            {
                "name": "test_summary",
                "stats": {
                    "min": 0.010,
                    "max": 0.020,
                    "mean": 0.015,
                    "stddev": 0.002,
                    "rounds": 1000,
                    "iterations": 1,
                    "median": 0.015,
                    "ops": 66.666666
                }
            }
        ]
    }"#;

    #[test]
    fn parses_raw_data_without_synthesizing_samples() {
        let receipt = parse_pytest_benchmark(PYTEST_JSON_WITH_DATA, None).unwrap();

        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "tests/test_perf.py::test_sort");
        assert_eq!(
            receipt.bench.command,
            vec![
                "(ingested pytest-benchmark JSON)".to_string(),
                "pytest-benchmark 4.0.0".to_string(),
                "python=CPython 3.11.0".to_string(),
                "timer=perf_counter".to_string(),
                "group=parser".to_string(),
            ]
        );
        assert_eq!(receipt.bench.repeat, 3);
        assert_eq!(receipt.bench.warmup, 0);
        assert_eq!(receipt.samples.len(), 3);
        assert_eq!(
            receipt
                .samples
                .iter()
                .map(|sample| sample.wall_ms)
                .collect::<Vec<_>>(),
            vec![23, 25, 31]
        );
        assert_eq!(receipt.stats.wall_ms.median, 25);
        assert_eq!(receipt.stats.wall_ms.min, 23);
        assert_eq!(receipt.stats.wall_ms.max, 31);
        assert_eq!(receipt.stats.wall_ms.mean, Some(25.6));
        assert_eq!(receipt.stats.wall_ms.stddev, Some(2.1));
        assert_eq!(
            receipt.stats.throughput_per_s.as_ref().unwrap().median,
            39.0625
        );
        assert_eq!(receipt.run.host.os, "Linux");
        assert_eq!(receipt.run.host.arch, "x86_64");
        assert_eq!(receipt.run.host.cpu_count, Some(8));
        assert_eq!(receipt.run.host.hostname_hash, None);
    }

    #[test]
    fn accepts_name_override() {
        let receipt = parse_pytest_benchmark(PYTEST_JSON_WITH_DATA, Some("parser-sort")).unwrap();

        assert_eq!(receipt.bench.name, "parser-sort");
    }

    #[test]
    fn summary_only_does_not_create_synthetic_samples() {
        let receipt = parse_pytest_benchmark(PYTEST_JSON_SUMMARY_ONLY, None).unwrap();

        assert_eq!(receipt.bench.name, "test_summary");
        assert_eq!(receipt.bench.repeat, 1000);
        assert!(receipt.samples.is_empty());
        assert_eq!(receipt.stats.wall_ms.median, 15);
        assert_eq!(receipt.stats.wall_ms.mean, Some(15.0));
        assert_eq!(
            receipt.stats.throughput_per_s.as_ref().unwrap().median,
            66.666666
        );
        assert_eq!(receipt.run.host.os, "unknown");
        assert_eq!(receipt.run.host.arch, "unknown");
    }

    #[test]
    fn rejects_data_rounds_mismatch() {
        let input = r#"{
            "benchmarks": [
                {
                    "name": "bad",
                    "stats": {
                        "min": 0.010,
                        "max": 0.020,
                        "mean": 0.015,
                        "stddev": 0.002,
                        "rounds": 3,
                        "iterations": 1,
                        "median": 0.015,
                        "data": [0.010, 0.020]
                    }
                }
            ]
        }"#;
        let err = parse_pytest_benchmark(input, None).unwrap_err();
        assert!(err.to_string().contains("data length"));
    }

    #[test]
    fn rejects_non_finite_or_negative_stats() {
        let input = r#"{
            "benchmarks": [
                {
                    "name": "bad",
                    "stats": {
                        "min": -0.010,
                        "max": 0.020,
                        "mean": 0.015,
                        "stddev": 0.002,
                        "rounds": 3,
                        "iterations": 1,
                        "median": 0.015
                    }
                }
            ]
        }"#;
        let err = parse_pytest_benchmark(input, None).unwrap_err();
        assert!(err.to_string().contains("finite and non-negative"));
    }

    #[test]
    fn rejects_empty_benchmarks() {
        let err = parse_pytest_benchmark(r#"{"benchmarks": []}"#, None).unwrap_err();
        assert!(err.to_string().contains("contains no benchmarks"));
    }

    #[test]
    fn rejects_invalid_json() {
        let err = parse_pytest_benchmark("not json", None).unwrap_err();
        assert!(err.to_string().contains("failed to parse"));
    }

    #[test]
    fn clamps_submillisecond_nonzero_samples_to_one_ms() {
        let input = r#"{
            "benchmarks": [
                {
                    "name": "test_fast",
                    "stats": {
                        "min": 0.0001,
                        "max": 0.0003,
                        "mean": 0.0002,
                        "stddev": 0.00005,
                        "rounds": 2,
                        "iterations": 100,
                        "median": 0.0002,
                        "data": [0.0001, 0.0003]
                    }
                }
            ]
        }"#;
        let receipt = parse_pytest_benchmark(input, None).unwrap();
        assert_eq!(
            receipt
                .samples
                .iter()
                .map(|sample| sample.wall_ms)
                .collect::<Vec<_>>(),
            vec![1, 1]
        );
        assert_eq!(receipt.stats.wall_ms.median, 1);
        assert_eq!(receipt.stats.wall_ms.mean, Some(0.2));
    }
}
