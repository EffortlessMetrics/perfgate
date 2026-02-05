//! Conversion from PerfgateReport to SensorReport envelope.
//!
//! This module provides functionality for wrapping a PerfgateReport into
//! a `sensor.report.v1` envelope suitable for cockpit integration.

use perfgate_types::{
    Capability, CapabilityStatus, PerfgateReport, SensorArtifact, SensorCapabilities,
    SensorFinding, SensorReport, SensorRunMeta, SensorSeverity, SensorVerdict, SensorVerdictCounts,
    SensorVerdictStatus, Severity, ToolInfo, VerdictStatus, CHECK_ID_TOOL_RUNTIME,
    FINDING_CODE_RUNTIME_ERROR, SENSOR_REPORT_SCHEMA_V1, VERDICT_REASON_TOOL_ERROR,
};

/// Builder for constructing a SensorReport from a PerfgateReport.
pub struct SensorReportBuilder {
    tool: ToolInfo,
    started_at: String,
    ended_at: Option<String>,
    duration_ms: Option<u64>,
    baseline_available: bool,
    baseline_reason: Option<String>,
    artifacts: Vec<SensorArtifact>,
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

        let verdict = SensorVerdict {
            status,
            counts,
            reasons: report.verdict.reasons.clone(),
        };

        // Map findings: Severity::Fail -> SensorSeverity::Error
        let findings: Vec<SensorFinding> = report
            .findings
            .iter()
            .map(|f| SensorFinding {
                check_id: f.check_id.clone(),
                code: f.code.clone(),
                severity: match f.severity {
                    Severity::Warn => SensorSeverity::Warn,
                    Severity::Fail => SensorSeverity::Error,
                },
                message: f.message.clone(),
                data: f.data.as_ref().and_then(|d| serde_json::to_value(d).ok()),
            })
            .collect();

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
        let data = serde_json::json!({
            "summary": {
                "pass_count": report.summary.pass_count,
                "warn_count": report.summary.warn_count,
                "fail_count": report.summary.fail_count,
                "total_count": report.summary.total_count,
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
}
