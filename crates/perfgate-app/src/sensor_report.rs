//! Conversion from PerfgateReport to SensorReport envelope.
//!
//! This module provides functionality for wrapping a PerfgateReport into
//! a `sensor.report.v1` envelope suitable for cockpit integration.
//!
//! Also provides `run_sensor_check()`, a library-linkable convenience function
//! so the cockpit binary can `use perfgate_app::run_sensor_check`.

use crate::{CheckRequest, CheckUseCase, Clock};
use perfgate_adapters::AdapterError;
use perfgate_adapters::{sha256_hex, HostProbe, ProcessRunner};
use perfgate_types::{
    validate_bench_name, Capability, CapabilityStatus, ConfigFile, ConfigValidationError,
    HostMismatchPolicy, PerfgateError, PerfgateReport, RunReceipt, SensorArtifact,
    SensorCapabilities, SensorFinding, SensorReport, SensorRunMeta, SensorSeverity, SensorVerdict,
    SensorVerdictCounts, SensorVerdictStatus, Severity, ToolInfo, VerdictStatus,
    BASELINE_REASON_NO_BASELINE, CHECK_ID_TOOL_RUNTIME, CHECK_ID_TOOL_TRUNCATION, ERROR_KIND_EXEC,
    ERROR_KIND_IO, ERROR_KIND_PARSE, FINDING_CODE_RUNTIME_ERROR, FINDING_CODE_TRUNCATED,
    MAX_FINDINGS_DEFAULT, SENSOR_REPORT_SCHEMA_V1, STAGE_BASELINE_RESOLVE, STAGE_CONFIG_PARSE,
    STAGE_RUN_COMMAND, STAGE_WRITE_ARTIFACTS, VERDICT_REASON_TOOL_ERROR, VERDICT_REASON_TRUNCATED,
};

/// Options for `run_sensor_check`.
#[derive(Debug, Clone)]
pub struct SensorCheckOptions {
    pub require_baseline: bool,
    pub fail_on_warn: bool,
    pub env: Vec<(String, String)>,
    pub output_cap_bytes: usize,
    pub allow_nonzero: bool,
    pub host_mismatch_policy: HostMismatchPolicy,
    pub max_findings: Option<usize>,
}

impl Default for SensorCheckOptions {
    fn default() -> Self {
        Self {
            require_baseline: false,
            fail_on_warn: false,
            env: Vec::new(),
            output_cap_bytes: 8192,
            allow_nonzero: false,
            host_mismatch_policy: HostMismatchPolicy::Warn,
            max_findings: Some(MAX_FINDINGS_DEFAULT),
        }
    }
}

/// Run a sensor check and return a `SensorReport` directly.
///
/// This is the library convenience API for cockpit linking. It delegates to
/// `CheckUseCase::execute()`, wraps the result in a `SensorReportBuilder`,
/// and catches errors to produce an error report.
///
/// Returns `SensorReport` directly — no I/O, no file writing.
pub fn run_sensor_check<R, H, C>(
    runner: &R,
    host_probe: &H,
    clock: &C,
    config: &ConfigFile,
    bench_name: &str,
    baseline: Option<&RunReceipt>,
    tool: ToolInfo,
    options: SensorCheckOptions,
) -> SensorReport
where
    R: ProcessRunner + Clone,
    H: HostProbe + Clone,
    C: Clock + Clone,
{
    let started_at = clock.now_rfc3339();
    let start_instant = std::time::Instant::now();

    // Validate bench name early — invalid names produce an error report.
    if let Err(msg) = validate_bench_name(bench_name) {
        let ended_at = clock.now_rfc3339();
        let duration_ms = start_instant.elapsed().as_millis() as u64;
        let builder = SensorReportBuilder::new(tool, started_at)
            .ended_at(ended_at, duration_ms)
            .baseline(baseline.is_some(), None);
        return builder.build_error(&msg, STAGE_CONFIG_PARSE, ERROR_KIND_PARSE);
    }

    // Validate config (covers other bench names in the file) — defense-in-depth
    // so library callers can't bypass config-level validation.
    if let Err(msg) = config.validate() {
        let ended_at = clock.now_rfc3339();
        let duration_ms = start_instant.elapsed().as_millis() as u64;
        let builder = SensorReportBuilder::new(tool, started_at)
            .ended_at(ended_at, duration_ms)
            .baseline(baseline.is_some(), None);
        return builder.build_error(
            &format!("config validation: {}", msg),
            STAGE_CONFIG_PARSE,
            ERROR_KIND_PARSE,
        );
    }

    let baseline_available = baseline.is_some();

    let result = CheckUseCase::new(runner.clone(), host_probe.clone(), clock.clone()).execute(
        CheckRequest {
            config: config.clone(),
            bench_name: bench_name.to_string(),
            out_dir: std::path::PathBuf::from("."), // artifacts not written
            baseline: baseline.cloned(),
            baseline_path: None,
            require_baseline: options.require_baseline,
            fail_on_warn: options.fail_on_warn,
            tool: tool.clone(),
            env: options.env.clone(),
            output_cap_bytes: options.output_cap_bytes,
            allow_nonzero: options.allow_nonzero,
            host_mismatch_policy: options.host_mismatch_policy,
        },
    );

    let ended_at = clock.now_rfc3339();
    let duration_ms = start_instant.elapsed().as_millis() as u64;

    let baseline_reason = if !baseline_available {
        Some(BASELINE_REASON_NO_BASELINE.to_string())
    } else {
        None
    };

    match result {
        Ok(outcome) => {
            let mut builder = SensorReportBuilder::new(tool, started_at)
                .ended_at(ended_at, duration_ms)
                .baseline(baseline_available, baseline_reason);

            if let Some(limit) = options.max_findings {
                builder = builder.max_findings(limit);
            }

            builder.build(&outcome.report)
        }
        Err(err) => {
            let (stage, error_kind) = classify_error(&err);
            let builder = SensorReportBuilder::new(tool, started_at)
                .ended_at(ended_at, duration_ms)
                .baseline(baseline_available, baseline_reason);

            builder.build_error(&format!("{:#}", err), stage, error_kind)
        }
    }
}

/// Classify an error into (stage, error_kind) for structured error reporting.
pub fn classify_error(err: &anyhow::Error) -> (&'static str, &'static str) {
    // Structural classification — preferred over string matching.
    if err.downcast_ref::<ConfigValidationError>().is_some() {
        return (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE);
    }

    if let Some(pe) = err.downcast_ref::<PerfgateError>() {
        return match pe {
            PerfgateError::BaselineResolve(_) => (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO),
            PerfgateError::ArtifactWrite(_) => (STAGE_WRITE_ARTIFACTS, ERROR_KIND_IO),
            PerfgateError::RunCommand(_) => (STAGE_RUN_COMMAND, ERROR_KIND_EXEC),
        };
    }

    // Walk the error chain for AdapterError.
    if let Some(ae) = err.downcast_ref::<AdapterError>() {
        return match ae {
            AdapterError::EmptyArgv | AdapterError::Timeout | AdapterError::TimeoutUnsupported => {
                (STAGE_RUN_COMMAND, ERROR_KIND_EXEC)
            }
            AdapterError::Other(_) => (STAGE_RUN_COMMAND, ERROR_KIND_IO),
        };
    }

    // Walk the chain for DomainError.
    if err.downcast_ref::<perfgate_domain::DomainError>().is_some() {
        return (STAGE_RUN_COMMAND, ERROR_KIND_EXEC);
    }

    // Fallback: string heuristics for errors not yet converted to typed errors.
    let msg = format!("{:#}", err);
    let msg_lower = msg.to_lowercase();

    if msg_lower.contains("bench name") || msg_lower.contains("config validation") {
        (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE)
    } else if msg_lower.contains("parse")
        && (msg_lower.contains("toml")
            || msg_lower.contains("json config")
            || msg_lower.contains("config"))
    {
        (STAGE_CONFIG_PARSE, ERROR_KIND_PARSE)
    } else if msg_lower.contains("baseline") || msg_lower.contains("not found") {
        (STAGE_BASELINE_RESOLVE, ERROR_KIND_IO)
    } else if msg_lower.contains("failed to run")
        || msg_lower.contains("spawn")
        || msg_lower.contains("exec")
    {
        (STAGE_RUN_COMMAND, ERROR_KIND_EXEC)
    } else if msg_lower.contains("write")
        || msg_lower.contains("create dir")
        || msg_lower.contains("rename")
    {
        (STAGE_WRITE_ARTIFACTS, ERROR_KIND_IO)
    } else if err.downcast_ref::<std::io::Error>().is_some() {
        (STAGE_RUN_COMMAND, ERROR_KIND_IO)
    } else {
        (STAGE_RUN_COMMAND, ERROR_KIND_EXEC)
    }
}

/// Build a fleet-standard fingerprint from semantic parts.
/// Trims trailing empty parts, joins with `|`, returns SHA-256 hex.
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
/// On Unix: `Available` (cpu_ms, max_rss_kb via wait4).
/// On non-Unix: `Unavailable` with reason `"platform_limited"`.
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
    pub fn bench_name(&self) -> &str {
        match self {
            BenchOutcome::Success { bench_name, .. } => bench_name,
            BenchOutcome::Error { bench_name, .. } => bench_name,
        }
    }
}

/// Builder for constructing a SensorReport from a PerfgateReport.
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
    pub fn ended_at(mut self, ended_at: String, duration_ms: u64) -> Self {
        self.ended_at = Some(ended_at);
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Set baseline availability.
    pub fn baseline(mut self, available: bool, reason: Option<String>) -> Self {
        self.baseline_available = available;
        self.baseline_reason = reason;
        self
    }

    /// Set engine capability explicitly.
    pub fn engine(mut self, capability: Capability) -> Self {
        self.engine_capability = Some(capability);
        self
    }

    /// Add an artifact.
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
    pub fn max_findings(mut self, limit: usize) -> Self {
        self.max_findings = Some(limit);
        self
    }

    /// Take ownership of accumulated artifacts (for manual report building).
    pub fn take_artifacts(&mut self) -> Vec<SensorArtifact> {
        std::mem::take(&mut self.artifacts)
    }

    /// Build the SensorReport from a PerfgateReport.
    pub fn build(mut self, report: &PerfgateReport) -> SensorReport {
        // Map VerdictStatus -> SensorVerdictStatus
        let status = match report.verdict.status {
            VerdictStatus::Pass => SensorVerdictStatus::Pass,
            VerdictStatus::Warn => SensorVerdictStatus::Warn,
            VerdictStatus::Fail => SensorVerdictStatus::Fail,
        };

        // Map counts: fail -> error (cockpit vocabulary)
        let counts = SensorVerdictCounts {
            info: report.summary.pass_count,
            warn: report.summary.warn_count,
            error: report.summary.fail_count,
        };

        let mut reasons = report.verdict.reasons.clone();

        // Map findings: Severity::Fail -> SensorSeverity::Error
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

        // Apply truncation if configured
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

        // Build capabilities
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

        // Build run metadata
        let run = SensorRunMeta {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            capabilities,
        };

        // Build data section (summary only, no compare receipt)
        let mut data = serde_json::json!({
            "summary": {
                "pass_count": report.summary.pass_count,
                "warn_count": report.summary.warn_count,
                "fail_count": report.summary.fail_count,
                "total_count": report.summary.total_count,
                "bench_count": 1,
            }
        });

        // findings_total: total real findings before truncation (excludes the truncation meta-finding)
        // findings_emitted: number of real findings preserved after truncation (excludes the truncation meta-finding)
        if let Some((total, emitted)) = truncation_totals {
            data["findings_total"] = serde_json::json!(total);
            data["findings_emitted"] = serde_json::json!(emitted);
        }

        // Sort artifacts by (type, path)
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

        // Sort artifacts by (type, path)
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
                    // Map findings from this bench's report
                    for f in &report.findings {
                        let severity = match f.severity {
                            Severity::Warn => SensorSeverity::Warn,
                            Severity::Fail => SensorSeverity::Error,
                        };
                        let mut finding_data =
                            f.data.as_ref().and_then(|d| serde_json::to_value(d).ok());
                        // In multi-bench mode, include bench name in finding data
                        if multi_bench {
                            if let Some(ref mut val) = finding_data {
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

                    // Aggregate counts
                    total_info += report.summary.pass_count;
                    total_warn += report.summary.warn_count;
                    total_error += report.summary.fail_count;

                    // Aggregate worst verdict status
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

                    // Aggregate reasons (union)
                    for reason in &report.verdict.reasons {
                        if !all_reasons.contains(reason) {
                            all_reasons.push(reason.clone());
                        }
                    }

                    // Add per-bench artifacts
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

                    // Collect markdown
                    if multi_bench && !combined_markdown.is_empty() {
                        combined_markdown.push_str("\n---\n\n");
                    }
                    combined_markdown.push_str(markdown);

                    // Baseline reason per bench
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
                    // Create error finding directly (like build_error does)
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

                    // Error contributes to counts
                    total_error += 1;
                    worst_status = SensorVerdictStatus::Fail;
                    if !all_reasons.contains(&VERDICT_REASON_TOOL_ERROR.to_string()) {
                        all_reasons.push(VERDICT_REASON_TOOL_ERROR.to_string());
                    }

                    // Error markdown
                    if multi_bench && !combined_markdown.is_empty() {
                        combined_markdown.push_str("\n---\n\n");
                    }
                    combined_markdown.push_str(&format!(
                        "## {}\n\n**Error:** {}\n",
                        bench_name, error_message
                    ));

                    // Error bench has no baseline
                    if !all_reasons.contains(&BASELINE_REASON_NO_BASELINE.to_string()) {
                        all_reasons.push(BASELINE_REASON_NO_BASELINE.to_string());
                    }
                    // No artifact registration for errored benches
                }
            }
        }

        self.artifacts.push(SensorArtifact {
            path: "comment.md".to_string(),
            artifact_type: "markdown".to_string(),
        });

        // Apply truncation to aggregated findings
        let limit = self.max_findings.unwrap_or(MAX_FINDINGS_DEFAULT);
        let truncation_totals = truncate_findings(
            &mut aggregated_findings,
            &mut all_reasons,
            limit,
            &self.tool.name,
        );

        // Build aggregated data section
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

        // Build capabilities
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

        // Sort artifacts by (type, path)
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
        PerfgateError, ReportFinding, ReportSummary, Verdict, VerdictCounts, ERROR_KIND_EXEC,
        ERROR_KIND_PARSE, FINDING_CODE_METRIC_FAIL, FINDING_CODE_METRIC_WARN, REPORT_SCHEMA_V1,
        STAGE_CONFIG_PARSE, STAGE_RUN_COMMAND,
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
        // Data should only have summary, no compare key
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
        // Severity::Fail -> SensorSeverity::Error
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
        // Sorted by (type, path): markdown < run_receipt
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
            STAGE_CONFIG_PARSE,
            ERROR_KIND_PARSE,
        );

        assert_eq!(sensor_report.schema, SENSOR_REPORT_SCHEMA_V1);
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(sensor_report.verdict.counts.error, 1);
        assert_eq!(sensor_report.verdict.reasons, vec!["tool_error"]);
        assert_eq!(sensor_report.findings.len(), 1);
        assert_eq!(sensor_report.findings[0].check_id, "tool.runtime");
        assert_eq!(sensor_report.findings[0].code, "runtime_error");
        assert!(sensor_report.findings[0]
            .message
            .contains("config file not found"));
        // Verify structured data
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
        // No FindingData on this report finding, so metric_name is ""
        // Trailing empty trimmed: "perfgate|perf.budget|metric_fail"
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
            STAGE_CONFIG_PARSE,
            ERROR_KIND_PARSE,
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

        // Only 1 finding, well under the limit of 100
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

        // Build a report with 5 findings
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
                .max_findings(3); // Limit to 3

        let sensor_report = builder.build(&report);

        // Should have 3 findings: 2 original + 1 truncation meta-finding
        assert_eq!(sensor_report.findings.len(), 3);

        // Last finding should be the truncation indicator
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

        // Verify truncation data
        let data = last.data.as_ref().unwrap();
        assert_eq!(data["total_findings"], 5);
        assert_eq!(data["shown_findings"], 2);

        // Verify verdict reason includes "truncated"
        assert!(
            sensor_report
                .verdict
                .reasons
                .contains(&"truncated".to_string()),
            "verdict.reasons should contain 'truncated'"
        );

        // Verify report-level truncation totals in data
        assert_eq!(sensor_report.data["findings_total"], 5);
        assert_eq!(sensor_report.data["findings_emitted"], 2);
    }

    #[test]
    fn test_truncation_meta_finding_structure() {
        use perfgate_types::FindingData;

        // Build a report with 10 findings, limit to 5
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

        // 4 original + 1 truncation = 5
        assert_eq!(sensor_report.findings.len(), 5);

        let meta = &sensor_report.findings[4];
        assert!(meta.message.contains("Showing 4 of 10"));
        assert!(meta.message.contains("6 omitted"));

        // Verify verdict reason includes "truncated"
        assert!(
            sensor_report
                .verdict
                .reasons
                .contains(&"truncated".to_string()),
            "verdict.reasons should contain 'truncated'"
        );

        // Verify report-level truncation totals in data
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

        // Serialize
        let json = serde_json::to_string(&sensor_report).expect("should serialize");

        // Deserialize
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
        let sensor_report = builder.build_error(msg, STAGE_CONFIG_PARSE, ERROR_KIND_PARSE);

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

    #[test]
    fn test_classify_error_config_parse() {
        let err = anyhow::anyhow!("parse TOML config perfgate.toml: expected `=`");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_baseline_resolve() {
        let err = anyhow::anyhow!("baseline file not found");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_BASELINE_RESOLVE);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_default_exec() {
        let err = anyhow::anyhow!("something unexpected happened");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_spawn_failure() {
        let err = anyhow::anyhow!("failed to run command: spawn error");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_exec_failure() {
        let err = anyhow::anyhow!("exec failed for process");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_write_artifacts() {
        let err = anyhow::anyhow!("write output file failed");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_create_dir() {
        let err = anyhow::anyhow!("create dir /tmp/out: permission denied");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_rename() {
        let err = anyhow::anyhow!("rename temp file: cross-device link");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_io_downcast() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file gone");
        let err: anyhow::Error = io_err.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_json_config() {
        let err = anyhow::anyhow!("parse JSON config perfgate.json: unexpected token");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_generic_config_parse() {
        let err = anyhow::anyhow!("parse config perfgate");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_bench_name_validation() {
        let err = anyhow::anyhow!(
            "bench name \"../evil\" contains a \"..\" path segment (path traversal is forbidden)"
        );
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation() {
        let err =
            anyhow::anyhow!("config validation: bench name \"Bad\" contains invalid characters");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation_without_bench_name() {
        let err = anyhow::anyhow!("config validation: some future validation error");
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation_typed_config_file() {
        let err: anyhow::Error = ConfigValidationError::ConfigFile(
            "bench name \"Bad\" contains invalid characters".to_string(),
        )
        .into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    #[test]
    fn test_classify_error_config_validation_typed_bench_name() {
        let err: anyhow::Error =
            ConfigValidationError::BenchName("bench name must not be empty".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
    }

    // --- Typed PerfgateError downcast tests ---

    #[test]
    fn test_classify_error_typed_baseline_resolve() {
        let err: anyhow::Error =
            PerfgateError::BaselineResolve("file not found".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_BASELINE_RESOLVE);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_typed_artifact_write() {
        let err: anyhow::Error =
            PerfgateError::ArtifactWrite("permission denied".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_WRITE_ARTIFACTS);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    #[test]
    fn test_classify_error_typed_run_command() {
        let err: anyhow::Error = PerfgateError::RunCommand("spawn failed".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    // --- AdapterError downcast tests ---

    #[test]
    fn test_classify_error_adapter_empty_argv() {
        let err: anyhow::Error = AdapterError::EmptyArgv.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_adapter_timeout() {
        let err: anyhow::Error = AdapterError::Timeout.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_adapter_timeout_unsupported() {
        let err: anyhow::Error = AdapterError::TimeoutUnsupported.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
    }

    #[test]
    fn test_classify_error_adapter_other() {
        let inner = anyhow::anyhow!("some IO problem");
        let err: anyhow::Error = AdapterError::Other(inner).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_IO);
    }

    // --- DomainError downcast test ---

    #[test]
    fn test_classify_error_domain_no_samples() {
        let err: anyhow::Error = perfgate_domain::DomainError::NoSamples.into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_RUN_COMMAND);
        assert_eq!(kind, ERROR_KIND_EXEC);
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
            STAGE_CONFIG_PARSE,
            ERROR_KIND_PARSE,
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

    // --- Bench-not-found misclassification bug regression test ---

    #[test]
    fn test_classify_error_bench_not_found_typed() {
        // This was previously misclassified as (baseline_resolve, io_error)
        // because "not found" matched the string heuristic.
        let err: anyhow::Error =
            ConfigValidationError::BenchName("bench 'xyz' not found in config".to_string()).into();
        let (stage, kind) = classify_error(&err);
        assert_eq!(stage, STAGE_CONFIG_PARSE);
        assert_eq!(kind, ERROR_KIND_PARSE);
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

        // Single bench: findings should NOT be prefixed
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
        // Finding data should have bench_name
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
        let report_a = make_fail_report(); // pass=1, warn=0, fail=1
        let report_b = make_warn_report(); // pass=1, warn=1, fail=0
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
        // Both benches have no baseline → should get one `no_baseline` reason
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

        // Create a report with many findings
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

        // 4 real + 1 truncation = 5
        assert_eq!(sensor_report.findings.len(), 5);
        assert_eq!(sensor_report.findings[4].check_id, CHECK_ID_TOOL_TRUNCATION);
        assert_eq!(sensor_report.data["findings_total"], 10);
        assert_eq!(sensor_report.data["findings_emitted"], 4);
        assert!(sensor_report
            .verdict
            .reasons
            .contains(&"truncated".to_string()));
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

        // Verify sorted by (type, path)
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
            STAGE_RUN_COMMAND,
            ERROR_KIND_EXEC,
        );

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        // Worst-wins: error bench → fail
        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);

        // Counts: bench-a has pass=1, warn=1, fail=0; bench-b error adds error=1
        assert_eq!(sensor_report.verdict.counts.info, 1);
        assert_eq!(sensor_report.verdict.counts.warn, 1);
        assert_eq!(sensor_report.verdict.counts.error, 1);

        // Reasons: should have tool_error and no_baseline
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

        // Findings: 1 from bench-a (warn) + 1 from bench-b (error)
        assert_eq!(sensor_report.findings.len(), 2);
        assert!(sensor_report.findings[0].message.starts_with("[bench-a]"));
        assert!(sensor_report.findings[1].message.starts_with("[bench-b]"));
        assert_eq!(sensor_report.findings[1].check_id, CHECK_ID_TOOL_RUNTIME);

        // bench_count includes both
        assert_eq!(sensor_report.data["summary"]["bench_count"], 2);

        // Markdown should have error section
        assert!(md.contains("bench-b"));
        assert!(md.contains("**Error:**"));
    }

    #[test]
    fn test_build_aggregated_single_error_outcome() {
        let outcome = make_error_outcome(
            "bench-a",
            "config parse failure",
            STAGE_CONFIG_PARSE,
            ERROR_KIND_PARSE,
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
        let outcome_b =
            make_error_outcome("bench-b", "spawn error", STAGE_RUN_COMMAND, ERROR_KIND_EXEC);

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        // bench-a should have artifacts, bench-b should NOT
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
        let outcome =
            make_error_outcome("bench-x", "spawn error", STAGE_RUN_COMMAND, ERROR_KIND_EXEC);

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome]);

        let finding = &sensor_report.findings[0];
        let data = finding.data.as_ref().expect("finding should have data");
        assert_eq!(data["stage"], STAGE_RUN_COMMAND);
        assert_eq!(data["error_kind"], ERROR_KIND_EXEC);
        // Single bench → no bench_name in data (not multi_bench)
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
        let outcome_b =
            make_error_outcome("bench-b", "spawn error", STAGE_RUN_COMMAND, ERROR_KIND_EXEC);

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());

        let (sensor_report, _md) = builder.build_aggregated(&[outcome_a, outcome_b]);

        // Find the error finding
        let error_finding = sensor_report
            .findings
            .iter()
            .find(|f| f.check_id == CHECK_ID_TOOL_RUNTIME)
            .expect("should have error finding");

        let data = error_finding
            .data
            .as_ref()
            .expect("finding should have data");
        assert_eq!(data["stage"], STAGE_RUN_COMMAND);
        assert_eq!(data["error_kind"], ERROR_KIND_EXEC);
        assert_eq!(data["bench_name"], "bench-b");
    }

    #[test]
    fn test_bench_outcome_bench_name() {
        let success = make_bench_outcome("my-bench", make_pass_report(), false, false, "extras");
        assert_eq!(success.bench_name(), "my-bench");

        let error = make_error_outcome("bad-bench", "error", STAGE_RUN_COMMAND, ERROR_KIND_EXEC);
        assert_eq!(error.bench_name(), "bad-bench");
    }
}
