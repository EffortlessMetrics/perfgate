//! Parser for Criterion benchmark output.
//!
//! Prefer `cargo criterion --message-format=json` benchmark-complete messages
//! or Criterion `raw.csv` files. `new/estimates.json` is accepted only as a
//! summary-only fallback because Criterion documents those JSON files as
//! private implementation details.

use anyhow::{Context, bail};
use perfgate_types::{
    BenchMeta, HostInfo, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, Stats, ToolInfo, U64Summary,
};
use serde::Deserialize;
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use super::compute_u64_summary;

#[derive(Debug, Deserialize)]
struct CargoCriterionMessage {
    reason: String,
    id: String,
    iteration_count: Vec<u64>,
    measured_values: Vec<f64>,
    unit: String,
    #[serde(default)]
    throughput: Vec<CargoCriterionThroughput>,
    #[serde(default)]
    mean: Option<CargoCriterionEstimate>,
    #[serde(default)]
    median: Option<CargoCriterionEstimate>,
}

#[derive(Debug, Deserialize)]
struct CargoCriterionThroughput {
    per_iteration: u64,
    #[serde(default)]
    unit: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CargoCriterionEstimate {
    estimate: f64,
    unit: String,
}

#[derive(Debug, Deserialize)]
struct CriterionEstimate {
    point_estimate: f64,
}

#[derive(Debug, Deserialize)]
struct CriterionEstimates {
    mean: CriterionEstimate,
    median: CriterionEstimate,
    std_dev: CriterionEstimate,
    #[serde(default)]
    slope: Option<CriterionEstimate>,
}

struct CriterionSamples {
    name: String,
    samples: Vec<Sample>,
    stats: U64Summary,
    work_units: Option<u64>,
}

/// Parse Criterion output into a `RunReceipt`.
///
/// Supported inputs:
///
/// - `cargo criterion --message-format=json` JSON or JSONL containing a
///   `benchmark-complete` message;
/// - Criterion `raw.csv`; and
/// - Criterion `new/estimates.json` as a summary-only fallback.
pub fn parse_criterion(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("Criterion input is empty");
    }

    if looks_like_raw_csv(trimmed) {
        return raw_csv_to_receipt(trimmed, name);
    }

    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => criterion_json_value_to_receipt(value, name),
        Err(_parse_error) if looks_like_jsonl(trimmed) => {
            cargo_criterion_jsonl_to_receipt(trimmed, name)
        }
        Err(parse_error) => {
            bail!(
                "failed to parse Criterion output; expected cargo-criterion JSON/JSONL benchmark-complete output, Criterion raw.csv, or Criterion new/estimates.json: {parse_error}"
            )
        }
    }
}

fn criterion_json_value_to_receipt(value: Value, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    if value.get("reason").and_then(Value::as_str) == Some("benchmark-complete") {
        let message: CargoCriterionMessage = serde_json::from_value(value)
            .context("failed to parse cargo-criterion benchmark-complete message")?;
        return cargo_criterion_message_to_receipt(message, name);
    }

    let estimates: CriterionEstimates = serde_json::from_value(value).with_context(|| {
        "unsupported Criterion JSON; expected reason=benchmark-complete or estimates.json with mean/median/std_dev"
    })?;
    estimates_to_receipt(estimates, name)
}

fn cargo_criterion_jsonl_to_receipt(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    for (index, line) in input.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("failed to parse Criterion JSONL line {}", index + 1))?;
        if value.get("reason").and_then(Value::as_str) == Some("benchmark-complete") {
            let message: CargoCriterionMessage = serde_json::from_value(value)
                .context("failed to parse cargo-criterion benchmark-complete message")?;
            return cargo_criterion_message_to_receipt(message, name);
        }
    }

    bail!("cargo-criterion JSONL contains no benchmark-complete message")
}

fn cargo_criterion_message_to_receipt(
    message: CargoCriterionMessage,
    name: Option<&str>,
) -> anyhow::Result<RunReceipt> {
    if message.reason != "benchmark-complete" {
        bail!(
            "cargo-criterion message reason must be benchmark-complete, got {}",
            message.reason
        );
    }
    if message.measured_values.is_empty() {
        bail!("cargo-criterion benchmark-complete message contains no measured_values");
    }
    if message.measured_values.len() != message.iteration_count.len() {
        bail!(
            "cargo-criterion measured_values length ({}) does not match iteration_count length ({})",
            message.measured_values.len(),
            message.iteration_count.len()
        );
    }

    let mut wall_values = Vec::new();
    let mut samples = Vec::new();
    for (index, measured_value) in message.measured_values.iter().enumerate() {
        let iterations = message.iteration_count[index];
        if iterations == 0 {
            bail!(
                "cargo-criterion iteration_count at sample {} must be greater than zero",
                index + 1
            );
        }
        let per_iteration = *measured_value / iterations as f64;
        let wall_ms = measurement_to_wall_ms(per_iteration, &message.unit, "measured_values")?;
        wall_values.push(wall_ms);
        samples.push(sample(wall_ms));
    }

    let mut stats = compute_u64_summary(&wall_values);
    if let Some(median) = &message.median {
        stats.median = estimate_to_wall_ms(median, "median")?;
    }
    if let Some(mean) = &message.mean {
        stats.mean = Some(estimate_to_wall_ms_f64(mean, "mean")?);
    }

    let work_units = message.throughput.first().map(|throughput| {
        let _unit = throughput.unit.as_deref();
        throughput.per_iteration
    });

    let samples = CriterionSamples {
        name: name.unwrap_or(&message.id).to_string(),
        samples,
        stats,
        work_units,
    };
    Ok(samples_to_receipt(samples))
}

fn raw_csv_to_receipt(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let mut lines = input.lines().filter(|line| !line.trim().is_empty());
    let header = lines.next().context("Criterion raw.csv is empty")?;
    validate_raw_csv_header(header)?;

    let mut identity: Option<String> = None;
    let mut wall_values = Vec::new();
    let mut samples = Vec::new();
    let mut work_units: Option<u64> = None;

    for (line_index, line) in lines.enumerate() {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.len() != 8 {
            bail!(
                "Criterion raw.csv line {} has {} fields; expected 8",
                line_index + 2,
                fields.len()
            );
        }

        let row_identity = raw_csv_identity(fields[0], fields[1], fields[2]);
        if let Some(existing) = &identity {
            if existing != &row_identity {
                bail!(
                    "Criterion raw.csv contains multiple benchmark identities ('{}' and '{}'); import one benchmark file at a time",
                    existing,
                    row_identity
                );
            }
        } else {
            identity = Some(row_identity);
        }

        if work_units.is_none() && !fields[3].is_empty() {
            let parsed = fields[3].parse::<u64>().with_context(|| {
                format!(
                    "Criterion raw.csv line {} throughput_num is not an integer",
                    line_index + 2
                )
            })?;
            work_units = Some(parsed);
        }

        let measured = fields[5].parse::<f64>().with_context(|| {
            format!(
                "Criterion raw.csv line {} sample_measured_value is not a number",
                line_index + 2
            )
        })?;
        let unit = fields[6];
        let iterations = fields[7].parse::<u64>().with_context(|| {
            format!(
                "Criterion raw.csv line {} iteration_count is not an integer",
                line_index + 2
            )
        })?;
        if iterations == 0 {
            bail!(
                "Criterion raw.csv line {} iteration_count must be greater than zero",
                line_index + 2
            );
        }

        let per_iteration = measured / iterations as f64;
        let wall_ms = measurement_to_wall_ms(per_iteration, unit, "sample_measured_value")?;
        wall_values.push(wall_ms);
        samples.push(sample(wall_ms));
    }

    if samples.is_empty() {
        bail!("Criterion raw.csv contains no sample rows");
    }

    let samples = CriterionSamples {
        name: name
            .map(str::to_string)
            .or(identity)
            .unwrap_or_else(|| "criterion-bench".to_string()),
        stats: compute_u64_summary(&wall_values),
        samples,
        work_units,
    };
    Ok(samples_to_receipt(samples))
}

fn estimates_to_receipt(
    estimates: CriterionEstimates,
    name: Option<&str>,
) -> anyhow::Result<RunReceipt> {
    let primary = estimates.slope.as_ref().unwrap_or(&estimates.mean);
    let mean_ms = ns_to_ms_f64(primary.point_estimate, "mean")?;
    let median_ms = ns_to_ms(estimates.median.point_estimate, "median")?;
    let stddev_ms = ns_to_ms_f64(estimates.std_dev.point_estimate, "std_dev")?;

    let stats = U64Summary {
        median: median_ms,
        min: median_ms,
        max: median_ms,
        mean: Some(mean_ms),
        stddev: Some(stddev_ms),
    };

    let samples = CriterionSamples {
        name: name.unwrap_or("criterion-bench").to_string(),
        samples: Vec::new(),
        stats,
        work_units: None,
    };
    Ok(samples_to_receipt(samples))
}

fn samples_to_receipt(input: CriterionSamples) -> RunReceipt {
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
            name: input.name,
            cwd: None,
            command: vec!["(ingested Criterion benchmark)".to_string()],
            repeat: input.samples.len() as u32,
            warmup: 0,
            work_units: input.work_units,
            timeout_ms: None,
        },
        samples: input.samples,
        stats: Stats {
            wall_ms: input.stats,
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
    }
}

fn validate_raw_csv_header(header: &str) -> anyhow::Result<()> {
    let fields: Vec<&str> = header.split(',').map(str::trim).collect();
    let expected = [
        "group",
        "function",
        "value",
        "throughput_num",
        "throughput_type",
        "sample_measured_value",
        "unit",
        "iteration_count",
    ];
    if fields != expected {
        bail!(
            "Criterion raw.csv header is not supported; expected {}",
            expected.join(",")
        );
    }
    Ok(())
}

fn raw_csv_identity(group: &str, function: &str, value: &str) -> String {
    let parts: Vec<&str> = [group, function, value]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        "criterion-bench".to_string()
    } else {
        parts.join("/")
    }
}

fn looks_like_raw_csv(input: &str) -> bool {
    input
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.trim_start_matches('\u{feff}')
                .starts_with("group,function,value,")
        })
        .unwrap_or(false)
}

fn looks_like_jsonl(input: &str) -> bool {
    let mut non_empty = input.lines().filter(|line| !line.trim().is_empty());
    matches!(
        (non_empty.next(), non_empty.next()),
        (Some(first), Some(_)) if first.trim_start().starts_with('{')
    )
}

fn estimate_to_wall_ms(estimate: &CargoCriterionEstimate, field: &str) -> anyhow::Result<u64> {
    measurement_to_wall_ms(estimate.estimate, &estimate.unit, field)
}

fn estimate_to_wall_ms_f64(estimate: &CargoCriterionEstimate, field: &str) -> anyhow::Result<f64> {
    measurement_to_wall_ms_f64(estimate.estimate, &estimate.unit, field)
}

fn measurement_to_wall_ms(value: f64, unit: &str, field: &str) -> anyhow::Result<u64> {
    let ms = measurement_to_wall_ms_f64(value, unit, field)?;
    f64_to_u64_ms(ms, field)
}

fn measurement_to_wall_ms_f64(value: f64, unit: &str, field: &str) -> anyhow::Result<f64> {
    if !value.is_finite() || value < 0.0 {
        bail!("Criterion field '{field}' must be finite and non-negative");
    }
    match normalize_unit(unit).as_str() {
        "ns" | "nanosecond" | "nanoseconds" => Ok(value / 1_000_000.0),
        "us" | "microsecond" | "microseconds" => Ok(value / 1_000.0),
        "ms" | "millisecond" | "milliseconds" => Ok(value),
        "s" | "sec" | "second" | "seconds" => Ok(value * 1000.0),
        _ => bail!(
            "Criterion unit '{}' is unsupported or ambiguous; expected ns, us, ms, or seconds wall-time units",
            unit
        ),
    }
}

fn ns_to_ms(ns: f64, field: &str) -> anyhow::Result<u64> {
    f64_to_u64_ms(ns_to_ms_f64(ns, field)?, field)
}

fn ns_to_ms_f64(ns: f64, field: &str) -> anyhow::Result<f64> {
    if !ns.is_finite() || ns < 0.0 {
        bail!("Criterion field '{field}' must be finite and non-negative");
    }
    Ok(ns / 1_000_000.0)
}

fn f64_to_u64_ms(ms: f64, field: &str) -> anyhow::Result<u64> {
    if !ms.is_finite() || ms < 0.0 {
        bail!("Criterion field '{field}' must be finite and non-negative");
    }
    if ms > u64::MAX as f64 {
        bail!("Criterion field '{field}' is too large for perfgate.run.v1");
    }
    if ms < 1.0 && ms > 0.0 {
        Ok(1)
    } else {
        Ok(ms.round() as u64)
    }
}

fn normalize_unit(unit: &str) -> String {
    unit.trim()
        .to_ascii_lowercase()
        .replace(['-', '/', ' '], "_")
}

fn sample(wall_ms: u64) -> Sample {
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

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::RUN_SCHEMA_V1;

    const CARGO_CRITERION_JSONL: &str = r#"{"reason":"warmup","id":"ignored"}
{"reason":"benchmark-complete","id":"parser/large","iteration_count":[10,20,30],"measured_values":[50000000.0,100000000.0,150000000.0],"unit":"ns","throughput":[{"per_iteration":5000,"unit":"elements"}],"mean":{"estimate":5000000.0,"lower_bound":4900000.0,"upper_bound":5100000.0,"unit":"ns"},"median":{"estimate":4950000.0,"lower_bound":4890000.0,"upper_bound":5010000.0,"unit":"ns"}}"#;

    const CRITERION_RAW_CSV: &str = r#"group,function,value,throughput_num,throughput_type,sample_measured_value,unit,iteration_count
Parser,large,,5000,elements,50000000,ns,10
Parser,large,,5000,elements,100000000,ns,20
Parser,large,,5000,elements,150000000,ns,30
"#;

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
    fn parse_cargo_criterion_jsonl_preserves_measured_samples() {
        let receipt = parse_criterion(CARGO_CRITERION_JSONL, None).unwrap();

        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "parser/large");
        assert_eq!(
            receipt.bench.command,
            vec!["(ingested Criterion benchmark)"]
        );
        assert_eq!(receipt.bench.repeat, 3);
        assert_eq!(receipt.bench.work_units, Some(5000));
        assert_eq!(receipt.samples.len(), 3);
        assert_eq!(
            receipt
                .samples
                .iter()
                .map(|sample| sample.wall_ms)
                .collect::<Vec<_>>(),
            vec![5, 5, 5]
        );
        assert_eq!(receipt.stats.wall_ms.median, 5);
        assert_eq!(receipt.stats.wall_ms.mean, Some(5.0));
        assert_eq!(receipt.run.host.os, "unknown");
        assert_eq!(receipt.run.host.arch, "unknown");
    }

    #[test]
    fn parse_cargo_criterion_json_object_accepts_name_override() {
        let input = CARGO_CRITERION_JSONL
            .lines()
            .nth(1)
            .expect("benchmark-complete line");
        let receipt = parse_criterion(input, Some("parser-renamed")).unwrap();

        assert_eq!(receipt.bench.name, "parser-renamed");
        assert_eq!(receipt.samples.len(), 3);
    }

    #[test]
    fn parse_criterion_raw_csv_preserves_sample_rows() {
        let receipt = parse_criterion(CRITERION_RAW_CSV, None).unwrap();

        assert_eq!(receipt.bench.name, "Parser/large");
        assert_eq!(receipt.bench.repeat, 3);
        assert_eq!(receipt.bench.work_units, Some(5000));
        assert_eq!(
            receipt
                .samples
                .iter()
                .map(|sample| sample.wall_ms)
                .collect::<Vec<_>>(),
            vec![5, 5, 5]
        );
        assert_eq!(receipt.run.host.os, "unknown");
    }

    #[test]
    fn parse_criterion_estimates_is_summary_only() {
        let receipt = parse_criterion(CRITERION_ESTIMATES, Some("my-bench")).unwrap();

        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "my-bench");
        assert_eq!(receipt.bench.repeat, 0);
        assert!(receipt.samples.is_empty());
        assert_eq!(receipt.stats.wall_ms.median, 5);
        assert_eq!(receipt.stats.wall_ms.mean, Some(5.0));
        assert_eq!(receipt.stats.wall_ms.stddev, Some(0.2));
        assert_eq!(receipt.run.host.os, "unknown");
    }

    #[test]
    fn parse_criterion_rejects_measured_value_iteration_mismatch() {
        let input = r#"{"reason":"benchmark-complete","id":"bad","iteration_count":[1],"measured_values":[1.0,2.0],"unit":"ns"}"#;
        let err = parse_criterion(input, None).unwrap_err();
        assert!(err.to_string().contains("measured_values length"));
    }

    #[test]
    fn parse_criterion_rejects_zero_iteration_count() {
        let input = r#"{"reason":"benchmark-complete","id":"bad","iteration_count":[0],"measured_values":[1.0],"unit":"ns"}"#;
        let err = parse_criterion(input, None).unwrap_err();
        assert!(err.to_string().contains("iteration_count"));
    }

    #[test]
    fn parse_criterion_rejects_unsupported_unit() {
        let input = r#"{"reason":"benchmark-complete","id":"bad","iteration_count":[1],"measured_values":[42.0],"unit":"cycles"}"#;
        let err = parse_criterion(input, None).unwrap_err();
        assert!(err.to_string().contains("unsupported or ambiguous"));
    }

    #[test]
    fn parse_criterion_rejects_raw_csv_with_multiple_identities() {
        let input = r#"group,function,value,throughput_num,throughput_type,sample_measured_value,unit,iteration_count
Parser,large,,,elements,50000000,ns,10
Parser,small,,,elements,100000000,ns,20
"#;
        let err = parse_criterion(input, None).unwrap_err();
        assert!(err.to_string().contains("multiple benchmark identities"));
    }

    #[test]
    fn parse_criterion_rejects_jsonl_without_benchmark_complete() {
        let input = r#"{"reason":"warmup","id":"parser"}
{"reason":"group-complete","group_name":"parser","benchmarks":["parser/large"]}"#;
        let err = parse_criterion(input, None).unwrap_err();
        assert!(err.to_string().contains("no benchmark-complete"));
    }
}
