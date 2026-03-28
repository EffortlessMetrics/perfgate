//! Parser for OpenTelemetry JSON trace exports.

use anyhow::Context;
use perfgate_types::{F64Summary, RunReceipt, Sample, Stats};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

use crate::{compute_u64_summary, make_receipt};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OTelExport {
    #[serde(default)]
    resource_spans: Vec<ResourceSpans>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResourceSpans {
    #[serde(default)]
    scope_spans: Vec<ScopeSpans>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScopeSpans {
    #[serde(default)]
    spans: Vec<OTelSpan>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OTelSpan {
    name: String,
    start_time_unix_nano: String,
    end_time_unix_nano: String,
}

pub fn parse_otel_json(
    input: &str,
    name: Option<&str>,
    include_spans: &[String],
    exclude_spans: &[String],
) -> anyhow::Result<RunReceipt> {
    let export: OTelExport = serde_json::from_str(input).context("failed to parse OTel JSON")?;

    let include: BTreeSet<&str> = include_spans.iter().map(String::as_str).collect();
    let exclude: BTreeSet<&str> = exclude_spans.iter().map(String::as_str).collect();

    let mut durations_by_span: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut all_wall_ms: Vec<u64> = Vec::new();

    for resource in export.resource_spans {
        for scope in resource.scope_spans {
            for span in scope.spans {
                if !include.is_empty() && !include.contains(span.name.as_str()) {
                    continue;
                }
                if exclude.contains(span.name.as_str()) {
                    continue;
                }

                let start = parse_nanos(&span.start_time_unix_nano).with_context(|| {
                    format!("invalid startTimeUnixNano for span '{}'", span.name)
                })?;
                let end = parse_nanos(&span.end_time_unix_nano)
                    .with_context(|| format!("invalid endTimeUnixNano for span '{}'", span.name))?;

                if end < start {
                    continue;
                }

                let duration_ms = (end - start) as f64 / 1_000_000.0;
                durations_by_span
                    .entry(span.name)
                    .or_default()
                    .push(duration_ms);
                all_wall_ms.push(duration_ms.max(0.0).round() as u64);
            }
        }
    }

    if !include.is_empty() {
        let missing: Vec<_> = include
            .iter()
            .copied()
            .filter(|name| !durations_by_span.contains_key(*name))
            .collect();
        if !missing.is_empty() {
            anyhow::bail!(
                "requested span(s) not found in trace: {}",
                missing.join(", ")
            );
        }
    }

    if all_wall_ms.is_empty() {
        anyhow::bail!("no spans matched filters in OTel JSON export");
    }

    let bench_name = name.unwrap_or("otel-trace");

    let samples = all_wall_ms
        .iter()
        .map(|&wall_ms| Sample {
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
        })
        .collect::<Vec<_>>();

    let mut custom_metrics = BTreeMap::new();
    for (span_name, durations) in durations_by_span {
        let key = format!("span.{}.wall_ms", sanitize_metric_key(&span_name));
        custom_metrics.insert(key, compute_f64_summary(&durations));
    }

    let stats = Stats {
        wall_ms: compute_u64_summary(&all_wall_ms),
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
        custom_metrics,
    };

    Ok(make_receipt(bench_name, samples, stats))
}

fn parse_nanos(v: &str) -> anyhow::Result<u64> {
    v.parse::<u64>()
        .with_context(|| format!("failed to parse '{}' as u64 nanoseconds", v))
}

fn compute_f64_summary(values: &[f64]) -> F64Summary {
    if values.is_empty() {
        return F64Summary::new(0.0, 0.0, 0.0);
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
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;

    F64Summary {
        median,
        min,
        max,
        mean: Some(mean),
        stddev: Some(variance.sqrt()),
    }
}

fn sanitize_metric_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OTEL_JSON: &str = r#"{
      "resourceSpans": [{
        "scopeSpans": [{
          "spans": [
            {
              "name": "ast_parsing",
              "startTimeUnixNano": "1000000000",
              "endTimeUnixNano": "1040000000"
            },
            {
              "name": "ast_parsing",
              "startTimeUnixNano": "1100000000",
              "endTimeUnixNano": "1165000000"
            },
            {
              "name": "resolve_imports",
              "startTimeUnixNano": "2000000000",
              "endTimeUnixNano": "2070000000"
            }
          ]
        }]
      }]
    }"#;

    #[test]
    fn parse_otel_builds_custom_metrics() {
        let receipt = parse_otel_json(OTEL_JSON, Some("lsp"), &[], &[]).unwrap();
        assert_eq!(receipt.bench.name, "lsp");
        assert!(
            receipt
                .stats
                .custom_metrics
                .contains_key("span.ast_parsing.wall_ms")
        );
        assert!(
            receipt
                .stats
                .custom_metrics
                .contains_key("span.resolve_imports.wall_ms")
        );
    }

    #[test]
    fn parse_otel_include_span_missing_errors() {
        let err = parse_otel_json(OTEL_JSON, None, &["missing_span".to_string()], &[]).unwrap_err();
        assert!(
            err.to_string()
                .contains("requested span(s) not found in trace")
        );
    }

    #[test]
    fn parse_otel_include_and_exclude_filters() {
        let receipt = parse_otel_json(
            OTEL_JSON,
            None,
            &["ast_parsing".to_string()],
            &["resolve_imports".to_string()],
        )
        .unwrap();
        assert_eq!(receipt.stats.custom_metrics.len(), 1);
        assert!(
            receipt
                .stats
                .custom_metrics
                .contains_key("span.ast_parsing.wall_ms")
        );
    }
}
