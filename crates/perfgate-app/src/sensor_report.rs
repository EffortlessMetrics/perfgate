//! Conversion from PerfgateReport to SensorReport envelope.
//!
//! This module provides functionality for wrapping a PerfgateReport into
//! a `sensor.report.v1` envelope suitable for cockpit integration.
//!
//! Also provides `run_sensor_check()`, a library-linkable convenience function
//! so the cockpit binary can `use perfgate_app::run_sensor_check`.

use crate::{CheckRequest, CheckUseCase, Clock};
use perfgate_adapters::{sha256_hex, HostProbe, ProcessRunner};
use perfgate_types::{
    validate_bench_name, Capability, CapabilityStatus, ConfigFile, HostMismatchPolicy,
    PerfgateReport, RunReceipt, SensorArtifact, SensorCapabilities, SensorFinding, SensorReport,
    SensorRunMeta, SensorSeverity, SensorVerdict, SensorVerdictCounts, SensorVerdictStatus,
    Severity, ToolInfo, VerdictStatus, BASELINE_REASON_NO_BASELINE, CHECK_ID_TOOL_RUNTIME,
    CHECK_ID_TOOL_TRUNCATION, ERROR_KIND_EXEC, ERROR_KIND_IO, ERROR_KIND_PARSE,
    FINDING_CODE_RUNTIME_ERROR, FINDING_CODE_TRUNCATED, MAX_FINDINGS_DEFAULT,
    SENSOR_REPORT_SCHEMA_V1, STAGE_BASELINE_RESOLVE, STAGE_CONFIG_PARSE, STAGE_RUN_COMMAND,
    STAGE_WRITE_ARTIFACTS, VERDICT_REASON_TOOL_ERROR, VERDICT_REASON_TRUNCATED,
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
    let msg = format!("{:#}", err);
    let msg_lower = msg.to_lowercase();

    if msg_lower.contains("parse")
        && (msg_lower.contains("config")
            || msg_lower.contains("toml")
            || msg_lower.contains("json config"))
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

/// Builder for constructing a SensorReport from a PerfgateReport.
pub struct SensorReportBuilder {
    tool: ToolInfo,
    started_at: String,
    ended_at: Option<String>,
    duration_ms: Option<u64>,
    baseline_available: bool,
    baseline_reason: Option<String>,
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
        let mut truncation_totals: Option<(usize, usize)> = None;
        if let Some(limit) = self.max_findings {
            if findings.len() > limit {
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
                        &self.tool.name,
                        CHECK_ID_TOOL_TRUNCATION,
                        FINDING_CODE_TRUNCATED,
                    ])),
                    data: Some(serde_json::json!({
                        "total_findings": total,
                        "shown_findings": shown,
                    })),
                });
                reasons.push(VERDICT_REASON_TRUNCATED.to_string());
                truncation_totals = Some((total, shown));
            }
        }

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        ReportFinding, ReportSummary, Verdict, VerdictCounts, ERROR_KIND_PARSE,
        FINDING_CODE_METRIC_FAIL, FINDING_CODE_METRIC_WARN, REPORT_SCHEMA_V1, STAGE_CONFIG_PARSE,
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
}
