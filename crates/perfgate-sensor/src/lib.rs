//! Cockpit mode and sensor report generation.
//!
//! This crate provides functionality for generating `sensor.report.v1` JSON
//! envelopes compatible with external performance monitoring tools like Cockpit.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

use perfgate_sha256::sha256_hex;
pub use perfgate_types::{
    Capability, CapabilityStatus, PerfgateReport, SENSOR_REPORT_SCHEMA_V1, SensorArtifact,
    SensorCapabilities, SensorFinding, SensorReport, SensorRunMeta, SensorSeverity, SensorVerdict,
    SensorVerdictCounts, SensorVerdictStatus, Severity, ToolInfo,
};

/// Error code for tool-level truncation.
pub const FINDING_CODE_TRUNCATED: &str = "tool_truncation";

/// Check ID for tool-level truncation findings.
pub const CHECK_ID_TOOL_TRUNCATION: &str = "perfgate.tool";

/// Builder for creating SensorReports.
pub struct SensorReportBuilder {
    tool: ToolInfo,
    started_at: String,
    ended_at: Option<String>,
    duration_ms: Option<u64>,
    baseline_available: bool,
    baseline_reason: Option<String>,
    max_findings: usize,
    artifacts: Vec<SensorArtifact>,
}

impl SensorReportBuilder {
    pub fn new(tool: ToolInfo, started_at: String) -> Self {
        Self {
            tool,
            started_at,
            ended_at: None,
            duration_ms: None,
            baseline_available: false,
            baseline_reason: None,
            max_findings: 100,
            artifacts: Vec::new(),
        }
    }

    pub fn ended_at(mut self, ended_at: String, duration_ms: u64) -> Self {
        self.ended_at = Some(ended_at);
        self.duration_ms = Some(duration_ms);
        self
    }

    pub fn baseline(mut self, available: bool, reason: Option<String>) -> Self {
        self.baseline_available = available;
        self.baseline_reason = reason;
        self
    }

    pub fn max_findings(mut self, limit: usize) -> Self {
        self.max_findings = limit;
        self
    }

    pub fn artifact(mut self, path: String, artifact_type: String) -> Self {
        self.artifacts.push(SensorArtifact {
            path,
            artifact_type,
        });
        self
    }

    /// Build a single report from a PerfgateReport.
    pub fn build(self, report: &PerfgateReport) -> SensorReport {
        let status = match report.verdict.status {
            perfgate_types::VerdictStatus::Pass => SensorVerdictStatus::Pass,
            perfgate_types::VerdictStatus::Warn => SensorVerdictStatus::Warn,
            perfgate_types::VerdictStatus::Fail => SensorVerdictStatus::Fail,
            perfgate_types::VerdictStatus::Skip => SensorVerdictStatus::Skip,
        };

        let counts = SensorVerdictCounts {
            info: report.summary.pass_count,
            warn: report.summary.warn_count,
            error: report.summary.fail_count,
        };

        let mut findings: Vec<SensorFinding> = report
            .findings
            .iter()
            .map(|f| SensorFinding {
                check_id: f.check_id.clone(),
                code: f.code.clone(),
                severity: map_severity(f.severity),
                message: f.message.clone(),
                fingerprint: None,
                data: f.data.as_ref().map(|d| serde_json::to_value(d).unwrap()),
            })
            .collect();

        findings.sort_by(|a, b| a.message.cmp(&b.message));

        for f in &mut findings {
            f.fingerprint = Some(sha256_hex(format!("{}:{}", f.check_id, f.code).as_bytes()));
        }

        if findings.len() > self.max_findings {
            truncate_findings(&mut findings, self.max_findings);
        }

        let run = SensorRunMeta {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            capabilities: SensorCapabilities {
                baseline: if self.baseline_available {
                    Capability {
                        status: CapabilityStatus::Available,
                        reason: None,
                    }
                } else {
                    Capability {
                        status: CapabilityStatus::Unavailable,
                        reason: self.baseline_reason.clone(),
                    }
                },
                engine: None,
            },
        };

        let mut artifacts = self.artifacts;
        artifacts.sort_by(|a, b| (&a.artifact_type, &a.path).cmp(&(&b.artifact_type, &b.path)));

        SensorReport {
            schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: self.tool,
            run,
            verdict: SensorVerdict {
                status,
                counts,
                reasons: report.verdict.reasons.clone(),
            },
            findings,
            artifacts,
            data: serde_json::json!({
                "summary": {
                    "bench_count": 1,
                    "pass_count": report.summary.pass_count,
                    "warn_count": report.summary.warn_count,
                    "fail_count": report.summary.fail_count,
                    "total_count": report.summary.pass_count + report.summary.warn_count + report.summary.fail_count,
                }
            }),
        }
    }

    /// Build a report representing a tool error.
    pub fn build_error(self, message: &str, stage: &str, code: &str) -> SensorReport {
        let findings = vec![SensorFinding {
            check_id: perfgate_types::CHECK_ID_TOOL_RUNTIME.to_string(),
            code: perfgate_types::FINDING_CODE_RUNTIME_ERROR.to_string(),
            severity: SensorSeverity::Error,
            message: message.to_string(),
            fingerprint: Some(sha256_hex(
                format!(
                    "{}:{}",
                    perfgate_types::CHECK_ID_TOOL_RUNTIME,
                    perfgate_types::FINDING_CODE_RUNTIME_ERROR
                )
                .as_bytes(),
            )),
            data: Some(serde_json::json!({
                "stage": stage,
                "error_kind": code,
            })),
        }];

        let run = SensorRunMeta {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            capabilities: SensorCapabilities {
                baseline: if self.baseline_available {
                    Capability {
                        status: CapabilityStatus::Available,
                        reason: None,
                    }
                } else {
                    Capability {
                        status: CapabilityStatus::Unavailable,
                        reason: self.baseline_reason.clone(),
                    }
                },
                engine: None,
            },
        };

        SensorReport {
            schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: self.tool,
            run,
            verdict: SensorVerdict {
                status: SensorVerdictStatus::Fail,
                counts: SensorVerdictCounts {
                    info: 0,
                    warn: 0,
                    error: 1,
                },
                reasons: vec!["tool_error".to_string()],
            },
            findings,
            artifacts: self.artifacts,
            data: serde_json::json!({ "error": message }),
        }
    }

    /// Build an aggregated report from multiple PerfgateReports.
    pub fn build_aggregated(self, outcomes: &[BenchOutcome]) -> (SensorReport, String) {
        let mut findings = Vec::new();
        let mut pass_count = 0;
        let mut warn_count = 0;
        let mut fail_count = 0;
        let mut all_reasons = Vec::new();
        let mut worst_status = SensorVerdictStatus::Skip;
        let mut all_markdown = String::new();
        let mut artifacts = self.artifacts;

        for outcome in outcomes {
            match outcome {
                BenchOutcome::Success {
                    bench_name,
                    report,
                    markdown,
                    extras_prefix,
                } => {
                    pass_count += report.summary.pass_count;
                    warn_count += report.summary.warn_count;
                    fail_count += report.summary.fail_count;

                    match report.verdict.status {
                        perfgate_types::VerdictStatus::Fail => {
                            worst_status = SensorVerdictStatus::Fail;
                        }
                        perfgate_types::VerdictStatus::Warn => {
                            if worst_status != SensorVerdictStatus::Fail {
                                worst_status = SensorVerdictStatus::Warn;
                            }
                        }
                        perfgate_types::VerdictStatus::Pass => {
                            if worst_status == SensorVerdictStatus::Skip {
                                worst_status = SensorVerdictStatus::Pass;
                            }
                        }
                        perfgate_types::VerdictStatus::Skip => {}
                    }

                    for reason in &report.verdict.reasons {
                        if !all_reasons.contains(reason) {
                            all_reasons.push(reason.clone());
                        }
                    }

                    for f in &report.findings {
                        let mut finding_data =
                            f.data.as_ref().map(|d| serde_json::to_value(d).unwrap());
                        if let Some(obj) = finding_data.as_mut().and_then(|v| v.as_object_mut()) {
                            obj.insert("bench_name".to_string(), serde_json::json!(bench_name));
                        } else {
                            finding_data = Some(serde_json::json!({ "bench_name": bench_name }));
                        }

                        findings.push(SensorFinding {
                            check_id: f.check_id.clone(),
                            code: f.code.clone(),
                            severity: map_severity(f.severity),
                            message: format!("[{}] {}", bench_name, f.message),
                            fingerprint: Some(sha256_hex(
                                format!("{}:{}:{}", bench_name, f.check_id, f.code).as_bytes(),
                            )),
                            data: finding_data,
                        });
                    }

                    if let Some(prefix) = extras_prefix {
                        artifacts.push(SensorArtifact {
                            path: format!("{}/perfgate.run.v1.json", prefix),
                            artifact_type: "run_receipt".to_string(),
                        });
                        if report.compare.is_some() {
                            artifacts.push(SensorArtifact {
                                path: format!("{}/perfgate.compare.v1.json", prefix),
                                artifact_type: "compare_receipt".to_string(),
                            });
                        }
                        artifacts.push(SensorArtifact {
                            path: format!("{}/perfgate.report.v1.json", prefix),
                            artifact_type: "perfgate_report".to_string(),
                        });
                    }

                    if !all_markdown.is_empty() {
                        all_markdown.push_str("\n---\n\n");
                    }
                    all_markdown.push_str(markdown);
                }
                BenchOutcome::Error {
                    bench_name,
                    error,
                    stage,
                    kind,
                } => {
                    worst_status = SensorVerdictStatus::Fail;
                    fail_count += 1;
                    if !all_reasons.contains(&"tool_error".to_string()) {
                        all_reasons.push("tool_error".to_string());
                    }
                    findings.push(SensorFinding {
                        check_id: perfgate_types::CHECK_ID_TOOL_RUNTIME.to_string(),
                        code: perfgate_types::FINDING_CODE_RUNTIME_ERROR.to_string(),
                        severity: SensorSeverity::Error,
                        message: format!("[{}] tool error: {}", bench_name, error),
                        fingerprint: Some(sha256_hex(
                            format!("{}:{}:{}", bench_name, stage, kind).as_bytes(),
                        )),
                        data: Some(serde_json::json!({
                            "bench_name": bench_name,
                            "stage": stage,
                            "error_kind": kind,
                        })),
                    });
                }
            }
        }

        findings.sort_by(|a, b| a.message.cmp(&b.message));

        if findings.len() > self.max_findings {
            truncate_findings(&mut findings, self.max_findings);
        }

        let run = SensorRunMeta {
            started_at: self.started_at,
            ended_at: self.ended_at,
            duration_ms: self.duration_ms,
            capabilities: SensorCapabilities {
                baseline: if self.baseline_available {
                    Capability {
                        status: CapabilityStatus::Available,
                        reason: None,
                    }
                } else {
                    Capability {
                        status: CapabilityStatus::Unavailable,
                        reason: self.baseline_reason.clone(),
                    }
                },
                engine: None,
            },
        };

        artifacts.sort_by(|a, b| (&a.artifact_type, &a.path).cmp(&(&b.artifact_type, &b.path)));

        let report = SensorReport {
            schema: SENSOR_REPORT_SCHEMA_V1.to_string(),
            tool: self.tool,
            run,
            verdict: SensorVerdict {
                status: worst_status,
                counts: SensorVerdictCounts {
                    info: pass_count,
                    warn: warn_count,
                    error: fail_count,
                },
                reasons: all_reasons,
            },
            findings,
            artifacts,
            data: serde_json::json!({
                "summary": {
                    "bench_count": outcomes.len(),
                    "pass_count": pass_count,
                    "warn_count": warn_count,
                    "fail_count": fail_count,
                    "total_count": pass_count + warn_count + fail_count,
                }
            }),
        };

        (report, all_markdown)
    }
}

#[derive(Debug, Clone)]
pub enum BenchOutcome {
    Success {
        bench_name: String,
        report: Box<PerfgateReport>,
        markdown: String,
        extras_prefix: Option<String>,
    },
    Error {
        bench_name: String,
        error: String,
        stage: String,
        kind: String,
    },
}

fn map_severity(s: Severity) -> SensorSeverity {
    match s {
        Severity::Warn => SensorSeverity::Warn,
        Severity::Fail => SensorSeverity::Error,
    }
}

fn truncate_findings(findings: &mut Vec<SensorFinding>, limit: usize) {
    let shown = limit.saturating_sub(1);
    findings.truncate(shown);
    findings.push(SensorFinding {
        check_id: CHECK_ID_TOOL_TRUNCATION.to_string(),
        code: FINDING_CODE_TRUNCATED.to_string(),
        severity: SensorSeverity::Info,
        message: "Some findings were truncated to stay within limits.".to_string(),
        fingerprint: Some(sha256_hex(
            format!("{}:{}", CHECK_ID_TOOL_TRUNCATION, FINDING_CODE_TRUNCATED).as_bytes(),
        )),
        data: None,
    });
}

/// Generates a fingerprint for a set of findings.
pub fn sensor_fingerprint(findings: &[SensorFinding]) -> String {
    if findings.is_empty() {
        return "".to_string();
    }

    let mut parts: Vec<String> = findings
        .iter()
        .map(|f| format!("{}:{}", f.check_id, f.code))
        .collect();
    parts.sort();
    parts.dedup();

    let combined = parts.join(",");
    sha256_hex(combined.as_bytes())
}

pub fn default_engine_capability() -> SensorCapabilities {
    SensorCapabilities {
        baseline: Capability {
            status: CapabilityStatus::Available,
            reason: None,
        },
        engine: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{REPORT_SCHEMA_V1, ReportSummary, Verdict, VerdictCounts, VerdictStatus};

    pub(crate) fn make_tool_info() -> ToolInfo {
        ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    pub(crate) fn make_pass_report() -> PerfgateReport {
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Pass,
                counts: VerdictCounts {
                    pass: 3,
                    warn: 0,
                    fail: 0,
                    skip: 0,
                },
                reasons: vec![],
            },
            compare: None,
            findings: vec![],
            summary: ReportSummary {
                pass_count: 3,
                warn_count: 0,
                fail_count: 0,
                skip_count: 0,
                total_count: 3,
            },
            complexity: None,
            profile_path: None,
        }
    }

    pub(crate) fn make_fail_report() -> PerfgateReport {
        use perfgate_types::{FINDING_CODE_METRIC_FAIL, ReportFinding};
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 0,
                    fail: 2,
                    skip: 0,
                },
                reasons: vec!["wall_ms_fail".to_string(), "max_rss_kb_fail".to_string()],
            },
            compare: None,
            findings: vec![
                ReportFinding {
                    check_id: "perf.budget".to_string(),
                    code: FINDING_CODE_METRIC_FAIL.to_string(),
                    severity: Severity::Fail,
                    message: "wall_ms regression: +30.00% (threshold: 20.0%)".to_string(),
                    data: None,
                },
                ReportFinding {
                    check_id: "perf.budget".to_string(),
                    code: FINDING_CODE_METRIC_FAIL.to_string(),
                    severity: Severity::Fail,
                    message: "max_rss_kb regression: +25.00% (threshold: 15.0%)".to_string(),
                    data: None,
                },
            ],
            summary: ReportSummary {
                pass_count: 1,
                warn_count: 0,
                fail_count: 2,
                skip_count: 0,
                total_count: 3,
            },
            complexity: None,
            profile_path: None,
        }
    }

    pub(crate) fn make_warn_report() -> PerfgateReport {
        use perfgate_types::{FINDING_CODE_METRIC_WARN, ReportFinding};
        PerfgateReport {
            report_type: REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Warn,
                counts: VerdictCounts {
                    pass: 1,
                    warn: 1,
                    fail: 0,
                    skip: 0,
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
                skip_count: 0,
                total_count: 2,
            },
            complexity: None,
            profile_path: None,
        }
    }

    #[test]
    fn test_build_pass_sensor_report() {
        let report = make_pass_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());
        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Pass);
        assert_eq!(sensor_report.verdict.counts.info, 3);
    }

    #[test]
    fn test_build_fail_sensor_report() {
        let report = make_fail_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());
        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(sensor_report.verdict.counts.error, 2);
        assert_eq!(sensor_report.findings.len(), 2);
    }

    #[test]
    fn test_build_warn_sensor_report() {
        let report = make_warn_report();
        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());
        let sensor_report = builder.build(&report);

        assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Warn);
        assert_eq!(sensor_report.verdict.counts.warn, 1);
    }

    #[test]
    fn test_build_aggregated_single_bench_matches_build() {
        let report = make_warn_report();
        let outcome = BenchOutcome::Success {
            bench_name: "bench-a".to_string(),
            report: Box::new(report.clone()),
            markdown: "md".to_string(),
            extras_prefix: None,
        };

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());
        let (agg_report, _) = builder.build_aggregated(&[outcome]);

        assert_eq!(agg_report.verdict.status, SensorVerdictStatus::Warn);
        assert_eq!(agg_report.verdict.counts.warn, 1);
        assert_eq!(agg_report.findings.len(), 1);
    }

    #[test]
    fn test_build_aggregated_multi_bench_counts_summed() {
        let report_a = make_warn_report();
        let report_b = make_fail_report();

        let outcome_a = BenchOutcome::Success {
            bench_name: "bench-a".to_string(),
            report: Box::new(report_a),
            markdown: "md-a".to_string(),
            extras_prefix: None,
        };
        let outcome_b = BenchOutcome::Success {
            bench_name: "bench-b".to_string(),
            report: Box::new(report_b),
            markdown: "md-b".to_string(),
            extras_prefix: None,
        };

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());
        let (agg_report, _) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(agg_report.verdict.status, SensorVerdictStatus::Fail);
        assert_eq!(agg_report.verdict.counts.info, 2);
        assert_eq!(agg_report.verdict.counts.warn, 1);
        assert_eq!(agg_report.verdict.counts.error, 2);
    }

    #[test]
    fn test_build_aggregated_mixed_success_and_error() {
        let report_a = make_pass_report();
        let outcome_a = BenchOutcome::Success {
            bench_name: "bench-a".to_string(),
            report: Box::new(report_a),
            markdown: "md-a".to_string(),
            extras_prefix: None,
        };
        let outcome_b = BenchOutcome::Error {
            bench_name: "bench-b".to_string(),
            error: "boom".to_string(),
            stage: "run_command".to_string(),
            kind: "error".to_string(),
        };

        let builder =
            SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string());
        let (agg_report, _) = builder.build_aggregated(&[outcome_a, outcome_b]);

        assert_eq!(agg_report.verdict.status, SensorVerdictStatus::Fail); // Error maps to fail
        assert_eq!(agg_report.verdict.counts.info, 3);
        assert_eq!(agg_report.findings.len(), 1); // Only the error finding
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use insta::assert_json_snapshot;
    use perfgate_types::{
        FindingData, ReportFinding, ReportSummary, Verdict, VerdictCounts, VerdictStatus,
    };

    #[test]
    fn snapshot_pass_report() {
        let report = tests::make_pass_report();
        let sensor_report =
            SensorReportBuilder::new(tests::make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:31:00Z".to_string(), 60000)
                .build(&report);

        assert_json_snapshot!(sensor_report);
    }

    #[test]
    fn snapshot_fail_report() {
        let report = tests::make_fail_report();
        let sensor_report =
            SensorReportBuilder::new(tests::make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:31:00Z".to_string(), 60000)
                .baseline(true, None)
                .build(&report);

        assert_json_snapshot!(sensor_report);
    }

    #[test]
    fn snapshot_aggregated_multi_bench() {
        let report_a = tests::make_warn_report();
        let outcome_a = BenchOutcome::Success {
            bench_name: "bench-a".to_string(),
            report: Box::new(report_a),
            markdown: "markdown a".to_string(),
            extras_prefix: Some("artifacts/bench-a".to_string()),
        };

        let report_b = tests::make_pass_report();
        let outcome_b = BenchOutcome::Success {
            bench_name: "bench-b".to_string(),
            report: Box::new(report_b),
            markdown: "markdown b".to_string(),
            extras_prefix: Some("artifacts/bench-b".to_string()),
        };

        let (sensor_report, _md) =
            SensorReportBuilder::new(tests::make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:32:00Z".to_string(), 120000)
                .build_aggregated(&[outcome_a, outcome_b]);

        assert_json_snapshot!(sensor_report);
    }

    #[test]
    fn snapshot_truncated_report() {
        use perfgate_types::FINDING_CODE_METRIC_FAIL;
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
            report_type: perfgate_types::REPORT_SCHEMA_V1.to_string(),
            verdict: Verdict {
                status: VerdictStatus::Fail,
                counts: VerdictCounts {
                    pass: 0,
                    warn: 0,
                    fail: 5,
                    skip: 0,
                },
                reasons: vec!["truncated".to_string()],
            },
            compare: None,
            findings,
            summary: ReportSummary {
                pass_count: 0,
                warn_count: 0,
                fail_count: 5,
                skip_count: 0,
                total_count: 5,
            },
            complexity: None,
            profile_path: None,
        };

        let sensor_report =
            SensorReportBuilder::new(tests::make_tool_info(), "2024-01-15T10:30:00Z".to_string())
                .ended_at("2024-01-15T10:31:00Z".to_string(), 60000)
                .baseline(true, None)
                .max_findings(3)
                .build(&report);

        assert_json_snapshot!(sensor_report);
    }
}
