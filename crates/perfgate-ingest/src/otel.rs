use std::collections::BTreeMap;

use anyhow::{Context, anyhow};
use perfgate_types::{F64Summary, RunReceipt, Sample, Stats};
use serde_json::Value;

use crate::{compute_u64_summary, make_receipt};

pub fn parse_otel(input: &str, name_override: Option<&str>) -> anyhow::Result<RunReceipt> {
    let parsed: Value = serde_json::from_str(input).context("parse OTel JSON")?;
    let spans = collect_spans(&parsed)?;
    if spans.is_empty() {
        return Err(anyhow!(
            "no OTel spans with start/end timestamps found in JSON"
        ));
    }

    let mut durations_by_name: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut all_wall_ms: Vec<u64> = Vec::with_capacity(spans.len());

    for (name, duration_ms) in spans {
        durations_by_name.entry(name).or_default().push(duration_ms);
        all_wall_ms.push(duration_ms.round().max(0.0) as u64);
    }

    let wall_summary = compute_u64_summary(&all_wall_ms);
    let mut receipt = make_receipt(
        name_override.unwrap_or("otel-trace"),
        build_samples(&all_wall_ms),
        Stats {
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
            throughput_per_s: None,
        },
    );

    receipt.span_metrics = durations_by_name
        .into_iter()
        .map(|(span_name, values)| {
            let key = format!("span.{span_name}.wall_ms");
            (key, summarize_f64(&values))
        })
        .collect();

    Ok(receipt)
}

fn collect_spans(v: &Value) -> anyhow::Result<Vec<(String, f64)>> {
    let resource_spans = v
        .get("resourceSpans")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing resourceSpans array in OTel JSON"))?;

    let mut spans = Vec::new();

    for resource in resource_spans {
        let Some(scope_spans) = resource
            .get("scopeSpans")
            .or_else(|| resource.get("instrumentationLibrarySpans"))
            .and_then(Value::as_array)
        else {
            continue;
        };

        for scope in scope_spans {
            if let Some(span_array) = scope.get("spans").and_then(Value::as_array) {
                for span in span_array {
                    let Some(name) = span.get("name").and_then(Value::as_str) else {
                        continue;
                    };

                    let start_ns = match extract_u128(span.get("startTimeUnixNano")) {
                        Some(v) => v,
                        None => continue,
                    };
                    let end_ns = match extract_u128(span.get("endTimeUnixNano")) {
                        Some(v) => v,
                        None => continue,
                    };
                    if end_ns < start_ns {
                        continue;
                    }
                    let duration_ms = (end_ns - start_ns) as f64 / 1_000_000.0;
                    spans.push((sanitize_span_name(name), duration_ms));
                }
            }
        }
    }

    Ok(spans)
}

fn extract_u128(v: Option<&Value>) -> Option<u128> {
    let value = v?;
    if let Some(s) = value.as_str() {
        return s.parse::<u128>().ok();
    }
    if let Some(n) = value.as_u64() {
        return Some(n as u128);
    }
    None
}

fn summarize_f64(values: &[f64]) -> F64Summary {
    if values.is_empty() {
        return F64Summary {
            median: 0.0,
            min: 0.0,
            max: 0.0,
            mean: None,
            stddev: None,
        };
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);

    let min = sorted[0];
    let max = sorted[sorted.len() - 1];
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;

    F64Summary {
        median,
        min,
        max,
        mean: Some(mean),
        stddev: Some(variance.sqrt()),
    }
}

fn build_samples(values: &[u64]) -> Vec<Sample> {
    values
        .iter()
        .map(|value| Sample {
            wall_ms: *value,
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
        .collect()
}

fn sanitize_span_name(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => c,
            _ => '_',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_otel_trace_into_span_metrics() {
        let json = r#"{
  "resourceSpans": [
    {
      "scopeSpans": [
        {
          "spans": [
            {"name": "ast_parsing", "startTimeUnixNano": "1000000000", "endTimeUnixNano": "1120000000"},
            {"name": "ast_parsing", "startTimeUnixNano": "2000000000", "endTimeUnixNano": "2140000000"},
            {"name": "resolve/imports", "startTimeUnixNano": "3000000000", "endTimeUnixNano": "3060000000"}
          ]
        }
      ]
    }
  ]
}"#;

        let receipt = parse_otel(json, Some("otel-test")).expect("parse otel");

        assert_eq!(receipt.bench.name, "otel-test");
        assert_eq!(receipt.samples.len(), 3);
        assert!(
            receipt
                .span_metrics
                .contains_key("span.ast_parsing.wall_ms")
        );
        assert!(
            receipt
                .span_metrics
                .contains_key("span.resolve_imports.wall_ms")
        );
        let parsing = receipt
            .span_metrics
            .get("span.ast_parsing.wall_ms")
            .expect("span summary");
        assert!((parsing.median - 130.0).abs() < 0.001);
    }

    #[test]
    fn errors_when_no_spans_found() {
        let json = r#"{"resourceSpans": []}"#;
        let err = parse_otel(json, None).expect_err("should fail");
        assert!(
            err.to_string()
                .contains("no OTel spans with start/end timestamps")
        );
    }
}
