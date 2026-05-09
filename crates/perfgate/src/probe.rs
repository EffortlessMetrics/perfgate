//! Feature-gated helpers for writing probe JSONL.
//!
//! These helpers deliberately emit the same language-agnostic JSONL accepted by
//! `perfgate ingest probes`. They do not start background workers, require a
//! server, or install a global sink.

use perfgate_types::{ProbeMetricValue, ProbeScope};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::time::Instant;

/// Start building a probe JSONL event.
///
/// The returned event serializes to one JSONL line compatible with
/// `perfgate ingest probes`.
pub fn probe_event(name: impl Into<String>) -> ProbeEvent {
    ProbeEvent::new(name)
}

/// Start a wall-clock probe timer.
///
/// Call [`ProbeTimer::finish`] to turn it into a [`ProbeEvent`] with a
/// `wall_ms` metric. The timer does not write anywhere by itself.
pub fn probe_timer(name: impl Into<String>) -> ProbeTimer {
    ProbeTimer::start(name)
}

/// One probe observation ready to write as JSONL.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProbeEvent {
    name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    scope: Option<ProbeScope>,

    #[serde(skip_serializing_if = "Option::is_none")]
    iteration: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    ended_at: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<u64>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    metrics: BTreeMap<String, ProbeMetricValue>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    attributes: BTreeMap<String, String>,
}

impl ProbeEvent {
    /// Create an event for a named probe.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            parent: None,
            scope: None,
            iteration: None,
            started_at: None,
            ended_at: None,
            items: None,
            metrics: BTreeMap::new(),
            attributes: BTreeMap::new(),
        }
    }

    /// Set the parent probe name.
    pub fn parent(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Set the probe scope.
    pub fn scope(mut self, scope: ProbeScope) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Set the iteration number for repeated probe observations.
    pub fn iteration(mut self, iteration: u32) -> Self {
        self.iteration = Some(iteration);
        self
    }

    /// Set the start timestamp.
    ///
    /// Use RFC 3339 strings when this should round-trip as receipt metadata.
    pub fn started_at(mut self, started_at: impl Into<String>) -> Self {
        self.started_at = Some(started_at.into());
        self
    }

    /// Set the end timestamp.
    ///
    /// Use RFC 3339 strings when this should round-trip as receipt metadata.
    pub fn ended_at(mut self, ended_at: impl Into<String>) -> Self {
        self.ended_at = Some(ended_at.into());
        self
    }

    /// Set the number of work items represented by this observation.
    pub fn items(mut self, items: u64) -> Self {
        self.items = Some(items);
        self
    }

    /// Add a metric with a unit.
    pub fn metric(mut self, name: impl Into<String>, value: f64, unit: impl Into<String>) -> Self {
        self.metrics.insert(
            name.into(),
            ProbeMetricValue {
                value,
                unit: Some(unit.into()),
                statistic: None,
            },
        );
        self
    }

    /// Add a unitless metric.
    pub fn metric_unitless(mut self, name: impl Into<String>, value: f64) -> Self {
        self.metrics.insert(
            name.into(),
            ProbeMetricValue {
                value,
                unit: None,
                statistic: None,
            },
        );
        self
    }

    /// Add a metric with a unit and statistic label.
    pub fn metric_with_statistic(
        mut self,
        name: impl Into<String>,
        value: f64,
        unit: impl Into<String>,
        statistic: impl Into<String>,
    ) -> Self {
        self.metrics.insert(
            name.into(),
            ProbeMetricValue {
                value,
                unit: Some(unit.into()),
                statistic: Some(statistic.into()),
            },
        );
        self
    }

    /// Add an attribute.
    pub fn attribute(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(name.into(), value.into());
        self
    }

    /// Serialize the event to a single JSONL line.
    pub fn to_json_line(&self) -> serde_json::Result<String> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }

    /// Write the event as one JSONL line.
    pub fn write_jsonl<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        serde_json::to_writer(&mut *writer, self).map_err(io::Error::other)?;
        writer.write_all(b"\n")
    }
}

/// A simple explicit JSONL writer for probe events.
#[derive(Debug)]
pub struct ProbeJsonlWriter<W> {
    inner: W,
}

impl ProbeJsonlWriter<File> {
    /// Create or truncate a probe JSONL file.
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        Ok(Self::new(file))
    }

    /// Open a probe JSONL file for appending.
    pub fn append(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self::new(file))
    }
}

impl<W: Write> ProbeJsonlWriter<W> {
    /// Wrap an existing writer.
    pub fn new(inner: W) -> Self {
        Self { inner }
    }

    /// Write one event.
    pub fn record(&mut self, event: &ProbeEvent) -> io::Result<()> {
        event.write_jsonl(&mut self.inner)
    }

    /// Flush the underlying writer.
    pub fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }

    /// Return the wrapped writer.
    pub fn into_inner(self) -> W {
        self.inner
    }
}

/// Wall-clock helper that produces a probe event on demand.
#[derive(Debug)]
pub struct ProbeTimer {
    event: ProbeEvent,
    start: Instant,
}

impl ProbeTimer {
    /// Start timing a named probe.
    pub fn start(name: impl Into<String>) -> Self {
        Self {
            event: ProbeEvent::new(name),
            start: Instant::now(),
        }
    }

    /// Set the parent probe name.
    pub fn parent(mut self, parent: impl Into<String>) -> Self {
        self.event = self.event.parent(parent);
        self
    }

    /// Set the probe scope.
    pub fn scope(mut self, scope: ProbeScope) -> Self {
        self.event = self.event.scope(scope);
        self
    }

    /// Set the iteration number.
    pub fn iteration(mut self, iteration: u32) -> Self {
        self.event = self.event.iteration(iteration);
        self
    }

    /// Set the number of work items represented by this observation.
    pub fn items(mut self, items: u64) -> Self {
        self.event = self.event.items(items);
        self
    }

    /// Add an attribute.
    pub fn attribute(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.event = self.event.attribute(name, value);
        self
    }

    /// Finish timing and return an event with a `wall_ms` metric.
    pub fn finish(self) -> ProbeEvent {
        self.event
            .metric("wall_ms", self.start.elapsed().as_secs_f64() * 1000.0, "ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::ingest::{ProbeIngestRequest, ingest_probes_jsonl};

    #[test]
    fn probe_event_jsonl_is_ingestible() {
        let line = probe_event("parser.tokenize")
            .parent("parser.total")
            .scope(ProbeScope::Local)
            .iteration(2)
            .items(10_000)
            .metric("wall_ms", 12.4, "ms")
            .metric("alloc_bytes", 184_320.0, "bytes")
            .attribute("phase", "tokenize")
            .to_json_line()
            .expect("serialize probe event");

        let receipt = ingest_probes_jsonl(&ProbeIngestRequest {
            input: line,
            bench: Some("parser".to_string()),
            scenario: Some("large_file_parse".to_string()),
        })
        .expect("ingest helper JSONL");

        assert_eq!(receipt.probes.len(), 1);
        let probe = &receipt.probes[0];
        assert_eq!(probe.name, "parser.tokenize");
        assert_eq!(probe.parent.as_deref(), Some("parser.total"));
        assert_eq!(probe.scope, Some(ProbeScope::Local));
        assert_eq!(probe.iteration, Some(2));
        assert_eq!(probe.items, Some(10_000));
        assert_eq!(probe.metrics["wall_ms"].unit.as_deref(), Some("ms"));
        assert_eq!(probe.metrics["alloc_bytes"].unit.as_deref(), Some("bytes"));
        assert_eq!(probe.attributes["phase"], "tokenize");
    }

    #[test]
    fn jsonl_writer_records_one_event_per_line() {
        let mut writer = ProbeJsonlWriter::new(Vec::new());
        writer
            .record(&probe_event("parser.tokenize").metric("wall_ms", 12.4, "ms"))
            .expect("write first event");
        writer
            .record(&probe_event("parser.ast_build").metric("wall_ms", 44.8, "ms"))
            .expect("write second event");

        let output = String::from_utf8(writer.into_inner()).expect("utf8 JSONL");
        let lines: Vec<_> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("parser.tokenize"));
        assert!(lines[1].contains("parser.ast_build"));
    }

    #[test]
    fn probe_timer_finishes_with_wall_ms_metric() {
        let event = probe_timer("parser.batch_loop")
            .scope(ProbeScope::Dominant)
            .items(10_000)
            .finish();

        let wall_ms = event.metrics["wall_ms"].value;
        assert!(wall_ms.is_finite());
        assert!(wall_ms >= 0.0);
        assert_eq!(event.metrics["wall_ms"].unit.as_deref(), Some("ms"));
    }
}
