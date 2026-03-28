use crate::{compute_u64_summary, make_receipt};
use anyhow::Context;
use perfgate_types::{Sample, Stats};
use regex::Regex;
use serde_json::Value;

/// Parse OpenTelemetry JSON trace export into a RunReceipt.
///
/// Supported inputs:
/// - top-level object with `resourceSpans`
/// - top-level array of trace/resource span payloads
///
/// Duration is computed from `endTimeUnixNano - startTimeUnixNano` and converted to ms.
pub fn parse_otel(
    input: &str,
    name_override: Option<&str>,
    span_name: Option<&str>,
    include: Option<&str>,
    exclude: Option<&str>,
) -> anyhow::Result<perfgate_types::RunReceipt> {
    let json: Value = serde_json::from_str(input).context("invalid OTel JSON")?;

    let include_regex = include
        .map(Regex::new)
        .transpose()
        .context("invalid --include regex")?;
    let exclude_regex = exclude
        .map(Regex::new)
        .transpose()
        .context("invalid --exclude regex")?;

    let mut durations_ms: Vec<u64> = Vec::new();
    let mut matched_names: Vec<String> = Vec::new();

    collect_durations(
        &json,
        span_name,
        include_regex.as_ref(),
        exclude_regex.as_ref(),
        &mut durations_ms,
        &mut matched_names,
    );

    if durations_ms.is_empty() {
        let target = span_name.unwrap_or("<any span>");
        anyhow::bail!(
            "no OTel spans matched filter (span_name={target}, include={include:?}, exclude={exclude:?})"
        );
    }

    let bench_name = name_override
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("otel.{}", span_name.unwrap_or("all_spans")));

    let samples = durations_ms
        .iter()
        .map(|wall_ms| Sample {
            wall_ms: *wall_ms,
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
        })
        .collect::<Vec<_>>();

    let stats = Stats {
        wall_ms: compute_u64_summary(&durations_ms),
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

    Ok(make_receipt(&bench_name, samples, stats))
}

fn collect_durations(
    node: &Value,
    span_name: Option<&str>,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
    out_durations_ms: &mut Vec<u64>,
    out_names: &mut Vec<String>,
) {
    match node {
        Value::Array(items) => {
            for item in items {
                collect_durations(
                    item,
                    span_name,
                    include,
                    exclude,
                    out_durations_ms,
                    out_names,
                );
            }
        }
        Value::Object(map) => {
            if let Some(Value::Array(spans)) = map.get("spans") {
                for span in spans {
                    if let Some((name, duration_ms)) =
                        parse_span_duration(span, span_name, include, exclude)
                    {
                        out_names.push(name);
                        out_durations_ms.push(duration_ms);
                    }
                }
            }

            for value in map.values() {
                collect_durations(
                    value,
                    span_name,
                    include,
                    exclude,
                    out_durations_ms,
                    out_names,
                );
            }
        }
        _ => {}
    }
}

fn parse_span_duration(
    span: &Value,
    span_name: Option<&str>,
    include: Option<&Regex>,
    exclude: Option<&Regex>,
) -> Option<(String, u64)> {
    let obj = span.as_object()?;
    let name = obj.get("name")?.as_str()?.to_string();

    if let Some(target) = span_name
        && name != target
    {
        return None;
    }

    if let Some(include) = include
        && !include.is_match(&name)
    {
        return None;
    }

    if let Some(exclude) = exclude
        && exclude.is_match(&name)
    {
        return None;
    }

    let start_ns = parse_ns(obj.get("startTimeUnixNano")?)?;
    let end_ns = parse_ns(obj.get("endTimeUnixNano")?)?;
    if end_ns < start_ns {
        return None;
    }
    let dur_ns = end_ns - start_ns;
    let duration_ms = (dur_ns / 1_000_000) as u64;
    Some((name, duration_ms))
}

fn parse_ns(value: &Value) -> Option<u128> {
    match value {
        Value::String(s) => s.parse::<u128>().ok(),
        Value::Number(n) => n.as_u64().map(u128::from),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_otel;

    #[test]
    fn parses_otel_json_and_extracts_span_durations() {
        let input = r#"
        {
          "resourceSpans": [{
            "scopeSpans": [{
              "spans": [
                { "name": "ast_parsing", "startTimeUnixNano": "1000000000", "endTimeUnixNano": "1150000000" },
                { "name": "ast_parsing", "startTimeUnixNano": "2000000000", "endTimeUnixNano": "2300000000" },
                { "name": "resolve_imports", "startTimeUnixNano": "3000000000", "endTimeUnixNano": "3200000000" }
              ]
            }]
          }]
        }
        "#;

        let receipt = parse_otel(input, None, Some("ast_parsing"), None, None).expect("parse");
        assert_eq!(receipt.bench.name, "otel.ast_parsing");
        assert_eq!(receipt.samples.len(), 2);
        assert_eq!(receipt.stats.wall_ms.median, 225);
        assert_eq!(receipt.stats.wall_ms.min, 150);
        assert_eq!(receipt.stats.wall_ms.max, 300);
    }

    #[test]
    fn errors_when_span_filter_matches_nothing() {
        let input = r#"
        {
          "resourceSpans": [{
            "scopeSpans": [{
              "spans": [
                { "name": "resolve_imports", "startTimeUnixNano": "1000000000", "endTimeUnixNano": "1200000000" }
              ]
            }]
          }]
        }
        "#;

        let err = parse_otel(input, None, Some("ast_parsing"), None, None).unwrap_err();
        assert!(err.to_string().contains("no OTel spans matched filter"));
    }
}
