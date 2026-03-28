use anyhow::{Context, anyhow};
use serde::Deserialize;

use crate::{compute_u64_summary, make_receipt};
use perfgate_types::{Sample, Stats};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OTelTrace {
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
    spans: Vec<Span>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Span {
    name: String,
    start_time_unix_nano: String,
    end_time_unix_nano: String,
}

pub fn parse_otel_json(
    input: &str,
    name: Option<&str>,
    include_spans: &[String],
    exclude_spans: &[String],
) -> anyhow::Result<perfgate_types::RunReceipt> {
    let trace: OTelTrace =
        serde_json::from_str(input).context("failed to parse OTel JSON trace export")?;

    let mut durations_ms = Vec::new();

    for resource in trace.resource_spans {
        for scope in resource.scope_spans {
            for span in scope.spans {
                if !include_spans.is_empty() && !include_spans.iter().any(|s| s == &span.name) {
                    continue;
                }
                if exclude_spans.iter().any(|s| s == &span.name) {
                    continue;
                }

                let start_ns: u128 = span.start_time_unix_nano.parse().with_context(|| {
                    format!("invalid start_time_unix_nano for span '{}'", span.name)
                })?;
                let end_ns: u128 = span.end_time_unix_nano.parse().with_context(|| {
                    format!("invalid end_time_unix_nano for span '{}'", span.name)
                })?;

                if end_ns < start_ns {
                    return Err(anyhow!(
                        "span '{}' has end_time_unix_nano earlier than start_time_unix_nano",
                        span.name
                    ));
                }

                let duration_ns = end_ns - start_ns;
                let duration_ms = (duration_ns / 1_000_000) as u64;
                durations_ms.push(duration_ms);
            }
        }
    }

    if durations_ms.is_empty() {
        let include_hint = if include_spans.is_empty() {
            "no spans found in the OTel export".to_string()
        } else {
            format!(
                "no spans matched include filter [{}]",
                include_spans.join(", ")
            )
        };
        return Err(anyhow!(
            "no span durations available for ingest: {}",
            include_hint
        ));
    }

    let samples: Vec<Sample> = durations_ms
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
        .collect();

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

    let bench_name = name
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "otel-spans".to_string());

    Ok(make_receipt(&bench_name, samples, stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACE_JSON: &str = r#"{
      "resourceSpans": [
        {
          "scopeSpans": [
            {
              "spans": [
                {"name": "ast_parsing", "startTimeUnixNano": "1000000000", "endTimeUnixNano": "1050000000"},
                {"name": "ast_parsing", "startTimeUnixNano": "2000000000", "endTimeUnixNano": "2070000000"},
                {"name": "resolve_imports", "startTimeUnixNano": "3000000000", "endTimeUnixNano": "3060000000"}
              ]
            }
          ]
        }
      ]
    }"#;

    #[test]
    fn ingest_otel_with_include_filter() {
        let receipt = parse_otel_json(
            TRACE_JSON,
            Some("otel-ast"),
            &["ast_parsing".to_string()],
            &[],
        )
        .expect("ingest OTel");

        assert_eq!(receipt.bench.name, "otel-ast");
        assert_eq!(receipt.samples.len(), 2);
        assert_eq!(receipt.stats.wall_ms.min, 50);
        assert_eq!(receipt.stats.wall_ms.max, 70);
    }

    #[test]
    fn ingest_otel_missing_span_returns_error() {
        let err = parse_otel_json(TRACE_JSON, None, &["does_not_exist".to_string()], &[])
            .expect_err("expected missing span error");

        assert!(
            err.to_string().contains("no spans matched include filter"),
            "unexpected error: {err}"
        );
    }
}
