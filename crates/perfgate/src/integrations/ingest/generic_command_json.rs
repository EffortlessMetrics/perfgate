//! Parser for user-shaped generic command JSON evidence.
//!
//! This adapter is intentionally conservative. It accepts JSON that already
//! names perfgate metrics, units, directions, and samples or summaries. It does
//! not perform arbitrary field mapping; that belongs to the later custom
//! JSON/CSV mapping slice.

use std::collections::BTreeMap;

use anyhow::{Context, bail};
use perfgate_types::{
    BenchMeta, Direction, F64Summary, HostInfo, Metric, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample,
    Stats, ToolInfo, U64Summary,
};
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct GenericCommandJson {
    #[serde(default)]
    source_kind: Option<String>,
    #[serde(default)]
    source_version: Option<String>,
    #[serde(default)]
    benchmark: Option<GenericBenchmark>,
    #[serde(default)]
    bench: Option<GenericBenchmark>,
    metrics: BTreeMap<String, GenericMetric>,
    #[serde(default)]
    host: Option<GenericHost>,
    #[serde(default)]
    non_inferences: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GenericBenchmark {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    command: Option<Vec<String>>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    repeat: Option<u32>,
    #[serde(default)]
    warmup: Option<u32>,
    #[serde(default)]
    work_units: Option<u64>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GenericHost {
    #[serde(default)]
    os: Option<String>,
    #[serde(default)]
    arch: Option<String>,
    #[serde(default)]
    cpu_count: Option<u32>,
    #[serde(default)]
    memory_bytes: Option<u64>,
    #[serde(default)]
    hostname_hash: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GenericMetric {
    #[serde(default)]
    unit: Option<String>,
    #[serde(default)]
    direction: Option<String>,
    #[serde(default)]
    samples: Option<Vec<GenericSample>>,
    #[serde(default)]
    summary: Option<GenericSummary>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum GenericSample {
    Number(f64),
    Object {
        #[serde(default)]
        value: Option<f64>,
        #[serde(default)]
        wall_ms: Option<f64>,
        #[serde(default)]
        exit_code: Option<i32>,
        #[serde(default)]
        warmup: Option<bool>,
        #[serde(default)]
        timed_out: Option<bool>,
    },
}

#[derive(Debug, Deserialize)]
struct GenericSummary {
    median: f64,
    min: f64,
    max: f64,
    #[serde(default)]
    mean: Option<f64>,
    #[serde(default)]
    stddev: Option<f64>,
    #[serde(default)]
    sample_count: Option<u32>,
}

/// Parse generic command JSON into a `RunReceipt`.
///
/// The input must include a `wall_ms` metric with explicit unit and direction.
/// Other known metrics may be included as additional summaries or sample series.
pub fn parse_generic_command_json(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let input: GenericCommandJson =
        serde_json::from_str(input).context("failed to parse generic command JSON")?;

    validate_source_kind(input.source_kind.as_deref())?;
    let _source_version = input.source_version.as_deref();
    let _source_non_inference_count = input.non_inferences.len();

    let bench = input.benchmark.as_ref().or(input.bench.as_ref());
    let bench_name = name
        .map(str::to_string)
        .or_else(|| bench.and_then(|bench| bench.name.clone()))
        .context("generic command JSON requires benchmark.name or --name")?;

    let wall_metric = input
        .metrics
        .get(Metric::WallMs.as_str())
        .context("generic command JSON requires a 'wall_ms' metric with samples or summary")?;
    validate_metric_mapping(Metric::WallMs, wall_metric)?;

    let wall_values = metric_sample_values(Metric::WallMs, wall_metric)?;
    let mut samples = wall_samples(wall_metric, &wall_values)?;
    let wall_stats = u64_summary_from_metric(Metric::WallMs, wall_metric, &wall_values)?;

    let mut stats = Stats {
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

    for (metric_name, metric_input) in &input.metrics {
        let Some(metric) = Metric::parse_key(metric_name) else {
            bail!(
                "unsupported metric '{}' in generic command JSON; use a known perfgate metric or wait for custom JSON/CSV mapping",
                metric_name
            );
        };
        if metric == Metric::WallMs {
            continue;
        }

        validate_metric_mapping(metric, metric_input)?;

        if metric == Metric::ThroughputPerS {
            let values = metric_sample_values(metric, metric_input)?;
            stats.throughput_per_s = Some(f64_summary_from_metric(metric_input, &values)?);
            continue;
        }

        let values = metric_sample_values(metric, metric_input)?;
        let summary = u64_summary_from_metric(metric, metric_input, &values)?;
        assign_u64_summary(&mut stats, metric, summary)?;
        apply_u64_sample_values(&mut samples, metric, &values)?;
    }

    let now = OffsetDateTime::now_utc();
    let timestamp = now
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    let repeat = bench
        .and_then(|bench| bench.repeat)
        .unwrap_or_else(|| inferred_repeat(&samples, wall_metric));

    Ok(RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate-ingest".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        run: RunMeta {
            id: Uuid::new_v4().to_string(),
            started_at: timestamp.clone(),
            ended_at: timestamp,
            host: host_info(input.host),
        },
        bench: BenchMeta {
            name: bench_name,
            cwd: bench.and_then(|bench| bench.cwd.clone()),
            command: bench
                .and_then(|bench| bench.command.clone())
                .unwrap_or_else(|| vec!["(ingested generic command JSON)".to_string()]),
            repeat,
            warmup: bench.and_then(|bench| bench.warmup).unwrap_or(0),
            work_units: bench.and_then(|bench| bench.work_units),
            timeout_ms: bench.and_then(|bench| bench.timeout_ms),
        },
        samples,
        stats,
    })
}

fn validate_source_kind(source_kind: Option<&str>) -> anyhow::Result<()> {
    let Some(source_kind) = source_kind else {
        return Ok(());
    };

    match normalize_label(source_kind).as_str() {
        "generic_command_json" => Ok(()),
        _ => bail!(
            "generic command JSON source_kind must be 'generic_command_json', got '{}'",
            source_kind
        ),
    }
}

fn validate_metric_mapping(metric: Metric, input: &GenericMetric) -> anyhow::Result<()> {
    let unit = input
        .unit
        .as_deref()
        .context("generic command JSON metric requires explicit unit")?;
    validate_unit(metric, unit)?;

    let direction = input
        .direction
        .as_deref()
        .context("generic command JSON metric requires explicit direction")?;
    let direction = parse_direction(direction).with_context(|| {
        format!(
            "generic command JSON metric '{}' has ambiguous direction; use lower_is_better or higher_is_better",
            metric.as_str()
        )
    })?;

    let expected = metric.default_direction();
    if direction != expected {
        bail!(
            "generic command JSON metric '{}' declares direction '{}' but perfgate expects '{}'; custom direction mapping requires a future adapter/schema",
            metric.as_str(),
            direction_label(direction),
            direction_label(expected)
        );
    }

    if input.samples.is_none() && input.summary.is_none() {
        bail!(
            "generic command JSON metric '{}' requires samples or summary",
            metric.as_str()
        );
    }

    Ok(())
}

fn validate_unit(metric: Metric, unit: &str) -> anyhow::Result<()> {
    let normalized = normalize_label(unit);
    let valid = match metric {
        Metric::WallMs | Metric::CpuMs => matches!(
            normalized.as_str(),
            "ms" | "millisecond" | "milliseconds" | "s" | "sec" | "second" | "seconds"
        ),
        Metric::MaxRssKb => matches!(
            normalized.as_str(),
            "kb" | "kib" | "kilobyte" | "kilobytes" | "bytes" | "byte" | "b"
        ),
        Metric::IoReadBytes | Metric::IoWriteBytes | Metric::BinaryBytes => {
            matches!(normalized.as_str(), "bytes" | "byte" | "b")
        }
        Metric::PageFaults | Metric::CtxSwitches | Metric::NetworkPackets => matches!(
            normalized.as_str(),
            "count" | "counts" | "events" | "event" | "packets" | "packet"
        ),
        Metric::EnergyUj => matches!(
            normalized.as_str(),
            "uj" | "microjoule" | "microjoules" | "micro_joule" | "micro_joules"
        ),
        Metric::ThroughputPerS => matches!(
            normalized.as_str(),
            "per_s"
                | "per_sec"
                | "per_second"
                | "ops_s"
                | "ops_sec"
                | "ops_per_s"
                | "ops_per_sec"
                | "operations_s"
                | "operations_sec"
                | "operations_per_s"
                | "operations_per_sec"
                | "requests_s"
                | "requests_sec"
                | "requests_per_s"
                | "requests_per_sec"
                | "rps"
                | "items_s"
                | "items_sec"
                | "items_per_s"
                | "items_per_sec"
        ),
    };

    if valid {
        Ok(())
    } else {
        bail!(
            "generic command JSON metric '{}' has unsupported or ambiguous unit '{}'",
            metric.as_str(),
            unit
        )
    }
}

fn metric_sample_values(metric: Metric, input: &GenericMetric) -> anyhow::Result<Vec<f64>> {
    let Some(samples) = &input.samples else {
        return Ok(Vec::new());
    };

    samples
        .iter()
        .map(|sample| {
            sample
                .value(metric)
                .and_then(|value| normalize_metric_value(metric, input.unit.as_deref(), value))
        })
        .collect()
}

fn wall_samples(input: &GenericMetric, values: &[f64]) -> anyhow::Result<Vec<Sample>> {
    let Some(raw_samples) = &input.samples else {
        return Ok(Vec::new());
    };

    raw_samples
        .iter()
        .zip(values)
        .map(|(raw, value)| {
            let GenericSampleMeta {
                exit_code,
                warmup,
                timed_out,
            } = raw.meta();
            Ok(Sample {
                wall_ms: f64_to_u64(*value, Metric::WallMs.as_str())?,
                exit_code,
                warmup,
                timed_out,
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

fn u64_summary_from_metric(
    metric: Metric,
    input: &GenericMetric,
    values: &[f64],
) -> anyhow::Result<U64Summary> {
    if let Some(summary) = &input.summary {
        return Ok(U64Summary {
            median: f64_to_u64(
                normalize_metric_value(metric, input.unit.as_deref(), summary.median)?,
                metric.as_str(),
            )?,
            min: f64_to_u64(
                normalize_metric_value(metric, input.unit.as_deref(), summary.min)?,
                metric.as_str(),
            )?,
            max: f64_to_u64(
                normalize_metric_value(metric, input.unit.as_deref(), summary.max)?,
                metric.as_str(),
            )?,
            mean: summary
                .mean
                .map(|value| normalize_metric_value(metric, input.unit.as_deref(), value))
                .transpose()?,
            stddev: summary
                .stddev
                .map(|value| normalize_metric_value(metric, input.unit.as_deref(), value))
                .transpose()?,
        });
    }

    let values: Vec<u64> = values
        .iter()
        .map(|value| f64_to_u64(*value, metric.as_str()))
        .collect::<anyhow::Result<_>>()?;
    Ok(super::compute_u64_summary(&values))
}

fn f64_summary_from_metric(input: &GenericMetric, values: &[f64]) -> anyhow::Result<F64Summary> {
    if let Some(summary) = &input.summary {
        return Ok(F64Summary {
            median: normalize_metric_value(
                Metric::ThroughputPerS,
                input.unit.as_deref(),
                summary.median,
            )?,
            min: normalize_metric_value(
                Metric::ThroughputPerS,
                input.unit.as_deref(),
                summary.min,
            )?,
            max: normalize_metric_value(
                Metric::ThroughputPerS,
                input.unit.as_deref(),
                summary.max,
            )?,
            mean: summary
                .mean
                .map(|value| {
                    normalize_metric_value(Metric::ThroughputPerS, input.unit.as_deref(), value)
                })
                .transpose()?,
            stddev: summary
                .stddev
                .map(|value| {
                    normalize_metric_value(Metric::ThroughputPerS, input.unit.as_deref(), value)
                })
                .transpose()?,
        });
    }

    if values.is_empty() {
        bail!("generic command JSON metric 'throughput_per_s' requires samples or summary");
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (*value - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64;

    Ok(F64Summary {
        median,
        min,
        max,
        mean: Some(mean),
        stddev: Some(variance.sqrt()),
    })
}

fn assign_u64_summary(
    stats: &mut Stats,
    metric: Metric,
    summary: U64Summary,
) -> anyhow::Result<()> {
    match metric {
        Metric::BinaryBytes => stats.binary_bytes = Some(summary),
        Metric::CpuMs => stats.cpu_ms = Some(summary),
        Metric::CtxSwitches => stats.ctx_switches = Some(summary),
        Metric::EnergyUj => stats.energy_uj = Some(summary),
        Metric::IoReadBytes => stats.io_read_bytes = Some(summary),
        Metric::IoWriteBytes => stats.io_write_bytes = Some(summary),
        Metric::MaxRssKb => stats.max_rss_kb = Some(summary),
        Metric::NetworkPackets => stats.network_packets = Some(summary),
        Metric::PageFaults => stats.page_faults = Some(summary),
        Metric::WallMs | Metric::ThroughputPerS => {
            bail!(
                "internal error: unsupported u64 summary metric {}",
                metric.as_str()
            )
        }
    }
    Ok(())
}

fn apply_u64_sample_values(
    samples: &mut [Sample],
    metric: Metric,
    values: &[f64],
) -> anyhow::Result<()> {
    if values.is_empty() {
        return Ok(());
    }
    if samples.is_empty() {
        return Ok(());
    }
    if samples.len() != values.len() {
        bail!(
            "generic command JSON metric '{}' has {} samples but wall_ms has {}; sample series must align",
            metric.as_str(),
            values.len(),
            samples.len()
        );
    }

    for (sample, value) in samples.iter_mut().zip(values) {
        let value = f64_to_u64(*value, metric.as_str())?;
        match metric {
            Metric::BinaryBytes => sample.binary_bytes = Some(value),
            Metric::CpuMs => sample.cpu_ms = Some(value),
            Metric::CtxSwitches => sample.ctx_switches = Some(value),
            Metric::EnergyUj => sample.energy_uj = Some(value),
            Metric::IoReadBytes => sample.io_read_bytes = Some(value),
            Metric::IoWriteBytes => sample.io_write_bytes = Some(value),
            Metric::MaxRssKb => sample.max_rss_kb = Some(value),
            Metric::NetworkPackets => sample.network_packets = Some(value),
            Metric::PageFaults => sample.page_faults = Some(value),
            Metric::WallMs | Metric::ThroughputPerS => {}
        }
    }

    Ok(())
}

fn normalize_metric_value(metric: Metric, unit: Option<&str>, value: f64) -> anyhow::Result<f64> {
    if !value.is_finite() || value < 0.0 {
        bail!(
            "generic command JSON metric '{}' value must be finite and non-negative",
            metric.as_str()
        );
    }

    let unit = unit.context("generic command JSON metric requires explicit unit")?;
    let normalized = normalize_label(unit);

    let value = match metric {
        Metric::WallMs | Metric::CpuMs => match normalized.as_str() {
            "s" | "sec" | "second" | "seconds" => value * 1000.0,
            _ => value,
        },
        Metric::MaxRssKb => match normalized.as_str() {
            "bytes" | "byte" | "b" => value / 1024.0,
            _ => value,
        },
        _ => value,
    };

    Ok(value)
}

fn f64_to_u64(value: f64, metric_name: &str) -> anyhow::Result<u64> {
    if !value.is_finite() || value < 0.0 {
        bail!(
            "generic command JSON metric '{}' value must be finite and non-negative",
            metric_name
        );
    }
    if value > u64::MAX as f64 {
        bail!(
            "generic command JSON metric '{}' value is too large for perfgate.run.v1",
            metric_name
        );
    }
    let rounded = value.round();
    if rounded == 0.0 && value > 0.0 {
        Ok(1)
    } else {
        Ok(rounded as u64)
    }
}

fn parse_direction(raw: &str) -> Option<Direction> {
    match normalize_label(raw).as_str() {
        "lower" | "lower_is_better" | "lower_better" => Some(Direction::Lower),
        "higher" | "higher_is_better" | "higher_better" => Some(Direction::Higher),
        _ => None,
    }
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Lower => "lower_is_better",
        Direction::Higher => "higher_is_better",
    }
}

fn normalize_label(raw: &str) -> String {
    raw.trim()
        .to_ascii_lowercase()
        .replace(['-', '/', ' '], "_")
}

fn host_info(host: Option<GenericHost>) -> HostInfo {
    match host {
        Some(host) => HostInfo {
            os: host.os.unwrap_or_else(|| "unknown".to_string()),
            arch: host.arch.unwrap_or_else(|| "unknown".to_string()),
            cpu_count: host.cpu_count,
            memory_bytes: host.memory_bytes,
            hostname_hash: host.hostname_hash,
        },
        None => HostInfo {
            os: "unknown".to_string(),
            arch: "unknown".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        },
    }
}

fn inferred_repeat(samples: &[Sample], wall_metric: &GenericMetric) -> u32 {
    if !samples.is_empty() {
        return samples.len() as u32;
    }
    wall_metric
        .summary
        .as_ref()
        .and_then(|summary| summary.sample_count)
        .unwrap_or(0)
}

struct GenericSampleMeta {
    exit_code: i32,
    warmup: bool,
    timed_out: bool,
}

impl GenericSample {
    fn value(&self, metric: Metric) -> anyhow::Result<f64> {
        match self {
            GenericSample::Number(value) => Ok(*value),
            GenericSample::Object { value, wall_ms, .. } => value.or(*wall_ms).with_context(|| {
                format!(
                    "generic command JSON metric '{}' sample object requires value",
                    metric.as_str()
                )
            }),
        }
    }

    fn meta(&self) -> GenericSampleMeta {
        match self {
            GenericSample::Number(_) => GenericSampleMeta {
                exit_code: 0,
                warmup: false,
                timed_out: false,
            },
            GenericSample::Object {
                exit_code,
                warmup,
                timed_out,
                ..
            } => GenericSampleMeta {
                exit_code: exit_code.unwrap_or(0),
                warmup: warmup.unwrap_or(false),
                timed_out: timed_out.unwrap_or(false),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_wall_samples_and_throughput_summary() {
        let receipt = parse_generic_command_json(
            r#"{
              "source_kind": "generic_command_json",
              "benchmark": {
                "name": "parser",
                "command": ["node", "bench.js"],
                "work_units": 10000
              },
              "host": {"os": "linux", "arch": "x86_64", "cpu_count": 8},
              "metrics": {
                "wall_ms": {
                  "unit": "ms",
                  "direction": "lower_is_better",
                  "samples": [101.0, 99.0, 105.0]
                },
                "throughput_per_s": {
                  "unit": "ops/s",
                  "direction": "higher_is_better",
                  "summary": {
                    "median": 9700.0,
                    "min": 9300.0,
                    "max": 10000.0,
                    "mean": 9666.7,
                    "stddev": 300.0
                  }
                }
              }
            }"#,
            None,
        )
        .unwrap();

        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "parser");
        assert_eq!(receipt.bench.command, vec!["node", "bench.js"]);
        assert_eq!(receipt.bench.work_units, Some(10000));
        assert_eq!(receipt.samples.len(), 3);
        assert_eq!(receipt.stats.wall_ms.median, 101);
        assert_eq!(
            receipt.stats.throughput_per_s.as_ref().unwrap().median,
            9700.0
        );
        assert_eq!(receipt.run.host.os, "linux");
        assert_eq!(receipt.run.host.arch, "x86_64");
    }

    #[test]
    fn parses_summary_only_wall_metric_with_unknown_host() {
        let receipt = parse_generic_command_json(
            r#"{
              "benchmark": {"name": "summary-only"},
              "metrics": {
                "wall_ms": {
                  "unit": "seconds",
                  "direction": "lower_is_better",
                  "summary": {
                    "median": 0.120,
                    "min": 0.100,
                    "max": 0.150,
                    "mean": 0.123,
                    "stddev": 0.010,
                    "sample_count": 15
                  }
                }
              }
            }"#,
            None,
        )
        .unwrap();

        assert!(receipt.samples.is_empty());
        assert_eq!(receipt.bench.repeat, 15);
        assert_eq!(receipt.stats.wall_ms.median, 120);
        assert_eq!(receipt.stats.wall_ms.mean, Some(123.0));
        assert_eq!(receipt.run.host.os, "unknown");
        assert_eq!(receipt.run.host.arch, "unknown");
    }

    #[test]
    fn rejects_missing_wall_metric() {
        let err = parse_generic_command_json(
            r#"{
              "benchmark": {"name": "no-wall"},
              "metrics": {
                "throughput_per_s": {
                  "unit": "ops/s",
                  "direction": "higher_is_better",
                  "samples": [1000.0, 1200.0]
                }
              }
            }"#,
            None,
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("requires a 'wall_ms' metric with samples or summary")
        );
    }

    #[test]
    fn rejects_missing_unit() {
        let err = parse_generic_command_json(
            r#"{
              "benchmark": {"name": "missing-unit"},
              "metrics": {
                "wall_ms": {
                  "direction": "lower_is_better",
                  "samples": [100.0]
                }
              }
            }"#,
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("metric requires explicit unit"));
    }

    #[test]
    fn rejects_missing_direction() {
        let err = parse_generic_command_json(
            r#"{
              "benchmark": {"name": "missing-direction"},
              "metrics": {
                "wall_ms": {
                  "unit": "ms",
                  "samples": [100.0]
                }
              }
            }"#,
            None,
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("metric requires explicit direction")
        );
    }

    #[test]
    fn rejects_direction_that_would_invert_metric_judgment() {
        let err = parse_generic_command_json(
            r#"{
              "benchmark": {"name": "bad-direction"},
              "metrics": {
                "throughput_per_s": {
                  "unit": "ops/s",
                  "direction": "lower_is_better",
                  "summary": {"median": 100.0, "min": 90.0, "max": 110.0}
                },
                "wall_ms": {
                  "unit": "ms",
                  "direction": "lower_is_better",
                  "samples": [100.0]
                }
              }
            }"#,
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("throughput_per_s"));
        assert!(err.to_string().contains("higher_is_better"));
    }

    #[test]
    fn accepts_common_throughput_per_second_unit_aliases() {
        let receipt = parse_generic_command_json(
            r#"{
              "benchmark": {"name": "throughput-alias"},
              "metrics": {
                "wall_ms": {
                  "unit": "ms",
                  "direction": "lower_is_better",
                  "samples": [100.0]
                },
                "throughput_per_s": {
                  "unit": "ops/sec",
                  "direction": "higher_is_better",
                  "samples": [42.0]
                }
              }
            }"#,
            None,
        )
        .unwrap();

        assert_eq!(
            receipt.stats.throughput_per_s.as_ref().unwrap().median,
            42.0
        );
    }
}
