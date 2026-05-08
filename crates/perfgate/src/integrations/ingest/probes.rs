use anyhow::Context;
use perfgate_types::{ProbeMetricValue, ProbeObservation, ProbeScope};
use serde::Deserialize;
use std::collections::BTreeMap;

use super::make_probe_receipt;

/// Request to ingest language-agnostic probe JSONL.
pub struct ProbeIngestRequest {
    /// Raw JSONL content.
    pub input: String,
    /// Optional benchmark name to attach as receipt metadata.
    pub bench: Option<String>,
    /// Optional scenario name to attach as receipt metadata.
    pub scenario: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawProbeEvent {
    #[serde(alias = "probe")]
    name: String,

    #[serde(default)]
    parent: Option<String>,

    #[serde(default)]
    scope: Option<ProbeScope>,

    #[serde(default)]
    iteration: Option<u32>,

    #[serde(default)]
    started_at: Option<String>,

    #[serde(default)]
    ended_at: Option<String>,

    #[serde(default)]
    items: Option<u64>,

    #[serde(default)]
    metrics: BTreeMap<String, ProbeMetricValue>,

    #[serde(default)]
    attributes: BTreeMap<String, String>,

    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

/// Ingest JSONL probe events into a `perfgate.probe.v1` receipt.
pub fn ingest_probes_jsonl(
    request: &ProbeIngestRequest,
) -> anyhow::Result<perfgate_types::ProbeReceipt> {
    let mut probes = Vec::new();

    for (index, line) in request.input.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let raw: RawProbeEvent = serde_json::from_str(trimmed)
            .with_context(|| format!("parse probe JSONL line {}", index + 1))?;
        probes.push(raw.into_observation());
    }

    if probes.is_empty() {
        anyhow::bail!("no probe events found in JSONL input");
    }

    Ok(make_probe_receipt(
        request.bench.as_deref(),
        request.scenario.clone(),
        probes,
    ))
}

impl RawProbeEvent {
    fn into_observation(mut self) -> ProbeObservation {
        let mut attributes = self.attributes;
        let mut metrics = self.metrics;

        for (key, value) in self.extra {
            if let Some(number) = value.as_f64() {
                metrics.insert(
                    key.clone(),
                    ProbeMetricValue {
                        value: number,
                        unit: infer_unit(&key).map(str::to_string),
                        statistic: None,
                    },
                );
            } else if let Some(text) = value.as_str() {
                attributes.insert(key, text.to_string());
            } else if value.is_boolean() || value.is_number() {
                attributes.insert(key, value.to_string());
            }
        }

        ProbeObservation {
            name: self.name,
            parent: self.parent.take(),
            scope: self.scope,
            iteration: self.iteration,
            started_at: self.started_at.take(),
            ended_at: self.ended_at.take(),
            items: self.items,
            metrics,
            attributes,
        }
    }
}

fn infer_unit(metric: &str) -> Option<&'static str> {
    match metric {
        name if name.ends_with("_ms") => Some("ms"),
        name if name.ends_with("_bytes") => Some("bytes"),
        name if name.ends_with("_kb") => Some("KB"),
        name if name.ends_with("_uj") => Some("uj"),
        name if name.ends_with("_per_s") => Some("/s"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_probe_jsonl_accepts_flat_metric_events() {
        let receipt = ingest_probes_jsonl(&ProbeIngestRequest {
            input: r#"{"probe":"parser.tokenize","wall_ms":12.4,"alloc_bytes":184320,"items":10000,"scope":"local"}"#.into(),
            bench: Some("parser".into()),
            scenario: Some("large_file_parse".into()),
        })
        .expect("ingest probe JSONL");

        assert_eq!(receipt.schema, perfgate_types::PROBE_SCHEMA_V1);
        assert_eq!(
            receipt.bench.as_ref().map(|bench| bench.name.as_str()),
            Some("parser")
        );
        assert_eq!(receipt.scenario.as_deref(), Some("large_file_parse"));
        assert_eq!(receipt.probes.len(), 1);
        assert_eq!(receipt.probes[0].name, "parser.tokenize");
        assert_eq!(
            receipt.probes[0].metrics["wall_ms"].unit.as_deref(),
            Some("ms")
        );
        assert_eq!(
            receipt.probes[0].metrics["alloc_bytes"].unit.as_deref(),
            Some("bytes")
        );
    }

    #[test]
    fn ingest_probe_jsonl_accepts_nested_metrics() {
        let receipt = ingest_probes_jsonl(&ProbeIngestRequest {
            input:
                r#"{"name":"parser.ast_build","metrics":{"wall_ms":{"value":44.8,"unit":"ms"}}}"#
                    .into(),
            bench: None,
            scenario: None,
        })
        .expect("ingest probe JSONL");

        assert_eq!(receipt.probes[0].metrics["wall_ms"].value, 44.8);
        assert!(receipt.bench.is_none());
    }

    #[test]
    fn ingest_probe_jsonl_rejects_empty_input() {
        let err = ingest_probes_jsonl(&ProbeIngestRequest {
            input: "\n\n".into(),
            bench: None,
            scenario: None,
        })
        .expect_err("empty JSONL should fail");

        assert!(err.to_string().contains("no probe events"));
    }
}
