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
#[cfg(feature = "probe-criterion")]
use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(any(feature = "probe-criterion", feature = "probe-tracing"))]
use std::sync::{Arc, Mutex};
#[cfg(any(feature = "probe-criterion", feature = "probe-tracing"))]
use std::time::Duration;
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

/// A Criterion measurement adapter that records each measurement as probe JSONL.
///
/// Enable the `probe-criterion` feature to use this adapter with
/// `criterion::Criterion::with_measurement`. It preserves Criterion's normal
/// wall-clock measurement behavior while writing one probe event for every
/// measurement sample that Criterion closes. The emitted JSONL is accepted by
/// `perfgate ingest probes`.
#[cfg(feature = "probe-criterion")]
#[derive(Debug)]
pub struct CriterionProbeMeasurement<W> {
    writer: Arc<Mutex<ProbeJsonlWriter<W>>>,
    event: ProbeEvent,
    next_iteration: Arc<AtomicU32>,
    last_error: Arc<Mutex<Option<String>>>,
}

#[cfg(feature = "probe-criterion")]
impl CriterionProbeMeasurement<File> {
    /// Create or truncate a probe JSONL file.
    pub fn create(name: impl Into<String>, path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(name, ProbeJsonlWriter::create(path)?))
    }

    /// Open a probe JSONL file for appending.
    pub fn append(name: impl Into<String>, path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(name, ProbeJsonlWriter::append(path)?))
    }
}

#[cfg(feature = "probe-criterion")]
impl<W: Write> CriterionProbeMeasurement<W> {
    /// Wrap an existing probe JSONL writer.
    pub fn new(name: impl Into<String>, writer: ProbeJsonlWriter<W>) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
            event: ProbeEvent::new(name),
            next_iteration: Arc::new(AtomicU32::new(0)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    /// Wrap an existing writer.
    pub fn from_writer(name: impl Into<String>, writer: W) -> Self {
        Self::new(name, ProbeJsonlWriter::new(writer))
    }

    /// Set the parent probe name on emitted events.
    pub fn parent(mut self, parent: impl Into<String>) -> Self {
        self.event = self.event.parent(parent);
        self
    }

    /// Set the probe scope on emitted events.
    pub fn scope(mut self, scope: ProbeScope) -> Self {
        self.event = self.event.scope(scope);
        self
    }

    /// Set the number of work items represented by each emitted event.
    pub fn items(mut self, items: u64) -> Self {
        self.event = self.event.items(items);
        self
    }

    /// Add an attribute to emitted events.
    pub fn attribute(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.event = self.event.attribute(name, value);
        self
    }

    /// Flush the wrapped JSONL writer.
    pub fn flush(&self) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| io::Error::other("probe criterion writer lock poisoned"))?;
        writer.flush()
    }

    /// Return the last write error observed by the measurement adapter, if any.
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|error| error.clone())
    }

    fn record_duration(&self, duration: Duration) {
        let iteration = self
            .next_iteration
            .fetch_add(1, Ordering::Relaxed)
            .saturating_add(1);
        let event = self.event.clone().iteration(iteration).metric(
            "wall_ms",
            duration.as_secs_f64() * 1000.0,
            "ms",
        );
        self.record_event(&event);
    }

    fn record_event(&self, event: &ProbeEvent) {
        match self.writer.lock() {
            Ok(mut writer) => {
                if let Err(error) = writer.record(event) {
                    self.set_last_error(error.to_string());
                }
            }
            Err(_) => self.set_last_error("probe criterion writer lock poisoned".to_string()),
        }
    }

    fn set_last_error(&self, message: String) {
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(message);
        }
    }
}

#[cfg(feature = "probe-criterion")]
impl<W> Clone for CriterionProbeMeasurement<W> {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
            event: self.event.clone(),
            next_iteration: Arc::clone(&self.next_iteration),
            last_error: Arc::clone(&self.last_error),
        }
    }
}

#[cfg(feature = "probe-criterion")]
impl<W: Write> criterion::measurement::Measurement for CriterionProbeMeasurement<W> {
    type Intermediate = Instant;
    type Value = Duration;

    fn start(&self) -> Self::Intermediate {
        Instant::now()
    }

    fn end(&self, started: Self::Intermediate) -> Self::Value {
        let duration = started.elapsed();
        self.record_duration(duration);
        duration
    }

    fn add(&self, v1: &Self::Value, v2: &Self::Value) -> Self::Value {
        *v1 + *v2
    }

    fn zero(&self) -> Self::Value {
        Duration::ZERO
    }

    fn to_f64(&self, value: &Self::Value) -> f64 {
        value.as_nanos() as f64
    }

    fn formatter(&self) -> &dyn criterion::measurement::ValueFormatter {
        static WALL_TIME: criterion::measurement::WallTime = criterion::measurement::WallTime;
        WALL_TIME.formatter()
    }
}

/// A `tracing-subscriber` layer that records closed spans as probe JSONL.
///
/// Enable the `probe-tracing` feature to use this adapter. It observes span
/// active time and writes one probe event per closed span. Span fields named
/// `scope`, `parent`, and `items` map to probe metadata. Numeric fields become
/// probe metrics; string and boolean fields become attributes.
#[cfg(feature = "probe-tracing")]
#[derive(Debug)]
pub struct TracingProbeLayer<W> {
    writer: Arc<Mutex<ProbeJsonlWriter<W>>>,
    last_error: Arc<Mutex<Option<String>>>,
}

#[cfg(feature = "probe-tracing")]
impl TracingProbeLayer<File> {
    /// Create or truncate a probe JSONL file.
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(ProbeJsonlWriter::create(path)?))
    }

    /// Open a probe JSONL file for appending.
    pub fn append(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self::new(ProbeJsonlWriter::append(path)?))
    }
}

#[cfg(feature = "probe-tracing")]
impl<W: Write> TracingProbeLayer<W> {
    /// Wrap an existing probe JSONL writer.
    pub fn new(writer: ProbeJsonlWriter<W>) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
            last_error: Arc::new(Mutex::new(None)),
        }
    }

    /// Wrap an existing writer.
    pub fn from_writer(writer: W) -> Self {
        Self::new(ProbeJsonlWriter::new(writer))
    }

    /// Flush the wrapped JSONL writer.
    pub fn flush(&self) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| io::Error::other("probe tracing writer lock poisoned"))?;
        writer.flush()
    }

    /// Return the last write error observed by the layer, if any.
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().ok().and_then(|error| error.clone())
    }

    fn record_event(&self, event: &ProbeEvent) {
        match self.writer.lock() {
            Ok(mut writer) => {
                if let Err(error) = writer.record(event) {
                    self.set_last_error(error.to_string());
                }
            }
            Err(_) => self.set_last_error("probe tracing writer lock poisoned".to_string()),
        }
    }

    fn set_last_error(&self, message: String) {
        if let Ok(mut last_error) = self.last_error.lock() {
            *last_error = Some(message);
        }
    }
}

#[cfg(feature = "probe-tracing")]
impl<W> Clone for TracingProbeLayer<W> {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
            last_error: Arc::clone(&self.last_error),
        }
    }
}

#[cfg(feature = "probe-tracing")]
impl<S, W> tracing_subscriber::Layer<S> for TracingProbeLayer<W>
where
    S: tracing::Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
    W: Write + Send + 'static,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else {
            return;
        };

        let mut fields = ProbeFieldVisitor::default();
        attrs.record(&mut fields);

        let metadata = attrs.metadata();
        let name = fields.name.unwrap_or_else(|| metadata.name().to_string());
        let parent = fields.parent.or_else(|| {
            span.parent()
                .map(|parent| parent.metadata().name().to_string())
        });

        span.extensions_mut().insert(TracingProbeState {
            event: ProbeEvent {
                name,
                parent,
                scope: fields.scope,
                iteration: fields.iteration,
                started_at: None,
                ended_at: None,
                items: fields.items,
                metrics: fields.metrics,
                attributes: fields.attributes,
            },
            active_since: None,
            active_duration: Duration::ZERO,
        });
    }

    fn on_record(
        &self,
        id: &tracing::Id,
        values: &tracing::span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        let Some(state) = extensions.get_mut::<TracingProbeState>() else {
            return;
        };

        let mut fields = ProbeFieldVisitor::default();
        values.record(&mut fields);
        state.event.merge_fields(fields);
    }

    fn on_enter(&self, id: &tracing::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        let Some(state) = extensions.get_mut::<TracingProbeState>() else {
            return;
        };
        if state.active_since.is_none() {
            state.active_since = Some(Instant::now());
        }
    }

    fn on_exit(&self, id: &tracing::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        let Some(state) = extensions.get_mut::<TracingProbeState>() else {
            return;
        };
        if let Some(started) = state.active_since.take() {
            state.active_duration += started.elapsed();
        }
    }

    fn on_close(&self, id: tracing::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        let Some(mut state) = extensions.remove::<TracingProbeState>() else {
            return;
        };
        if let Some(started) = state.active_since.take() {
            state.active_duration += started.elapsed();
        }

        state.event = state.event.metric(
            "wall_ms",
            state.active_duration.as_secs_f64() * 1000.0,
            "ms",
        );
        self.record_event(&state.event);
    }
}

#[cfg(feature = "probe-tracing")]
#[derive(Debug)]
struct TracingProbeState {
    event: ProbeEvent,
    active_since: Option<Instant>,
    active_duration: Duration,
}

#[cfg(feature = "probe-tracing")]
#[derive(Default)]
struct ProbeFieldVisitor {
    name: Option<String>,
    parent: Option<String>,
    scope: Option<ProbeScope>,
    iteration: Option<u32>,
    items: Option<u64>,
    metrics: BTreeMap<String, ProbeMetricValue>,
    attributes: BTreeMap<String, String>,
}

#[cfg(feature = "probe-tracing")]
impl ProbeEvent {
    fn merge_fields(&mut self, fields: ProbeFieldVisitor) {
        if let Some(name) = fields.name {
            self.name = name;
        }
        if fields.parent.is_some() {
            self.parent = fields.parent;
        }
        if fields.scope.is_some() {
            self.scope = fields.scope;
        }
        if fields.iteration.is_some() {
            self.iteration = fields.iteration;
        }
        if fields.items.is_some() {
            self.items = fields.items;
        }
        self.metrics.extend(fields.metrics);
        self.attributes.extend(fields.attributes);
    }
}

#[cfg(feature = "probe-tracing")]
impl tracing::field::Visit for ProbeFieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.record_text(field.name(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record_text(field.name(), value.to_string());
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.record_text(field.name(), value.to_string());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.record_number(field.name(), value as f64);
        self.record_u64_metadata(field.name(), value.try_into().ok());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.record_number(field.name(), value as f64);
        self.record_u64_metadata(field.name(), Some(value));
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.record_number(field.name(), value);
    }
}

#[cfg(feature = "probe-tracing")]
impl ProbeFieldVisitor {
    fn record_text(&mut self, name: &str, value: String) {
        match name {
            "probe" | "probe.name" | "perfgate.probe" | "perfgate.probe.name" => {
                self.name = Some(value);
            }
            "parent" | "probe.parent" | "perfgate.probe.parent" => {
                self.parent = Some(value);
            }
            "scope" | "probe.scope" | "perfgate.probe.scope" => {
                self.scope = parse_scope(&value);
                if self.scope.is_none() {
                    self.attributes.insert(name.to_string(), value);
                }
            }
            "items" | "probe.items" | "perfgate.probe.items" => {
                if let Ok(items) = value.parse() {
                    self.items = Some(items);
                } else {
                    self.attributes.insert(name.to_string(), value);
                }
            }
            "iteration" | "probe.iteration" | "perfgate.probe.iteration" => {
                if let Ok(iteration) = value.parse() {
                    self.iteration = Some(iteration);
                } else {
                    self.attributes.insert(name.to_string(), value);
                }
            }
            _ => {
                self.attributes.insert(name.to_string(), value);
            }
        }
    }

    fn record_number(&mut self, name: &str, value: f64) {
        if matches!(
            name,
            "items"
                | "probe.items"
                | "perfgate.probe.items"
                | "iteration"
                | "probe.iteration"
                | "perfgate.probe.iteration"
        ) {
            return;
        }

        let metric_name = name
            .strip_prefix("metric.")
            .or_else(|| name.strip_prefix("metrics."))
            .unwrap_or(name);
        self.metrics.insert(
            metric_name.to_string(),
            ProbeMetricValue {
                value,
                unit: infer_unit(metric_name).map(str::to_string),
                statistic: None,
            },
        );
    }

    fn record_u64_metadata(&mut self, name: &str, value: Option<u64>) {
        let Some(value) = value else {
            return;
        };
        match name {
            "items" | "probe.items" | "perfgate.probe.items" => {
                self.items = Some(value);
            }
            "iteration" | "probe.iteration" | "perfgate.probe.iteration" => {
                if let Ok(iteration) = value.try_into() {
                    self.iteration = Some(iteration);
                }
            }
            _ => {}
        }
    }
}

#[cfg(feature = "probe-tracing")]
fn parse_scope(value: &str) -> Option<ProbeScope> {
    match value {
        "local" => Some(ProbeScope::Local),
        "enclosing" => Some(ProbeScope::Enclosing),
        "dominant" => Some(ProbeScope::Dominant),
        "total" => Some(ProbeScope::Total),
        _ => None,
    }
}

#[cfg(feature = "probe-tracing")]
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
    use crate::app::render::render_tradeoff_markdown;
    use crate::app::{
        ProbeCompareRequest, ProbeCompareUseCase, ScenarioEvaluateInput, ScenarioEvaluateRequest,
        ScenarioUseCase, TradeoffEvaluateRequest, TradeoffUseCase,
    };
    use crate::integrations::ingest::{ProbeIngestRequest, ingest_probes_jsonl};
    use perfgate_types::{
        BenchMeta, COMPARE_SCHEMA_V1, CompareReceipt, CompareRef, ConfigFile, DecisionPolicyConfig,
        DefaultsConfig, Delta, Metric, MetricStatistic, MetricStatus, ScenarioConfigFile, ToolInfo,
        TradeoffAllowance, TradeoffDowngrade, TradeoffRequirement, TradeoffRule, Verdict,
        VerdictCounts, VerdictStatus,
    };
    use std::collections::BTreeMap;

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

    #[test]
    fn probe_helper_jsonl_drives_tradeoff_decision_evidence() {
        let baseline = ingest_probes_jsonl(&ProbeIngestRequest {
            input: helper_jsonl(&[
                ("parser.tokenize", ProbeScope::Local, 12.10, 184_320.0),
                (
                    "parser.batch_loop",
                    ProbeScope::Dominant,
                    100.00,
                    1_048_576.0,
                ),
            ]),
            bench: Some("parser".to_string()),
            scenario: Some("large_file_parse".to_string()),
        })
        .expect("ingest helper baseline JSONL");
        let current = ingest_probes_jsonl(&ProbeIngestRequest {
            input: helper_jsonl(&[
                ("parser.tokenize", ProbeScope::Local, 12.35, 184_960.0),
                (
                    "parser.batch_loop",
                    ProbeScope::Dominant,
                    89.60,
                    1_000_000.0,
                ),
            ]),
            bench: Some("parser".to_string()),
            scenario: Some("large_file_parse".to_string()),
        })
        .expect("ingest helper current JSONL");

        let probe_compare = ProbeCompareUseCase::compare(ProbeCompareRequest {
            baseline,
            current,
            baseline_ref: CompareRef {
                path: Some("artifacts/perfgate/parser/probes-baseline.json".to_string()),
                run_id: Some("probe-baseline".to_string()),
            },
            current_ref: CompareRef {
                path: Some("artifacts/perfgate/parser/probes-current.json".to_string()),
                run_id: Some("probe-current".to_string()),
            },
            tool: tool(),
        })
        .expect("compare helper probe receipts")
        .receipt;
        assert_eq!(probe_compare.verdict.status, VerdictStatus::Warn);
        assert!(
            probe_compare.probes.iter().any(
                |probe| probe.name == "parser.batch_loop" && probe.status == MetricStatus::Pass
            )
        );
        assert!(
            probe_compare
                .probes
                .iter()
                .any(|probe| probe.name == "parser.tokenize" && probe.status == MetricStatus::Warn)
        );

        let scenario = ScenarioUseCase::evaluate(ScenarioEvaluateRequest {
            config: ConfigFile {
                defaults: DefaultsConfig {
                    threshold: Some(0.20),
                    warn_factor: Some(0.50),
                    ..Default::default()
                },
                ..Default::default()
            },
            inputs: vec![ScenarioEvaluateInput {
                config: ScenarioConfigFile {
                    name: "large_file_parse".to_string(),
                    weight: 1.0,
                    bench: "parser".to_string(),
                    description: None,
                    compare: Some("artifacts/perfgate/parser/compare.json".to_string()),
                    probe_compare: Some("artifacts/perfgate/parser/probe-compare.json".to_string()),
                    probe_baseline: None,
                    probe_current: None,
                },
                compare_ref: CompareRef {
                    path: Some("artifacts/perfgate/parser/compare.json".to_string()),
                    run_id: Some("parser-current".to_string()),
                },
                compare: compare_receipt(),
                probe_compare_ref: Some(CompareRef {
                    path: Some("artifacts/perfgate/parser/probe-compare.json".to_string()),
                    run_id: Some(probe_compare.run.id.clone()),
                }),
                probe_compare: Some(probe_compare.clone()),
                probe_compare_warning: None,
            }],
            workload_name: None,
            tool: tool(),
        })
        .expect("evaluate scenario from probe evidence")
        .receipt;
        assert!(
            scenario.components[0]
                .probes
                .iter()
                .any(|probe| probe == "parser.batch_loop")
        );

        let tradeoff = TradeoffUseCase::evaluate(TradeoffEvaluateRequest {
            scenario,
            probe_compares: vec![probe_compare],
            rules: vec![TradeoffRule {
                name: "memory_for_probe_speed".to_string(),
                if_failed: Metric::MaxRssKb,
                require: vec![TradeoffRequirement {
                    metric: Metric::WallMs,
                    probe: Some("parser.batch_loop".to_string()),
                    min_improvement_ratio: 1.10,
                }],
                allow: vec![TradeoffAllowance {
                    metric: Metric::WallMs,
                    probe: "parser.tokenize".to_string(),
                    max_regression: 0.03,
                }],
                downgrade_to: TradeoffDowngrade::Warn,
            }],
            decision_policy: DecisionPolicyConfig::default(),
            tool: tool(),
        })
        .expect("evaluate probe-backed tradeoff")
        .receipt;

        assert!(tradeoff.decision.accepted_tradeoff);
        assert_eq!(tradeoff.decision.status, MetricStatus::Warn);
        assert_eq!(
            tradeoff.rules[0].requirements[0].probe.as_deref(),
            Some("parser.batch_loop")
        );
        assert_eq!(tradeoff.rules[0].allowances[0].probe, "parser.tokenize");

        let decision = render_tradeoff_markdown(&tradeoff);
        assert!(decision.contains("perfgate tradeoff: warn"));
        assert!(decision.contains("tradeoff 'memory_for_probe_speed' accepted"));
        assert!(decision.contains("Probe Evidence"));
        assert!(decision.contains("parser.batch_loop"));
        assert!(decision.contains("parser.tokenize"));
    }

    fn helper_jsonl(probes: &[(&str, ProbeScope, f64, f64)]) -> String {
        let mut writer = ProbeJsonlWriter::new(Vec::new());
        for (name, scope, wall_ms, alloc_bytes) in probes {
            writer
                .record(
                    &probe_event(*name)
                        .scope(*scope)
                        .items(10_000)
                        .metric("wall_ms", *wall_ms, "ms")
                        .metric("alloc_bytes", *alloc_bytes, "bytes"),
                )
                .expect("record helper probe event");
        }
        writer.flush().expect("flush helper probe JSONL");
        String::from_utf8(writer.into_inner()).expect("helper JSONL should be utf8")
    }

    fn compare_receipt() -> CompareReceipt {
        CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: tool(),
            bench: BenchMeta {
                name: "parser".to_string(),
                cwd: None,
                command: vec!["cargo".to_string(), "bench".to_string()],
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            baseline_ref: CompareRef {
                path: Some("baselines/parser.json".to_string()),
                run_id: Some("parser-baseline".to_string()),
            },
            current_ref: CompareRef {
                path: Some("artifacts/perfgate/parser/run.json".to_string()),
                run_id: Some("parser-current".to_string()),
            },
            budgets: BTreeMap::new(),
            deltas: BTreeMap::from([
                (Metric::WallMs, delta(100.0, 96.0, 0.0, MetricStatus::Pass)),
                (
                    Metric::MaxRssKb,
                    delta(100.0, 1200.0, 11.0, MetricStatus::Fail),
                ),
            ]),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 1,
                    skip: 0,
                },
                reasons: vec!["max_rss_kb_fail".to_string()],
            },
        }
    }

    fn delta(baseline: f64, current: f64, regression: f64, status: MetricStatus) -> Delta {
        Delta {
            baseline,
            current,
            ratio: current / baseline,
            pct: (current - baseline) / baseline,
            regression,
            cv: None,
            noise_threshold: None,
            statistic: MetricStatistic::Median,
            significance: None,
            status,
        }
    }

    fn tool() -> ToolInfo {
        ToolInfo {
            name: "perfgate".to_string(),
            version: "0.17.0".to_string(),
        }
    }

    #[cfg(feature = "probe-criterion")]
    #[test]
    fn criterion_measurement_records_samples_as_probe_jsonl() {
        use criterion::measurement::Measurement;
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct SharedWriter(Arc<Mutex<Vec<u8>>>);

        impl Write for SharedWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0
                    .lock()
                    .map_err(|_| io::Error::other("buffer lock poisoned"))?
                    .write(buf)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let output = Arc::new(Mutex::new(Vec::new()));
        let measurement = CriterionProbeMeasurement::from_writer(
            "parser.batch_loop",
            SharedWriter(Arc::clone(&output)),
        )
        .scope(ProbeScope::Dominant)
        .items(10_000)
        .attribute("harness", "criterion");
        let _criterion: criterion::Criterion<CriterionProbeMeasurement<SharedWriter>> =
            criterion::Criterion::default().with_measurement(measurement.clone());

        let started = measurement.start();
        let duration = measurement.end(started);
        measurement
            .flush()
            .expect("flush criterion probe measurement");
        assert_eq!(measurement.last_error(), None);
        assert_eq!(measurement.zero(), Duration::ZERO);
        assert_eq!(measurement.add(&duration, &Duration::ZERO), duration);
        assert_eq!(measurement.to_f64(&duration), duration.as_nanos() as f64);

        let jsonl =
            String::from_utf8(output.lock().expect("buffer lock").clone()).expect("utf8 JSONL");
        let receipt = ingest_probes_jsonl(&ProbeIngestRequest {
            input: jsonl,
            bench: None,
            scenario: None,
        })
        .expect("ingest criterion JSONL");

        assert_eq!(receipt.probes.len(), 1);
        let probe = &receipt.probes[0];
        assert_eq!(probe.name, "parser.batch_loop");
        assert_eq!(probe.scope, Some(ProbeScope::Dominant));
        assert_eq!(probe.iteration, Some(1));
        assert_eq!(probe.items, Some(10_000));
        assert!(probe.metrics["wall_ms"].value.is_finite());
        assert_eq!(probe.metrics["wall_ms"].unit.as_deref(), Some("ms"));
        assert_eq!(probe.attributes["harness"], "criterion");
    }

    #[cfg(feature = "probe-tracing")]
    #[test]
    fn tracing_layer_records_closed_spans_as_probe_jsonl() {
        use std::sync::{Arc, Mutex};
        use tracing::{Level, span};
        use tracing_subscriber::prelude::*;

        #[derive(Clone)]
        struct SharedWriter(Arc<Mutex<Vec<u8>>>);

        impl Write for SharedWriter {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0
                    .lock()
                    .map_err(|_| io::Error::other("buffer lock poisoned"))?
                    .write(buf)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let output = Arc::new(Mutex::new(Vec::new()));
        let layer = TracingProbeLayer::from_writer(SharedWriter(Arc::clone(&output)));
        let subscriber = tracing_subscriber::registry().with(layer.clone());

        tracing::subscriber::with_default(subscriber, || {
            let span = span!(
                Level::INFO,
                "parser.tokenize",
                scope = "local",
                items = 10_000_u64,
                alloc_bytes = 184_320.0,
                phase = "tokenize"
            );
            {
                let _guard = span.enter();
            }
            drop(span);
        });

        layer.flush().expect("flush tracing probe layer");
        assert_eq!(layer.last_error(), None);

        let jsonl =
            String::from_utf8(output.lock().expect("buffer lock").clone()).expect("utf8 JSONL");
        let receipt = ingest_probes_jsonl(&ProbeIngestRequest {
            input: jsonl,
            bench: None,
            scenario: None,
        })
        .expect("ingest tracing JSONL");

        assert_eq!(receipt.probes.len(), 1);
        let probe = &receipt.probes[0];
        assert_eq!(probe.name, "parser.tokenize");
        assert_eq!(probe.scope, Some(ProbeScope::Local));
        assert_eq!(probe.items, Some(10_000));
        assert_eq!(probe.metrics["alloc_bytes"].unit.as_deref(), Some("bytes"));
        assert!(probe.metrics["wall_ms"].value.is_finite());
        assert_eq!(probe.metrics["wall_ms"].unit.as_deref(), Some("ms"));
        assert_eq!(probe.attributes["phase"], "tokenize");
    }
}
