//! Sensor report building for cockpit integration.
//!
//! This crate provides the `SensorReportBuilder` for wrapping a `PerfgateReport`
//! into a `sensor.report.v1` envelope suitable for cockpit integration.
//!
//! # Overview
//!
//! The sensor report is a standardized envelope format for CI/CD cockpit systems.
//! It wraps performance benchmark results with:
//! - Tool metadata (name, version)
//! - Run metadata (timestamps, duration)
//! - Capabilities (baseline availability, engine features)
//! - Verdict (pass/warn/fail with counts)
//! - Findings (individual check results with fingerprints)
//! - Artifacts (links to detailed reports)
//!
//! # Example
//!
//! ```rust
//! use perfgate_sensor::{SensorReportBuilder, sensor_fingerprint, default_engine_capability};
//! use perfgate_types::{ToolInfo, PerfgateReport, SensorReport, CapabilityStatus};
//!
//! let tool = ToolInfo {
//!     name: "perfgate".to_string(),
//!     version: "0.1.0".to_string(),
//! };
//!
//! // Build a fingerprint for a finding
//! let fp = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail", "wall_ms"]);
//! assert_eq!(fp.len(), 64); // SHA-256 hex
//!
//! // Check default engine capability (varies by platform)
//! let cap = default_engine_capability();
//! if cfg!(unix) {
//!     assert_eq!(cap.status, CapabilityStatus::Available);
//! } else {
//!     assert_eq!(cap.status, CapabilityStatus::Unavailable);
//! }
//!
//! // Build a sensor report (with a minimal PerfgateReport)
//! // See SensorReportBuilder documentation for full example.
//! ```

use perfgate_sha256::sha256_hex;
use perfgate_types::{
    BASELINE_REASON_NO_BASELINE, CHECK_ID_TOOL_RUNTIME, CHECK_ID_TOOL_TRUNCATION, Capability,
    CapabilityStatus, FINDING_CODE_RUNTIME_ERROR, FINDING_CODE_TRUNCATED, MAX_FINDINGS_DEFAULT,
    PerfgateReport, SENSOR_REPORT_SCHEMA_V1, SensorArtifact, SensorCapabilities, SensorFinding,
    SensorReport, SensorRunMeta, SensorSeverity, SensorVerdict, SensorVerdictCounts,
    SensorVerdictStatus, Severity, ToolInfo, VERDICT_REASON_TOOL_ERROR, VERDICT_REASON_TRUNCATED,
    VerdictStatus,
};

/// Build a fleet-standard fingerprint from semantic parts.
///
/// The fingerprint is a SHA-256 hash of the pipe-joined parts, with trailing
/// empty parts trimmed. This provides a stable, deterministic identifier for
/// findings that can be used for deduplication and tracking.
///
/// # Example
///
/// ```rust
/// use perfgate_sensor::sensor_fingerprint;
///
/// let fp = sensor_fingerprint(&["tool", "check", "code", "metric"]);
/// assert_eq!(fp.len(), 64); // SHA-256 hex string
///
/// // Trailing empty parts are trimmed
/// let fp1 = sensor_fingerprint(&["a", "b", ""]);
/// let fp2 = sensor_fingerprint(&["a", "b"]);
/// assert_eq!(fp1, fp2);
///
/// // Different inputs produce different fingerprints
/// let fp3 = sensor_fingerprint(&["a", "c"]);
/// assert_ne!(fp1, fp3);
/// ```
pub fn sensor_fingerprint(parts: &[&str]) -> String {
    let trimmed: Vec<&str> = parts
        .iter()
        .rev()
        .skip_while(|s| s.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .copied()
        .collect();
    sha256_hex(trimmed.join("|").as_bytes())
}

/// Build a default engine capability based on the current platform.
///
/// On Unix platforms, returns `CapabilityStatus::Available` because
/// the engine can collect `cpu_ms` and `max_rss_kb` via `wait4()`.
///
/// On non-Unix platforms, returns `CapabilityStatus::Unavailable` with
/// reason `"platform_limited"` because these metrics are not available.
///
/// # Example
///
/// ```rust
/// use perfgate_sensor::default_engine_capability;
/// use perfgate_types::CapabilityStatus;
///
/// let cap = default_engine_capability();
/// if cfg!(unix) {
///     assert_eq!(cap.status, CapabilityStatus::Available);
///     assert!(cap.reason.is_none());
/// } else {
///     assert_eq!(cap.status, CapabilityStatus::Unavailable);
///     assert_eq!(cap.reason, Some("platform_limited".to_string()));
/// }
/// ```
pub fn default_engine_capability() -> Capability {
    if cfg!(unix) {
        Capability {
            status: CapabilityStatus::Available,
            reason: None,
        }
    } else {
        Capability {
            status: CapabilityStatus::Unavailable,
            reason: Some("platform_limited".to_string()),
        }
    }
}

/// Apply truncation to a findings vector if it exceeds the given limit.
///
/// When truncation is applied:
/// - `findings` is truncated to `limit - 1` real findings, then a meta-finding is appended
/// - `reasons` gets `VERDICT_REASON_TRUNCATED` appended (if not already present)
/// - Returns `Some((total, shown))` where total is the original count and shown is the emitted count
///
/// When not applied (under limit): returns `None` and leaves inputs unchanged.
fn truncate_findings(
    findings: &mut Vec<SensorFinding>,
    reasons: &mut Vec<String>,
    limit: usize,
    tool_name: &str,
) -> Option<(usize, usize)> {
    if findings.len() <= limit {
        return None;
    }
    let total = findings.len();
    let shown = limit.saturating_sub(1);
    findings.truncate(shown);
    findings.push(SensorFinding {
        check_id: CHECK_ID_TOOL_TRUNCATION.to_string(),
        code: FINDING_CODE_TRUNCATED.to_string(),
        severity: SensorSeverity::Info,
        message: format!(
            "Showing {} of {} findings; {} omitted",
            shown,
            total,
            total - shown
        ),
        fingerprint: Some(sensor_fingerprint(&[
            tool_name,
            CHECK_ID_TOOL_TRUNCATION,
            FINDING_CODE_TRUNCATED,
        ])),
        data: Some(serde_json::json!({
            "total_findings": total,
            "shown_findings": shown,
        })),
    });
    if !reasons.contains(&VERDICT_REASON_TRUNCATED.to_string()) {
        reasons.push(VERDICT_REASON_TRUNCATED.to_string());
    }
    Some((total, shown))
}

/// A single bench's outcome for aggregation into a sensor report.
#[allow(clippy::large_enum_variant)]
pub enum BenchOutcome {
    /// The bench ran successfully and produced a report.
    Success {
        bench_name: String,
        report: PerfgateReport,
        has_compare: bool,
        baseline_available: bool,
        markdown: String,
        extras_prefix: String,
    },
    /// The bench failed with an error before producing a report.
    Error {
        bench_name: String,
        error_message: String,
        stage: &'static str,
        error_kind: &'static str,
    },
}

impl BenchOutcome {
    /// Get the bench name regardless of variant.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::BenchOutcome;
    ///
    /// let outcome = BenchOutcome::Error {
    ///     bench_name: "my-bench".to_string(),
    ///     error_message: "oops".to_string(),
    ///     stage: "run",
    ///     error_kind: "exec_error",
    /// };
    /// assert_eq!(outcome.bench_name(), "my-bench");
    /// ```
    pub fn bench_name(&self) -> &str {
        match self {
            BenchOutcome::Success { bench_name, .. } => bench_name,
            BenchOutcome::Error { bench_name, .. } => bench_name,
        }
    }
}

/// Builder for constructing a SensorReport from a PerfgateReport.
///
/// This builder provides a fluent API for constructing sensor reports
/// suitable for cockpit integration. It handles:
/// - Mapping verdict status (Pass/Warn/Fail) to sensor vocabulary (Pass/Warn/Error)
/// - Generating fingerprints for findings
/// - Truncating findings when limits are exceeded
/// - Aggregating multiple bench outcomes
/// - Sorting artifacts deterministically
///
/// # Example
///
/// ```rust
/// use perfgate_sensor::SensorReportBuilder;
/// use perfgate_types::{ToolInfo, PerfgateReport, VerdictStatus, Verdict, VerdictCounts, ReportSummary, REPORT_SCHEMA_V1};
///
/// let tool = ToolInfo {
///     name: "perfgate".to_string(),
///     version: "0.1.0".to_string(),
/// };
///
/// let report = PerfgateReport {
///     report_type: REPORT_SCHEMA_V1.to_string(),
///     verdict: Verdict {
///         status: VerdictStatus::Pass,
///         counts: VerdictCounts { pass: 2, warn: 0, fail: 0 },
///         reasons: vec![],
///     },
///     compare: None,
///     findings: vec![],
///     summary: ReportSummary { pass_count: 2, warn_count: 0, fail_count: 0, total_count: 2 },
/// };
///
/// let sensor_report = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
///     .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
///     .baseline(true, None)
///     .artifact("report.json".to_string(), "sensor_report".to_string())
///     .build(&report);
///
/// assert_eq!(sensor_report.verdict.status, perfgate_types::SensorVerdictStatus::Pass);
/// assert_eq!(sensor_report.verdict.counts.info, 2);
/// ```
pub struct SensorReportBuilder {
    tool: ToolInfo,
    started_at: String,
    ended_at: Option<String>,
    duration_ms: Option<u64>,
    baseline_available: bool,
    baseline_reason: Option<String>,
    engine_capability: Option<Capability>,
    artifacts: Vec<SensorArtifact>,
    max_findings: Option<usize>,
}

impl SensorReportBuilder {
    /// Create a new SensorReportBuilder.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::ToolInfo;
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let builder = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string());
    /// ```
    pub fn new(tool: ToolInfo, started_at: String) -> Self {
        Self {
            tool,
            started_at,
            ended_at: None,
            duration_ms: None,
            baseline_available: false,
            baseline_reason: None,
            engine_capability: Some(default_engine_capability()),
            artifacts: Vec::new(),
            max_findings: None,
        }
    }

    /// Set the end time and duration.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::ToolInfo;
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let builder = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .ended_at("2024-01-01T00:01:00Z".to_string(), 60000);
    /// ```
    pub fn ended_at(mut self, ended_at: String, duration_ms: u64) -> Self {
        self.ended_at = Some(ended_at);
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Set baseline availability.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::ToolInfo;
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// // Baseline available
    /// let builder = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .baseline(true, None);
    /// ```
    pub fn baseline(mut self, available: bool, reason: Option<String>) -> Self {
        self.baseline_available = available;
        self.baseline_reason = reason;
        self
    }

    /// Set engine capability explicitly.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::{ToolInfo, Capability, CapabilityStatus};
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let builder = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .engine(Capability {
    ///         status: CapabilityStatus::Available,
    ///         reason: None,
    ///     });
    /// ```
    pub fn engine(mut self, capability: Capability) -> Self {
        self.engine_capability = Some(capability);
        self
    }

    /// Add an artifact.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::ToolInfo;
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let builder = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .artifact("report.json".to_string(), "sensor_report".to_string());
    /// ```
    pub fn artifact(mut self, path: String, artifact_type: String) -> Self {
        self.artifacts.push(SensorArtifact {
            path,
            artifact_type,
        });
        self
    }

    /// Set the maximum number of findings to include.
    /// When exceeded, findings are truncated and a meta-finding is appended.
    /// `findings_emitted` in the output counts real findings only (excluding the truncation meta-finding).
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::ToolInfo;
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let builder = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .max_findings(50);
    /// ```
    pub fn max_findings(mut self, limit: usize) -> Self {
        self.max_findings = Some(limit);
        self
    }

    /// Take ownership of accumulated artifacts (for manual report building).
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::ToolInfo;
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let mut builder = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .artifact("report.json".to_string(), "sensor_report".to_string());
    /// let artifacts = builder.take_artifacts();
    /// assert_eq!(artifacts.len(), 1);
    /// assert_eq!(artifacts[0].path, "report.json");
    /// ```
    pub fn take_artifacts(&mut self) -> Vec<SensorArtifact> {
        std::mem::take(&mut self.artifacts)
    }

    /// Build the SensorReport from a PerfgateReport.
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::{
    ///     ToolInfo, PerfgateReport, VerdictStatus, Verdict, VerdictCounts,
    ///     ReportSummary, SensorVerdictStatus, REPORT_SCHEMA_V1,
    /// };
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let report = PerfgateReport {
    ///     report_type: REPORT_SCHEMA_V1.to_string(),
    ///     verdict: Verdict {
    ///         status: VerdictStatus::Pass,
    ///         counts: VerdictCounts { pass: 1, warn: 0, fail: 0 },
    ///         reasons: vec![],
    ///     },
    ///     compare: None,
    ///     findings: vec![],
    ///     summary: ReportSummary {
    ///         pass_count: 1, warn_count: 0, fail_count: 0, total_count: 1,
    ///     },
    /// };
    ///
    /// let sensor = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
    ///     .baseline(true, None)
    ///     .build(&report);
    ///
    /// assert_eq!(sensor.verdict.status, SensorVerdictStatus::Pass);
    /// assert_eq!(sensor.verdict.counts.info, 1);
    /// ```
    pub fn build(mut self, report: &PerfgateReport) -> SensorReport {
        let status = match report.verdict.status {
            VerdictStatus::Pass => SensorVerdictStatus::Pass,
            VerdictStatus::Warn => SensorVerdictStatus::Warn,
            VerdictStatus::Fail => SensorVerdictStatus::Fail,
        };

        let counts = SensorVerdictCounts {
            info: report.summary.pass_count,
            warn: report.summary.warn_count,
            error: report.summary.fail_count,
        };

        let mut reasons = report.verdict.reasons.clone();

        let mut findings: Vec<SensorFinding> = report
            .findings
            .iter()
            .map(|f| {
                let metric_name = f
                    .data
                    .as_ref()
                    .map(|d| d.metric_name.as_str())
                    .unwrap_or("");
                SensorFinding {
                    check_id: f.check_id.clone(),
                    code: f.code.clone(),
                    severity: match f.severity {
                        Severity::Warn => SensorSeverity::Warn,
                        Severity::Fail => SensorSeverity::Error,
                    },
                    message: f.message.clone(),
                    fingerprint: Some(sensor_fingerprint(&[
                        &self.tool.name,
                        &f.check_id,
                        &f.code,
                        metric_name,
                    ])),
                    data: f.data.as_ref().and_then(|d| serde_json::to_value(d).ok()),
                }
            })
            .collect();

        let truncation_totals = if let Some(limit) = self.max_findings {
            truncate_findings(&mut findings, &mut reasons, limit, &self.tool.name)
        } else {
            None
        };

        let verdict = SensorVerdict {
            status,
            counts,
            reasons,
        };

        let capabilities = SensorCapabilities {
            baseline: Capability {
                status: if self.baseline_available {
                    CapabilityStatus::Available
                } else {
                    CapabilityStatus::Unavailable
                },
                reason: self.baseline_reason,
            },
            engine: self.engine_capability,
        };

        let run = SensorRunMeta {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            capabilities,
        };

        let mut data = serde_json::json!({
            "summary": {
                "pass_count": report.summary.pass_count,
                "warn_count": report.summary.warn_count,
                "fail_count": report.summary.fail_count,
                "total_count": report.summary.total_count,
                "bench_count": 1,
            }
        });

        if let Some((total, emitted)) = truncation_totals {
            data["findings_total"] = serde_json::json!(total);
            data["findings_emitted"] = serde_json::json!(emitted);
        }

        self.artifacts
            .sort_by(|a, b| (&a.artifact_type, &a.path).cmp(&(&b.artifact_type, &b.path)));

        SensorReport {
            schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: self.tool,
            run,
            verdict,
            findings,
            artifacts: self.artifacts,
            data,
        }
    }

    /// Build an error SensorReport for catastrophic failures.
    ///
    /// This creates a report when the sensor itself failed to run properly.
    /// `stage` indicates which phase failed (e.g. "config_parse", "run_command").
    /// `error_kind` classifies the error (e.g. "io_error", "parse_error", "exec_error").
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::SensorReportBuilder;
    /// use perfgate_types::{ToolInfo, SensorVerdictStatus};
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let sensor = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .build_error("config not found", "config_parse", "parse_error");
    ///
    /// assert_eq!(sensor.verdict.status, SensorVerdictStatus::Fail);
    /// assert_eq!(sensor.verdict.counts.error, 1);
    /// assert_eq!(sensor.findings.len(), 1);
    /// ```
    pub fn build_error(
        mut self,
        error_message: &str,
        stage: &str,
        error_kind: &str,
    ) -> SensorReport {
        let verdict = SensorVerdict {
            status: SensorVerdictStatus::Fail,
            counts: SensorVerdictCounts {
                info: 0,
                warn: 0,
                error: 1,
            },
            reasons: vec![VERDICT_REASON_TOOL_ERROR.to_string()],
        };

        let finding = SensorFinding {
            check_id: CHECK_ID_TOOL_RUNTIME.to_string(),
            code: FINDING_CODE_RUNTIME_ERROR.to_string(),
            severity: SensorSeverity::Error,
            message: error_message.to_string(),
            fingerprint: Some(sensor_fingerprint(&[
                &self.tool.name,
                CHECK_ID_TOOL_RUNTIME,
                FINDING_CODE_RUNTIME_ERROR,
                stage,
                error_kind,
            ])),
            data: Some(serde_json::json!({
                "stage": stage,
                "error_kind": error_kind,
            })),
        };

        let capabilities = SensorCapabilities {
            baseline: Capability {
                status: if self.baseline_available {
                    CapabilityStatus::Available
                } else {
                    CapabilityStatus::Unavailable
                },
                reason: self.baseline_reason,
            },
            engine: self.engine_capability,
        };

        let run = SensorRunMeta {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            capabilities,
        };

        let data = serde_json::json!({
            "summary": {
                "pass_count": 0,
                "warn_count": 0,
                "fail_count": 1,
                "total_count": 1,
                "bench_count": 0,
            }
        });

        self.artifacts
            .sort_by(|a, b| (&a.artifact_type, &a.path).cmp(&(&b.artifact_type, &b.path)));

        SensorReport {
            schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: self.tool,
            run,
            verdict,
            findings: vec![finding],
            artifacts: self.artifacts,
            data,
        }
    }

    /// Build an aggregated SensorReport from multiple bench outcomes.
    ///
    /// This encapsulates the multi-bench cockpit aggregation logic:
    /// - Maps findings from each bench's `PerfgateReport` to sensor findings
    /// - In multi-bench mode: prefixes messages with `[bench_name]`, injects
    ///   `bench_name` into finding data, includes bench_name in fingerprint seed
    /// - Aggregates counts (sum), verdict (worst-of), reasons (union/deduped)
    /// - Registers per-bench artifacts
    /// - Combines markdown with `\n---\n\n` separator in multi-bench
    /// - Applies truncation via `truncate_findings()` using `self.max_findings`
    /// - Returns `(SensorReport, combined_markdown)`
    ///
    /// # Examples
    ///
    /// ```
    /// use perfgate_sensor::{SensorReportBuilder, BenchOutcome};
    /// use perfgate_types::{
    ///     ToolInfo, PerfgateReport, VerdictStatus, Verdict, VerdictCounts,
    ///     ReportSummary, SensorVerdictStatus, REPORT_SCHEMA_V1,
    /// };
    ///
    /// let tool = ToolInfo { name: "perfgate".to_string(), version: "0.1.0".to_string() };
    /// let report = PerfgateReport {
    ///     report_type: REPORT_SCHEMA_V1.to_string(),
    ///     verdict: Verdict {
    ///         status: VerdictStatus::Pass,
    ///         counts: VerdictCounts { pass: 1, warn: 0, fail: 0 },
    ///         reasons: vec![],
    ///     },
    ///     compare: None,
    ///     findings: vec![],
    ///     summary: ReportSummary {
    ///         pass_count: 1, warn_count: 0, fail_count: 0, total_count: 1,
    ///     },
    /// };
    ///
    /// let outcome = BenchOutcome::Success {
    ///     bench_name: "my-bench".to_string(),
    ///     report,
    ///     has_compare: false,
    ///     baseline_available: false,
    ///     markdown: "## Results\n".to_string(),
    ///     extras_prefix: "extras".to_string(),
    /// };
    ///
    /// let (sensor, markdown) = SensorReportBuilder::new(tool, "2024-01-01T00:00:00Z".to_string())
    ///     .build_aggregated(&[outcome]);
    ///
    /// assert_eq!(sensor.verdict.status, SensorVerdictStatus::Pass);
    /// assert!(markdown.contains("Results"));
    /// ```
    pub fn build_aggregated(mut self, outcomes: &[BenchOutcome]) -> (SensorReport, String) {
        let multi_bench = outcomes.len() > 1;

        let mut aggregated_findings: Vec<SensorFinding> = Vec::new();
        let mut total_info = 0u32;
        let mut total_warn = 0u32;
        let mut total_error = 0u32;
        let mut worst_status = SensorVerdictStatus::Pass;
        let mut all_reasons: Vec<String> = Vec::new();
        let mut combined_markdown = String::new();

        for outcome in outcomes {
            match outcome {
                BenchOutcome::Success {
                    bench_name,
                    report,
                    has_compare,
                    baseline_available,
                    markdown,
                    extras_prefix,
                } => {
                    for f in &report.findings {
                        let severity = match f.severity {
                            Severity::Warn => SensorSeverity::Warn,
                            Severity::Fail => SensorSeverity::Error,
                        };
                        let mut finding_data =
                            f.data.as_ref().and_then(|d| serde_json::to_value(d).ok());
                        if multi_bench {
                            if let Some(val) = &mut finding_data {
                                if let Some(obj) = val.as_object_mut() {
                                    obj.insert(
                                        "bench_name".to_string(),
                                        serde_json::Value::String(bench_name.clone()),
                                    );
                                }
                            } else {
                                finding_data =
                                    Some(serde_json::json!({ "bench_name": bench_name }));
                            }
                        }
                        let metric_name = f
                            .data
                            .as_ref()
                            .map(|d| d.metric_name.as_str())
                            .unwrap_or("");
                        let fingerprint = if multi_bench {
                            Some(sensor_fingerprint(&[
                                &self.tool.name,
                                bench_name,
                                &f.check_id,
                                &f.code,
                                metric_name,
                            ]))
                        } else {
                            Some(sensor_fingerprint(&[
                                &self.tool.name,
                                &f.check_id,
                                &f.code,
                                metric_name,
                            ]))
                        };
                        aggregated_findings.push(SensorFinding {
                            check_id: f.check_id.clone(),
                            code: f.code.clone(),
                            severity,
                            message: if multi_bench {
                                format!("[{}] {}", bench_name, f.message)
                            } else {
                                f.message.clone()
                            },
                            fingerprint,
                            data: finding_data,
                        });
                    }

                    total_info += report.summary.pass_count;
                    total_warn += report.summary.warn_count;
                    total_error += report.summary.fail_count;

                    match report.verdict.status {
                        VerdictStatus::Fail => {
                            worst_status = SensorVerdictStatus::Fail;
                        }
                        VerdictStatus::Warn => {
                            if worst_status != SensorVerdictStatus::Fail {
                                worst_status = SensorVerdictStatus::Warn;
                            }
                        }
                        VerdictStatus::Pass => {}
                    }

                    for reason in &report.verdict.reasons {
                        if !all_reasons.contains(reason) {
                            all_reasons.push(reason.clone());
                        }
                    }

                    self.artifacts.push(SensorArtifact {
                        path: format!("{}/perfgate.run.v1.json", extras_prefix),
                        artifact_type: "run_receipt".to_string(),
                    });
                    self.artifacts.push(SensorArtifact {
                        path: format!("{}/perfgate.report.v1.json", extras_prefix),
                        artifact_type: "perfgate_report".to_string(),
                    });
                    if *has_compare {
                        self.artifacts.push(SensorArtifact {
                            path: format!("{}/perfgate.compare.v1.json", extras_prefix),
                            artifact_type: "compare_receipt".to_string(),
                        });
                    }

                    if multi_bench && !combined_markdown.is_empty() {
                        combined_markdown.push_str("\n---\n\n");
                    }
                    combined_markdown.push_str(markdown);

                    if !baseline_available
                        && !all_reasons.contains(&BASELINE_REASON_NO_BASELINE.to_string())
                    {
                        all_reasons.push(BASELINE_REASON_NO_BASELINE.to_string());
                    }
                }

                BenchOutcome::Error {
                    bench_name,
                    error_message,
                    stage,
                    error_kind,
                } => {
                    let mut finding_data = serde_json::json!({
                        "stage": stage,
                        "error_kind": error_kind,
                    });
                    if multi_bench {
                        finding_data
                            .as_object_mut()
                            .unwrap()
                            .insert("bench_name".to_string(), serde_json::json!(bench_name));
                    }
                    let fingerprint = Some(sensor_fingerprint(&[
                        &self.tool.name,
                        bench_name,
                        CHECK_ID_TOOL_RUNTIME,
                        FINDING_CODE_RUNTIME_ERROR,
                        stage,
                    ]));
                    let message = if multi_bench {
                        format!("[{}] {}", bench_name, error_message)
                    } else {
                        error_message.clone()
                    };
                    aggregated_findings.push(SensorFinding {
                        check_id: CHECK_ID_TOOL_RUNTIME.to_string(),
                        code: FINDING_CODE_RUNTIME_ERROR.to_string(),
                        severity: SensorSeverity::Error,
                        message,
                        fingerprint,
                        data: Some(finding_data),
                    });

                    total_error += 1;
                    worst_status = SensorVerdictStatus::Fail;
                    if !all_reasons.contains(&VERDICT_REASON_TOOL_ERROR.to_string()) {
                        all_reasons.push(VERDICT_REASON_TOOL_ERROR.to_string());
                    }

                    if multi_bench && !combined_markdown.is_empty() {
                        combined_markdown.push_str("\n---\n\n");
                    }
                    combined_markdown.push_str(&format!(
                        "## {}\n\n**Error:** {}\n",
                        bench_name, error_message
                    ));

                    if *stage == perfgate_types::STAGE_BASELINE_RESOLVE
                        && !all_reasons.contains(&BASELINE_REASON_NO_BASELINE.to_string())
                    {
                        all_reasons.push(BASELINE_REASON_NO_BASELINE.to_string());
                    }
                }
            }
        }

        self.artifacts.push(SensorArtifact {
            path: "comment.md".to_string(),
            artifact_type: "markdown".to_string(),
        });

        let limit = self.max_findings.unwrap_or(MAX_FINDINGS_DEFAULT);
        let truncation_totals = truncate_findings(
            &mut aggregated_findings,
            &mut all_reasons,
            limit,
            &self.tool.name,
        );

        let mut data = serde_json::json!({
            "summary": {
                "pass_count": total_info,
                "warn_count": total_warn,
                "fail_count": total_error,
                "total_count": total_info + total_warn + total_error,
                "bench_count": outcomes.len(),
            }
        });

        if let Some((total, emitted)) = truncation_totals {
            data["findings_total"] = serde_json::json!(total);
            data["findings_emitted"] = serde_json::json!(emitted);
        }

        let any_baseline_available = outcomes.iter().any(|o| {
            matches!(
                o,
                BenchOutcome::Success {
                    baseline_available: true,
                    ..
                }
            )
        });
        let all_baseline_available = outcomes.iter().all(|o| {
            matches!(
                o,
                BenchOutcome::Success {
                    baseline_available: true,
                    ..
                }
            )
        });

        let capabilities = SensorCapabilities {
            baseline: Capability {
                status: if all_baseline_available {
                    CapabilityStatus::Available
                } else {
                    CapabilityStatus::Unavailable
                },
                reason: if !any_baseline_available {
                    self.baseline_reason
                        .clone()
                        .or(Some(BASELINE_REASON_NO_BASELINE.to_string()))
                } else {
                    None
                },
            },
            engine: self.engine_capability,
        };

        let run = SensorRunMeta {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            capabilities,
        };

        let verdict = SensorVerdict {
            status: worst_status,
            counts: SensorVerdictCounts {
                info: total_info,
                warn: total_warn,
                error: total_error,
            },
            reasons: all_reasons,
        };

        self.artifacts
            .sort_by(|a, b| (&a.artifact_type, &a.path).cmp(&(&b.artifact_type, &b.path)));

        let sensor_report = SensorReport {
            schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: self.tool,
            run,
            verdict,
            findings: aggregated_findings,
            artifacts: self.artifacts,
            data,
        };

        (sensor_report, combined_markdown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        FINDING_CODE_METRIC_FAIL, FINDING_CODE_METRIC_WARN, REPORT_SCHEMA_V1, ReportFinding,
        ReportSummary, Verdict, VerdictCounts,
    };

    fn make_tool_info() -> ToolInfo {
        ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    fn make_pass_report() -> PerfgateReport {
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 2,
                    warn: 0,
                    fail: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            summary: ReportSummary {
                pass_count: 2,
                warn_count: 0,
                fail_count: 0,
                total_count: 2,
            },
        }
    }

    fn make_fail_report() -> PerfgateReport {
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 1,
                },
                reasons: vec!["wall_ms_fail".to_string()],
            },
            compare: None,
            findings: vec![ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: "wall_ms regression: +25.00% (threshold: 20.0%)".to_string(),
                data: None,
            }],
            summary: ReportSummary {
                pass_count: 1,
                warn_count: 0,
                fail_count: 1,
                total_count: 2,
            },
        }
    }

    fn make_warn_report() -> PerfgateReport {
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 1,
                    fail: 0,
                },
                reasons: vec!["wall_ms_warn".to_string()],
            },
            compare: None,
            findings: vec![ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_WARN.to_string(),
                severity: Severity::Warn,
                message: "wall_ms regression: +15.00% (threshold: 20.0%)".to_string(),
                data: None,
            }],
            summary: ReportSummary {
                pass_count: 1,
                warn_count: 1,
                fail_count: 0,
                total_count: 2,
            },
        }
    }

    #[test]
    fn test_build_pass_sensor_report() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Pass);
        assert_eq!(sensor_report.verdict.counts.info, 2);
        assert_eq!(sensor_report.verdict.counts.warn, 0);
        assert_eq!(sensor_report.verdict.counts.error, 0);
        assert!(sensor_report.findings.is_empty());
        assert_eq!(
            sensor_report.run.capabilities.baseline.status,
            CapabilityStatus::Available
        );
        assert!(sensor_report.data.get("summary").is_some());
        assert!(sensor_report.data.get("compare").is_none());
    }

    #[test]
    fn test_build_fail_sensor_report() {
        let report = make_fail_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(sensor_report.verdict.counts.error, 1);
        assert_eq!(sensor_report.findings.len(), 1);
        assert_eq!(sensor_report.findings[0].severity, SensorSeverity::Error);
    }

    #[test]
    fn test_build_warn_sensor_report() {
        let report = make_warn_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Warn);
        assert_eq!(sensor_report.verdict.counts.warn, 1);
        assert_eq!(sensor_report.findings.len(), 1);
        assert_eq!(sensor_report.findings[0].severity, SensorSeverity::Warn);
    }

    #[test]
    fn test_build_with_no_baseline() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(false, Some("baseline.json not found".to_string()));

        let sensor_report = builder.build(&report);

        assert_eq!(
            sensor_report.run.capabilities.baseline.status,
            CapabilityStatus::Unavailable
        );
        assert_eq!(
            sensor_report.run.capabilities.baseline.reason,
            Some("baseline.json not found".to_string())
        );
    }

    #[test]
    fn test_build_with_artifacts_sorted() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .artifact(
                    "extras/perfgate.run.v1.json".to_string(),
                    "run_receipt".to_string(),
                )
                .artifact("comment.md".to_string(), "markdown".to_string());

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.artifacts.len(), 2);
        assert_eq!(sensor_report.artifacts[0].artifact_type, "markdown");
        assert_eq!(sensor_report.artifacts[1].artifact_type, "run_receipt");
    }

    #[test]
    fn test_build_error_report() {
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:00:01Z".to_string(), 1000)
                .baseline(false, None);

        let sensor_report = builder.build_error(
            "config file not found",
            perfgate_types::STAGE_CONFIG_PARSE,
            perfgate_types::ERROR_KIND_PARSE,
        );

        assert_eq!(sensor_report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(sensor_report.verdict.counts.error, 1);
        assert_eq!(sensor_report.verdict.reasons, vec!["tool_error"]);
        assert_eq!(sensor_report.findings.len(), 1);
        assert_eq!(sensor_report.findings[0].check_id, "tool.runtime");
        assert_eq!(sensor_report.findings[0].code, "runtime_error");
        assert!(
            sensor_report.findings[0]
                .message
                .contains("config file not found")
        );
        let data = sensor_report.findings[0].data.as_ref().unwrap();
        assert_eq!(data["stage"], "config_parse");
        assert_eq!(data["error_kind"], "parse_error");
    }

    #[test]
    fn test_fingerprint_format_for_metric_finding() {
        let report = make_fail_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.findings.len(), 1);
        assert_eq!(
            sensor_report.findings[0].fingerprint,
            Some(sensor_fingerprint(&[
                "perfgate",
                "perf.budget",
                "metric_fail",
                ""
            ]))
        );
    }

    #[test]
    fn test_fingerprint_format_for_metric_finding_with_data() {
        use perfgate_types::{Direction, FindingData};

        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 1,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: "wall_ms regression".to_string(),
                data: Some(FindingData {
                    metric_name: "wall_ms".to_string(),
                    baseline: 100.0,
                    current: 150.0,
                    regression_pct: 50.0,
                    threshold: 0.2,
                    direction: Direction::Lower,
                }),
            }],
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 1,
                total_count: 1,
            },
        };

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert_eq!(
            sensor_report.findings[0].fingerprint,
            Some(sensor_fingerprint(&[
                "perfgate",
                "perf.budget",
                "metric_fail",
                "wall_ms"
            ]))
        );
    }

    #[test]
    fn test_fingerprint_format_for_error_finding() {
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(false, None);

        let sensor_report = builder.build_error(
            "config file not found",
            perfgate_types::STAGE_CONFIG_PARSE,
            perfgate_types::ERROR_KIND_PARSE,
        );

        assert_eq!(
            sensor_report.findings[0].fingerprint,
            Some(sensor_fingerprint(&[
                "perfgate",
                "tool.runtime",
                "runtime_error",
                "config_parse",
                "parse_error"
            ]))
        );
    }

    #[test]
    fn test_fingerprint_absent_when_no_findings() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert!(sensor_report.findings.is_empty());
    }

    #[test]
    fn test_truncation_not_applied_under_limit() {
        let report = make_fail_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None)
                .max_findings(100);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.findings.len(), 1);
        assert_ne!(sensor_report.findings[0].check_id, CHECK_ID_TOOL_TRUNCATION);
        assert!(
            !sensor_report
                .verdict
                .reasons
                .contains(&"truncated".to_string()),
            "verdict.reasons should NOT contain 'truncated' when under limit"
        );
    }

    #[test]
    fn test_truncation_applied_at_limit() {
        use perfgate_types::FindingData;

        let findings: Vec<ReportFinding> = (0..5)
            .map(|i| ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: format!("metric {} regression", i),
                data: Some(FindingData {
                    metric_name: format!("metric_{}", i),
                    baseline: 100.0,
                    current: 150.0,
                    regression_pct: 50.0,
                    threshold: 0.2,
                    direction: perfgate_types::Direction::Lower,
                }),
            })
            .collect();

        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 5,
                },
                reasons: vec![],
            },
            compare: None,
            findings,
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 5,
                total_count: 5,
            },
        };

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None)
                .max_findings(3);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.findings.len(), 3);

        let last = &sensor_report.findings[2];
        assert_eq!(last.check_id, CHECK_ID_TOOL_TRUNCATION);
        assert_eq!(last.code, FINDING_CODE_TRUNCATED);
        assert_eq!(last.severity, SensorSeverity::Info);
        assert_eq!(
            last.fingerprint,
            Some(sensor_fingerprint(&[
                "perfgate",
                "tool.truncation",
                "truncated"
            ]))
        );

        let data = last.data.as_ref().unwrap();
        assert_eq!(data["total_findings"], 5);
        assert_eq!(data["shown_findings"], 2);

        assert!(
            sensor_report
                .verdict
                .reasons
                .contains(&"truncated".to_string()),
            "verdict.reasons should contain 'truncated'"
        );

        assert_eq!(sensor_report.data["findings_total"], 5);
        assert_eq!(sensor_report.data["findings_emitted"], 2);
    }

    #[test]
    fn test_truncation_meta_finding_structure() {
        use perfgate_types::FindingData;

        let findings: Vec<ReportFinding> = (0..10)
            .map(|i| ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: format!("metric {} regression", i),
                data: Some(FindingData {
                    metric_name: format!("metric_{}", i),
                    baseline: 100.0,
                    current: 150.0,
                    regression_pct: 50.0,
                    threshold: 0.2,
                    direction: perfgate_types::Direction::Lower,
                }),
            })
            .collect();

        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 10,
                },
                reasons: vec![],
            },
            compare: None,
            findings,
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 10,
                total_count: 10,
            },
        };

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None)
                .max_findings(5);

        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.findings.len(), 5);

        let meta = &sensor_report.findings[4];
        assert!(meta.message.contains("Showing 4 of 10"));
        assert!(meta.message.contains("6 omitted"));

        assert!(
            sensor_report
                .verdict
                .reasons
                .contains(&"truncated".to_string()),
            "verdict.reasons should contain 'truncated'"
        );

        assert_eq!(sensor_report.data["findings_total"], 10);
        assert_eq!(sensor_report.data["findings_emitted"], 4);
    }

    #[test]
    fn test_sensor_report_serialization_round_trip() {
        let report = make_fail_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
                .baseline(true, None)
                .artifact("report.json".to_string(), "sensor_report".to_string());

        let sensor_report = builder.build(&report);

        let json = serde_json::to_string(&sensor_report).expect("should serialize");

        let deserialized: SensorReport = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(sensor_report, deserialized);
    }

    #[test]
    fn test_build_error_report_for_invalid_bench_name() {
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:00:01Z".to_string(), 1000)
                .baseline(false, None);

        let msg =
            "bench name \"../evil\" contains a \"..\" path segment (path traversal is forbidden)";
        let sensor_report = builder.build_error(
            msg,
            perfgate_types::STAGE_CONFIG_PARSE,
            perfgate_types::ERROR_KIND_PARSE,
        );

        assert_eq!(sensor_report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(sensor_report.verdict.counts.error, 1);
        assert_eq!(sensor_report.findings.len(), 1);
        assert_eq!(sensor_report.findings[0].check_id, "tool.runtime");
        assert_eq!(sensor_report.findings[0].code, "runtime_error");
        let data = sensor_report.findings[0].data.as_ref().unwrap();
        assert_eq!(data["stage"], "config_parse");
        assert_eq!(data["error_kind"], "parse_error");
    }

    // --- Engine capability tests ---

    #[test]
    fn test_build_pass_sensor_report_has_engine_capability() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None);

        let sensor_report = builder.build(&report);

        assert!(
            sensor_report.run.capabilities.engine.is_some(),
            "engine capability should be present"
        );
    }

    #[test]
    fn test_build_error_report_has_engine_capability() {
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(false, None);

        let sensor_report = builder.build_error(
            "config file not found",
            perfgate_types::STAGE_CONFIG_PARSE,
            perfgate_types::ERROR_KIND_PARSE,
        );

        assert!(
            sensor_report.run.capabilities.engine.is_some(),
            "engine capability should be present in error report"
        );
    }

    #[test]
    fn test_engine_capability_explicit_override() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .baseline(true, None)
                .engine(Capability {
                    status: CapabilityStatus::Unavailable,
                    reason: Some("platform_limited".to_string()),
                });

        let sensor_report = builder.build(&report);

        let engine = sensor_report.run.capabilities.engine.unwrap();
        assert_eq!(engine.status, CapabilityStatus::Unavailable);
        assert_eq!(engine.reason, Some("platform_limited".to_string()));
    }

    #[test]
    fn test_default_engine_capability_value() {
        let cap = default_engine_capability();
        if cfg!(unix) {
            assert_eq!(cap.status, CapabilityStatus::Available);
            assert!(cap.reason.is_none());
        } else {
            assert_eq!(cap.status, CapabilityStatus::Unavailable);
            assert_eq!(cap.reason, Some("platform_limited".to_string()));
        }
    }

    // --- build_aggregated() tests ---

    fn make_bench_outcome(
        bench_name: &str,
        report: PerfgateReport,
        has_compare: bool,
        baseline_available: bool,
        extras_prefix: &str,
    ) -> BenchOutcome {
        BenchOutcome::Success {
            bench_name: bench_name.to_string(),
            report,
            has_compare,
            baseline_available,
            markdown: format!("## {}\n\nSome results\n", bench_name),
            extras_prefix: extras_prefix.to_string(),
        }
    }

    fn make_error_outcome(
        bench_name: &str,
        error_message: &str,
        stage: &'static str,
        error_kind: &'static str,
    ) -> BenchOutcome {
        BenchOutcome::Error {
            bench_name: bench_name.to_string(),
            error_message: error_message.to_string(),
            stage,
            error_kind,
        }
    }

    #[test]
    fn test_build_aggregated_single_bench_matches_build() {
        let report = make_fail_report();
        let outcome = make_bench_outcome("my-bench", report.clone(), true, true, "extras");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
                .baseline(true, None);

        let (sensor_report, _md) = builder.build_aggregated(&[outcome]);

        assert_eq!(sensor_report.findings.len(), 1);
        assert!(
            !sensor_report.findings[0].message.starts_with("[my-bench]"),
            "single bench findings should not be prefixed"
        );
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(sensor_report.verdict.counts.error, 1);
    }

    #[test]
    fn test_build_aggregated_multi_bench_findings_prefixed() {
        let report_a = make_fail_report();
        let report_b = make_warn_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, true, true, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, true, true, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .ended_at("2024-01-01T00:01:00Z".to_string(), 60000);

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(sensor_report.findings.len(), 2);
        assert!(
            sensor_report.findings[0].message.starts_with("[bench-a]"),
            "multi-bench findings should be prefixed: {}",
            sensor_report.findings[0].message
        );
        assert!(
            sensor_report.findings[1].message.starts_with("[bench-b]"),
            "multi-bench findings should be prefixed: {}",
            sensor_report.findings[1].message
        );
        let data_0 = sensor_report.findings[0].data.as_ref().unwrap();
        assert_eq!(data_0["bench_name"], "bench-a");
        let data_1 = sensor_report.findings[1].data.as_ref().unwrap();
        assert_eq!(data_1["bench_name"], "bench-b");
    }

    #[test]
    fn test_build_aggregated_multi_bench_fingerprints_unique() {
        let report_a = make_fail_report();
        let report_b = make_fail_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, true, true, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, true, true, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        let fp_a = sensor_report.findings[0].fingerprint.as_ref().unwrap();
        let fp_b = sensor_report.findings[1].fingerprint.as_ref().unwrap();
        assert_ne!(fp_a, fp_b, "fingerprints should differ per bench");
        assert_eq!(fp_a.len(), 64, "fingerprint should be 64-char hex");
        assert_eq!(fp_b.len(), 64, "fingerprint should be 64-char hex");
    }

    #[test]
    fn test_build_aggregated_multi_bench_verdict_worst_wins() {
        let report_pass = make_pass_report();
        let report_fail = make_fail_report();
        let outcome_pass =
            make_bench_outcome("bench-a", report_pass, false, false, "extras/bench-a");
        let outcome_fail = make_bench_outcome("bench-b", report_fail, true, true, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_pass, outcome_fail]);

        assert_eq!(
            sensor_report.verdict.status,
            SensorVerdictStatus::Fail,
            "worst verdict should win"
        );
    }

    #[test]
    fn test_build_aggregated_multi_bench_counts_summed() {
        let report_a = make_fail_report();
        let report_b = make_warn_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, true, true, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, true, true, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(sensor_report.verdict.counts.info, 2, "pass counts summed");
        assert_eq!(sensor_report.verdict.counts.warn, 1, "warn counts summed");
        assert_eq!(sensor_report.verdict.counts.error, 1, "fail counts summed");
    }

    #[test]
    fn test_build_aggregated_multi_bench_reasons_deduped() {
        let report_a = make_pass_report();
        let report_b = make_pass_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, false, false, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, false, false, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        let no_baseline_count = sensor_report
            .verdict
            .reasons
            .iter()
            .filter(|r| r.as_str() == BASELINE_REASON_NO_BASELINE)
            .count();
        assert_eq!(
            no_baseline_count, 1,
            "no_baseline should appear exactly once"
        );
    }

    #[test]
    fn test_build_aggregated_multi_bench_markdown_joined() {
        let report_a = make_pass_report();
        let report_b = make_pass_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, false, false, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, false, false, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (_sensor_report, md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert!(md.contains("bench-a"), "markdown should contain bench-a");
        assert!(md.contains("bench-b"), "markdown should contain bench-b");
        assert!(
            md.contains("\n---\n\n"),
            "multi-bench markdown should have --- separator"
        );
    }

    #[test]
    fn test_build_aggregated_multi_bench_truncation() {
        use perfgate_types::FindingData;

        let findings: Vec<ReportFinding> = (0..10)
            .map(|i| ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: format!("metric {} regression", i),
                data: Some(FindingData {
                    metric_name: format!("metric_{}", i),
                    baseline: 100.0,
                    current: 150.0,
                    regression_pct: 50.0,
                    threshold: 0.2,
                    direction: perfgate_types::Direction::Lower,
                }),
            })
            .collect();

        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 10,
                },
                reasons: vec![],
            },
            compare: None,
            findings,
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 10,
                total_count: 10,
            },
        };

        let outcome = make_bench_outcome("bench-a", report, true, true, "extras");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
                .max_findings(5);

        let (sensor_report, _md) = builder.build_aggregated(&[outcome]);

        assert_eq!(sensor_report.findings.len(), 5);
        assert_eq!(sensor_report.findings[4].check_id, CHECK_ID_TOOL_TRUNCATION);
        assert_eq!(sensor_report.data["findings_total"], 10);
        assert_eq!(sensor_report.data["findings_emitted"], 4);
        assert!(
            sensor_report
                .verdict
                .reasons
                .contains(&"truncated".to_string())
        );
    }

    #[test]
    fn test_build_aggregated_multi_bench_artifacts_sorted() {
        let report_a = make_pass_report();
        let report_b = make_pass_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, true, true, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, true, true, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        let arts = &sensor_report.artifacts;
        for window in arts.windows(2) {
            assert!(
                (&window[0].artifact_type, &window[0].path)
                    <= (&window[1].artifact_type, &window[1].path),
                "artifacts not sorted: {:?} > {:?}",
                (&window[0].artifact_type, &window[0].path),
                (&window[1].artifact_type, &window[1].path)
            );
        }
    }

    #[test]
    fn test_build_aggregated_baseline_all_available() {
        let report_a = make_pass_report();
        let report_b = make_pass_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, true, true, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, true, true, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(
            sensor_report.run.capabilities.baseline.status,
            CapabilityStatus::Available,
            "all baselines available → status = available"
        );
        assert!(
            sensor_report.run.capabilities.baseline.reason.is_none(),
            "all baselines available → no reason"
        );
    }

    #[test]
    fn test_build_aggregated_baseline_partial() {
        let report_a = make_pass_report();
        let report_b = make_pass_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, true, true, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, false, false, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(
            sensor_report.run.capabilities.baseline.status,
            CapabilityStatus::Unavailable,
            "partial baselines → status = unavailable"
        );
        assert!(
            sensor_report.run.capabilities.baseline.reason.is_none(),
            "partial baselines → reason = null (some have baselines)"
        );
    }

    #[test]
    fn test_build_aggregated_baseline_none() {
        let report_a = make_pass_report();
        let report_b = make_pass_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, false, false, "extras/bench-a");
        let outcome_b = make_bench_outcome("bench-b", report_b, false, false, "extras/bench-b");

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(
            sensor_report.run.capabilities.baseline.status,
            CapabilityStatus::Unavailable,
            "no baselines → status = unavailable"
        );
        assert_eq!(
            sensor_report.run.capabilities.baseline.reason,
            Some(BASELINE_REASON_NO_BASELINE.to_string()),
            "no baselines → reason = no_baseline"
        );
    }

    // --- BenchOutcome::Error aggregation tests ---

    #[test]
    fn test_build_aggregated_mixed_success_and_error() {
        let report_a = make_warn_report();
        let outcome_a = make_bench_outcome("bench-a", report_a, false, false, "extras/bench-a");
        let outcome_b = make_error_outcome(
            "bench-b",
            "failed to spawn: no such file",
            perfgate_types::STAGE_RUN_COMMAND,
            perfgate_types::ERROR_KIND_EXEC,
        );

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);

        assert_eq!(sensor_report.verdict.counts.info, 1);
        assert_eq!(sensor_report.verdict.counts.warn, 1);
        assert_eq!(sensor_report.verdict.counts.error, 1);

        assert!(
            sensor_report
                .verdict
                .reasons
                .contains(&VERDICT_REASON_TOOL_ERROR.to_string()),
            "should have tool_error reason"
        );
        assert!(
            sensor_report
                .verdict
                .reasons
                .contains(&BASELINE_REASON_NO_BASELINE.to_string()),
            "should have no_baseline reason"
        );

        assert_eq!(sensor_report.findings.len(), 2);
        assert!(sensor_report.findings[0].message.starts_with("[bench-a]"));
        assert!(sensor_report.findings[1].message.starts_with("[bench-b]"));
        assert_eq!(sensor_report.findings[1].check_id, CHECK_ID_TOOL_RUNTIME);

        assert_eq!(sensor_report.data["summary"]["bench_count"], 2);

        assert!(md.contains("bench-b"));
        assert!(md.contains("**Error:**"));
    }

    #[test]
    fn test_build_aggregated_single_error_outcome() {
        let outcome = make_error_outcome(
            "bench-a",
            "config parse failure",
            perfgate_types::STAGE_CONFIG_PARSE,
            perfgate_types::ERROR_KIND_PARSE,
        );

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome]);

        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(sensor_report.verdict.counts.error, 1);
        assert_eq!(sensor_report.findings.len(), 1);
        assert_eq!(sensor_report.findings[0].check_id, CHECK_ID_TOOL_RUNTIME);
        assert_eq!(sensor_report.data["summary"]["bench_count"], 1);
    }

    #[test]
    fn test_build_aggregated_error_no_artifacts() {
        let outcome_a =
            make_bench_outcome("bench-a", make_pass_report(), true, true, "extras/bench-a");
        let outcome_b = make_error_outcome(
            "bench-b",
            "spawn error",
            perfgate_types::STAGE_RUN_COMMAND,
            perfgate_types::ERROR_KIND_EXEC,
        );

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        let has_bench_a_artifact = sensor_report
            .artifacts
            .iter()
            .any(|a| a.path.contains("bench-a"));
        let has_bench_b_artifact = sensor_report
            .artifacts
            .iter()
            .any(|a| a.path.contains("bench-b"));

        assert!(has_bench_a_artifact, "bench-a should have artifacts");
        assert!(!has_bench_b_artifact, "bench-b should NOT have artifacts");
    }

    #[test]
    fn test_build_aggregated_error_finding_data() {
        let outcome = make_error_outcome(
            "bench-x",
            "spawn error",
            perfgate_types::STAGE_RUN_COMMAND,
            perfgate_types::ERROR_KIND_EXEC,
        );

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome]);

        let finding = &sensor_report.findings[0];
        let data = finding.data.as_ref().expect("finding should have data");
        assert_eq!(data["stage"], perfgate_types::STAGE_RUN_COMMAND);
        assert_eq!(data["error_kind"], perfgate_types::ERROR_KIND_EXEC);
        assert!(
            data.get("bench_name").is_none() || data["bench_name"].is_null(),
            "single bench error should not have bench_name in data"
        );
    }

    #[test]
    fn test_build_aggregated_error_finding_data_multi_bench() {
        let outcome_a = make_bench_outcome(
            "bench-a",
            make_pass_report(),
            false,
            false,
            "extras/bench-a",
        );
        let outcome_b = make_error_outcome(
            "bench-b",
            "spawn error",
            perfgate_types::STAGE_RUN_COMMAND,
            perfgate_types::ERROR_KIND_EXEC,
        );

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        let error_finding = sensor_report
            .findings
            .iter()
            .find(|f| f.check_id == CHECK_ID_TOOL_RUNTIME)
            .expect("should have error finding");

        let data = error_finding
            .data
            .as_ref()
            .expect("finding should have data");
        assert_eq!(data["stage"], perfgate_types::STAGE_RUN_COMMAND);
        assert_eq!(data["error_kind"], perfgate_types::ERROR_KIND_EXEC);
        assert_eq!(data["bench_name"], "bench-b");
    }

    #[test]
    fn test_bench_outcome_bench_name() {
        let success = make_bench_outcome("my-bench", make_pass_report(), false, false, "extras");
        assert_eq!(success.bench_name(), "my-bench");

        let error = make_error_outcome(
            "bad-bench",
            "error",
            perfgate_types::STAGE_RUN_COMMAND,
            perfgate_types::ERROR_KIND_EXEC,
        );
        assert_eq!(error.bench_name(), "bad-bench");
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn fingerprint_deterministic(parts in proptest::collection::vec("[a-zA-Z0-9_\\-]{0,20}", 0..10)) {
            let parts_ref: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
            let fp1 = sensor_fingerprint(&parts_ref);
            let fp2 = sensor_fingerprint(&parts_ref);
            prop_assert_eq!(&fp1, &fp2, "fingerprint should be deterministic");
            prop_assert_eq!(fp1.len(), 64, "fingerprint should be 64-char hex");
        }

        #[test]
        fn fingerprint_trailing_empty_trimmed(
            prefix in "[a-zA-Z0-9_\\-]{1,10}",
            empty_count in 0usize..5
        ) {
            let mut parts: Vec<String> = vec![prefix.clone()];
            for _ in 0..empty_count {
                parts.push("".to_string());
            }
            let parts_ref: Vec<&str> = parts.iter().map(|s| s.as_str()).collect();
            let fp_with_empty = sensor_fingerprint(&parts_ref);

            let parts_no_empty: Vec<&str> = vec![prefix.as_str()];
            let fp_no_empty = sensor_fingerprint(&parts_no_empty);

            prop_assert_eq!(fp_with_empty, fp_no_empty, "trailing empty parts should be trimmed");
        }

        #[test]
        fn fingerprint_different_inputs_different_output(
            a in "[a-zA-Z0-9_\\-]{1,10}",
            b in "[a-zA-Z0-9_\\-]{1,10}",
            c in "[a-zA-Z0-9_\\-]{1,10}"
        ) {
            prop_assume!(a != b || b != c);
            let fp1 = sensor_fingerprint(&[&a, &b]);
            let fp2 = sensor_fingerprint(&[&b, &c]);
            prop_assert_ne!(fp1, fp2, "different inputs should produce different fingerprints");
        }

        #[test]
        fn fingerprint_order_matters(
            a in "[a-zA-Z0-9_\\-]{1,10}",
            b in "[a-zA-Z0-9_\\-]{1,10}"
        ) {
            prop_assume!(a != b);
            let fp1 = sensor_fingerprint(&[&a, &b]);
            let fp2 = sensor_fingerprint(&[&b, &a]);
            prop_assert_ne!(fp1, fp2, "fingerprint order should matter");
        }
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use insta::assert_json_snapshot;
    use perfgate_types::{
        FINDING_CODE_METRIC_FAIL, FINDING_CODE_METRIC_WARN, REPORT_SCHEMA_V1, ReportFinding,
        ReportSummary, Verdict, VerdictCounts,
    };

    fn make_tool_info() -> ToolInfo {
        ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    /// Fixed engine capability for platform-independent snapshots.
    fn fixed_engine() -> Capability {
        Capability {
            status: CapabilityStatus::Available,
            reason: None,
        }
    }

    #[test]
    fn snapshot_pass_report() {
        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 3,
                    warn: 0,
                    fail: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            summary: ReportSummary {
                pass_count: 3,
                warn_count: 0,
                fail_count: 0,
                total_count: 3,
            },
        };

        let sensor_report =
            SensorReportBuilder::new(make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:31:00Z".to_string(), 60000)
                .baseline(true, None)
                .engine(fixed_engine())
                .build(&report);

        assert_json_snapshot!(sensor_report);
    }

    #[test]
    fn snapshot_fail_report() {
        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 2,
                },
                reasons: vec!["wall_ms_fail".to_string(), "max_rss_kb_fail".to_string()],
            },
            compare: None,
            findings: vec![ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: "wall_ms regression: +30.00% (threshold: 20.0%)".to_string(),
                data: None,
            }],
            summary: ReportSummary {
                pass_count: 1,
                warn_count: 0,
                fail_count: 2,
                total_count: 3,
            },
        };

        let sensor_report =
            SensorReportBuilder::new(make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:31:00Z".to_string(), 60000)
                .baseline(false, Some("no_baseline".to_string()))
                .engine(fixed_engine())
                .build(&report);

        assert_json_snapshot!(sensor_report);
    }

    #[test]
    fn snapshot_error_report() {
        let sensor_report =
            SensorReportBuilder::new(make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:30:01Z".to_string(), 1000)
                .baseline(false, None)
                .engine(fixed_engine())
                .build_error(
                    "failed to parse config: invalid TOML at line 5",
                    perfgate_types::STAGE_CONFIG_PARSE,
                    perfgate_types::ERROR_KIND_PARSE,
                );

        assert_json_snapshot!(sensor_report);
    }

    #[test]
    fn snapshot_aggregated_multi_bench() {
        let report_a = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 1,
                    fail: 0,
                },
                reasons: vec!["wall_ms_warn".to_string()],
            },
            compare: None,
            findings: vec![ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_WARN.to_string(),
                severity: Severity::Warn,
                message: "wall_ms regression: +15.00%".to_string(),
                data: None,
            }],
            summary: ReportSummary {
                pass_count: 1,
                warn_count: 1,
                fail_count: 0,
                total_count: 2,
            },
        };

        let report_b = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 2,
                    warn: 0,
                    fail: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            summary: ReportSummary {
                pass_count: 2,
                warn_count: 0,
                fail_count: 0,
                total_count: 2,
            },
        };

        let outcome_a = BenchOutcome::Success {
            bench_name: "bench-a".to_string(),
            report: report_a,
            has_compare: true,
            baseline_available: true,
            markdown: "## bench-a\n\nResults: +15%".to_string(),
            extras_prefix: "extras/bench-a".to_string(),
        };

        let outcome_b = BenchOutcome::Success {
            bench_name: "bench-b".to_string(),
            report: report_b,
            has_compare: true,
            baseline_available: true,
            markdown: "## bench-b\n\nResults: pass".to_string(),
            extras_prefix: "extras/bench-b".to_string(),
        };

        let (sensor_report, _md) =
            SensorReportBuilder::new(make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:32:00Z".to_string(), 120000)
                .engine(fixed_engine())
                .build_aggregated(&[outcome_a, outcome_b]);

        assert_json_snapshot!(sensor_report);
    }

    #[test]
    fn snapshot_truncated_report() {
        use perfgate_types::FindingData;

        let findings: Vec<ReportFinding> = (0..5)
            .map(|i| ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: format!("metric_{} regression", i),
                data: Some(FindingData {
                    metric_name: format!("metric_{}", i),
                    baseline: 100.0,
                    current: 150.0,
                    regression_pct: 50.0,
                    threshold: 0.2,
                    direction: perfgate_types::Direction::Lower,
                }),
            })
            .collect();

        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 5,
                },
                reasons: vec![],
            },
            compare: None,
            findings,
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 5,
                total_count: 5,
            },
        };

        let sensor_report =
            SensorReportBuilder::new(make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:31:00Z".to_string(), 60000)
                .baseline(true, None)
                .engine(fixed_engine())
                .max_findings(3)
                .build(&report);

        assert_json_snapshot!(sensor_report);
    }
}

#[cfg(test)]
mod schema_conformance_tests {
    use super::*;
    use perfgate_types::{
        FINDING_CODE_METRIC_FAIL, REPORT_SCHEMA_V1, ReportFinding, ReportSummary, Verdict,
        VerdictCounts,
    };

    fn make_tool_info() -> ToolInfo {
        ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    fn make_pass_report() -> PerfgateReport {
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 2,
                    warn: 0,
                    fail: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            summary: ReportSummary {
                pass_count: 2,
                warn_count: 0,
                fail_count: 0,
                total_count: 2,
            },
        }
    }

    fn make_fail_report_with_finding() -> PerfgateReport {
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 1,
                },
                reasons: vec!["wall_ms_fail".to_string()],
            },
            compare: None,
            findings: vec![ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: "wall_ms regression: +25.00%".to_string(),
                data: None,
            }],
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 1,
                total_count: 1,
            },
        }
    }

    /// Validate that a serialized SensorReport has the required top-level fields
    /// matching the vendored sensor.report.v1 schema.
    fn assert_schema_conformance(json: &serde_json::Value) {
        let obj = json.as_object().expect("report should be a JSON object");

        // Required fields per schema: schema, tool, run, verdict, findings, data
        assert!(obj.contains_key("schema"), "missing 'schema' field");
        assert!(obj.contains_key("tool"), "missing 'tool' field");
        assert!(obj.contains_key("run"), "missing 'run' field");
        assert!(obj.contains_key("verdict"), "missing 'verdict' field");
        assert!(obj.contains_key("findings"), "missing 'findings' field");
        assert!(obj.contains_key("data"), "missing 'data' field");

        // schema must be the const value
        assert_eq!(
            obj["schema"].as_str().unwrap(),
            "sensor.report.v1",
            "schema field must be 'sensor.report.v1'"
        );

        // tool must have name and version
        let tool = obj["tool"].as_object().expect("tool should be object");
        assert!(tool.contains_key("name"), "tool missing 'name'");
        assert!(tool.contains_key("version"), "tool missing 'version'");
        assert!(tool["name"].is_string());
        assert!(tool["version"].is_string());

        // run must have started_at and capabilities
        let run = obj["run"].as_object().expect("run should be object");
        assert!(run.contains_key("started_at"), "run missing 'started_at'");
        assert!(
            run.contains_key("capabilities"),
            "run missing 'capabilities'"
        );
        assert!(run["started_at"].is_string());
        assert!(run["capabilities"].is_object());

        // verdict must have status, counts, reasons
        let verdict = obj["verdict"]
            .as_object()
            .expect("verdict should be object");
        assert!(verdict.contains_key("status"), "verdict missing 'status'");
        assert!(verdict.contains_key("counts"), "verdict missing 'counts'");
        assert!(verdict.contains_key("reasons"), "verdict missing 'reasons'");

        let valid_statuses = ["pass", "warn", "fail", "skip"];
        let status = verdict["status"].as_str().unwrap();
        assert!(
            valid_statuses.contains(&status),
            "verdict.status '{}' not in {:?}",
            status,
            valid_statuses
        );

        let counts = verdict["counts"]
            .as_object()
            .expect("counts should be object");
        assert!(counts.contains_key("info"), "counts missing 'info'");
        assert!(counts.contains_key("warn"), "counts missing 'warn'");
        assert!(counts.contains_key("error"), "counts missing 'error'");
        assert!(counts["info"].as_u64().is_some());
        assert!(counts["warn"].as_u64().is_some());
        assert!(counts["error"].as_u64().is_some());

        assert!(verdict["reasons"].is_array());

        // findings must be an array
        assert!(obj["findings"].is_array());

        // data must be an object
        assert!(obj["data"].is_object());
    }

    /// Validate individual findings conform to the schema.
    fn assert_finding_conformance(finding: &serde_json::Value) {
        let obj = finding
            .as_object()
            .expect("finding should be a JSON object");

        // Required: check_id, code, severity, message
        assert!(obj.contains_key("check_id"), "finding missing 'check_id'");
        assert!(obj.contains_key("code"), "finding missing 'code'");
        assert!(obj.contains_key("severity"), "finding missing 'severity'");
        assert!(obj.contains_key("message"), "finding missing 'message'");

        assert!(obj["check_id"].is_string());
        assert!(obj["code"].is_string());
        assert!(obj["message"].is_string());

        let valid_severities = ["info", "warn", "error"];
        let severity = obj["severity"].as_str().unwrap();
        assert!(
            valid_severities.contains(&severity),
            "finding.severity '{}' not in {:?}",
            severity,
            valid_severities
        );

        // fingerprint, if present, must be 64-char hex
        if let Some(fp) = obj.get("fingerprint") {
            let fp_str = fp.as_str().expect("fingerprint should be string");
            assert_eq!(fp_str.len(), 64, "fingerprint should be 64-char hex");
            assert!(
                fp_str.chars().all(|c| c.is_ascii_hexdigit()),
                "fingerprint should contain only hex chars"
            );
        }

        // No additional properties beyond what the schema allows
        let allowed_keys = [
            "check_id",
            "code",
            "severity",
            "message",
            "fingerprint",
            "data",
        ];
        for key in obj.keys() {
            assert!(
                allowed_keys.contains(&key.as_str()),
                "unexpected finding field: '{}'",
                key
            );
        }
    }

    #[test]
    fn pass_report_conforms_to_vendored_schema() {
        let report = make_pass_report();
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
            .baseline(true, None)
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        assert_schema_conformance(&json);
        assert!(json["findings"].as_array().unwrap().is_empty());
    }

    #[test]
    fn fail_report_conforms_to_vendored_schema() {
        let report = make_fail_report_with_finding();
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
            .baseline(true, None)
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        assert_schema_conformance(&json);

        let findings = json["findings"].as_array().unwrap();
        assert!(!findings.is_empty());
        for finding in findings {
            assert_finding_conformance(finding);
        }
    }

    #[test]
    fn error_report_conforms_to_vendored_schema() {
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .ended_at("2024-01-01T00:00:01Z".to_string(), 1000)
            .baseline(false, None)
            .build_error(
                "config parse failed",
                perfgate_types::STAGE_CONFIG_PARSE,
                perfgate_types::ERROR_KIND_PARSE,
            );

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        assert_schema_conformance(&json);

        let findings = json["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_finding_conformance(&findings[0]);
        assert_eq!(findings[0]["severity"], "error");
    }

    #[test]
    fn report_with_artifacts_conforms_to_vendored_schema() {
        let report = make_pass_report();
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .artifact("report.json".to_string(), "sensor_report".to_string())
            .artifact("comment.md".to_string(), "markdown".to_string())
            .baseline(true, None)
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        assert_schema_conformance(&json);

        // artifacts, if present, must have path and type
        let artifacts = json["artifacts"].as_array().unwrap();
        assert_eq!(artifacts.len(), 2);
        for artifact in artifacts {
            let obj = artifact.as_object().unwrap();
            assert!(obj.contains_key("path"), "artifact missing 'path'");
            assert!(obj.contains_key("type"), "artifact missing 'type'");
            assert!(obj["path"].is_string());
            assert!(obj["type"].is_string());
        }
    }

    #[test]
    fn report_without_ended_at_conforms_to_vendored_schema() {
        let report = make_pass_report();
        // No ended_at or duration_ms — these are optional in the schema
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .baseline(true, None)
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        assert_schema_conformance(&json);

        let run = json["run"].as_object().unwrap();
        assert!(!run.contains_key("ended_at") || run["ended_at"].is_string());
        assert!(!run.contains_key("duration_ms") || run["duration_ms"].is_u64());
    }

    #[test]
    fn truncated_report_findings_all_conform() {
        use perfgate_types::FindingData;

        let findings: Vec<ReportFinding> = (0..10)
            .map(|i| ReportFinding {
                check_id: "perf.budget".to_string(),
                code: FINDING_CODE_METRIC_FAIL.to_string(),
                severity: Severity::Fail,
                message: format!("metric_{} regression", i),
                data: Some(FindingData {
                    metric_name: format!("metric_{}", i),
                    baseline: 100.0,
                    current: 150.0,
                    regression_pct: 50.0,
                    threshold: 0.2,
                    direction: perfgate_types::Direction::Lower,
                }),
            })
            .collect();

        let report = PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 10,
                },
                reasons: vec![],
            },
            compare: None,
            findings,
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 10,
                total_count: 10,
            },
        };

        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .baseline(true, None)
            .max_findings(5)
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        assert_schema_conformance(&json);

        let findings = json["findings"].as_array().unwrap();
        for finding in findings {
            assert_finding_conformance(finding);
        }
    }

    #[test]
    fn serialized_report_no_additional_top_level_properties() {
        let report = make_pass_report();
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
            .baseline(true, None)
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        let obj = json.as_object().unwrap();

        // Schema says additionalProperties: false
        let allowed_top_level = [
            "schema",
            "tool",
            "run",
            "verdict",
            "findings",
            "artifacts",
            "data",
        ];
        for key in obj.keys() {
            assert!(
                allowed_top_level.contains(&key.as_str()),
                "unexpected top-level field: '{}'",
                key
            );
        }
    }

    #[test]
    fn verdict_counts_are_non_negative_integers() {
        let report = make_fail_report_with_finding();
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .baseline(true, None)
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        let counts = &json["verdict"]["counts"];

        assert!(counts["info"].as_u64().is_some(), "info should be u64");
        assert!(counts["warn"].as_u64().is_some(), "warn should be u64");
        assert!(counts["error"].as_u64().is_some(), "error should be u64");
    }

    #[test]
    fn capabilities_baseline_has_valid_status() {
        let report = make_pass_report();
        let sensor = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
            .baseline(false, Some("no_baseline".to_string()))
            .build(&report);

        let json: serde_json::Value = serde_json::to_value(&sensor).unwrap();
        let caps = &json["run"]["capabilities"];
        let baseline = caps["baseline"].as_object().unwrap();
        let status = baseline["status"].as_str().unwrap();
        let valid = ["available", "unavailable", "skipped"];
        assert!(
            valid.contains(&status),
            "capability status '{}' not in {:?}",
            status,
            valid
        );
    }
}
