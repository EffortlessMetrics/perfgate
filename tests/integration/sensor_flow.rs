//! Integration tests: sensor report building flow.
//!
//! These tests verify the full sensor report building flow,
//! including error classification through to sensor report.

use perfgate_sensor::{
    BenchOutcome, SensorReportBuilder, default_engine_capability, sensor_fingerprint,
};
use perfgate_types::{
    CHECK_ID_TOOL_RUNTIME, CapabilityStatus, Direction, ERROR_KIND_PARSE,
    FINDING_CODE_RUNTIME_ERROR, FindingData, PerfgateReport, REPORT_SCHEMA_V1, ReportFinding,
    ReportSummary, STAGE_CONFIG_PARSE, SensorSeverity, SensorVerdictStatus, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus,
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
                pass: 0,
                warn: 0,
                fail: 1,
            },
            reasons: vec!["wall_ms_fail".to_string()],
        },
        compare: None,
        findings: vec![ReportFinding {
            check_id: "perf.budget".to_string(),
            code: "metric_fail".to_string(),
            severity: Severity::Fail,
            message: "wall_ms regression: +25.00%".to_string(),
            data: Some(FindingData {
                metric_name: "wall_ms".to_string(),
                baseline: 100.0,
                current: 125.0,
                regression_pct: 25.0,
                threshold: 0.20,
                direction: Direction::Lower,
            }),
        }],
        summary: ReportSummary {
            pass_count: 0,
            warn_count: 0,
            fail_count: 1,
            total_count: 1,
        },
    }
}

#[test]
fn sensor_report_pass_verdict() {
    let report = make_pass_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let sensor_report = builder.build(&report);

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Pass);
    assert_eq!(sensor_report.verdict.counts.info, 2);
    assert_eq!(sensor_report.verdict.counts.warn, 0);
    assert_eq!(sensor_report.verdict.counts.error, 0);
}

#[test]
fn sensor_report_fail_verdict() {
    let report = make_fail_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let sensor_report = builder.build(&report);

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
    assert_eq!(sensor_report.verdict.counts.error, 1);
    assert_eq!(sensor_report.findings.len(), 1);
    assert_eq!(sensor_report.findings[0].severity, SensorSeverity::Error);
}

#[test]
fn sensor_report_error_classification() {
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(false, None);

    let sensor_report = builder.build_error(
        "config file not found",
        STAGE_CONFIG_PARSE,
        ERROR_KIND_PARSE,
    );

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
    assert_eq!(sensor_report.findings.len(), 1);

    let finding = &sensor_report.findings[0];
    assert_eq!(finding.check_id, CHECK_ID_TOOL_RUNTIME);
    assert_eq!(finding.code, FINDING_CODE_RUNTIME_ERROR);
    assert_eq!(finding.severity, SensorSeverity::Error);

    let data = finding.data.as_ref().unwrap();
    assert_eq!(data["stage"], STAGE_CONFIG_PARSE);
    assert_eq!(data["error_kind"], ERROR_KIND_PARSE);
}

#[test]
fn sensor_report_baseline_capability() {
    let report = make_pass_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let sensor_report = builder.build(&report);

    assert_eq!(
        sensor_report.run.capabilities.baseline.status,
        CapabilityStatus::Available
    );
    assert!(sensor_report.run.capabilities.baseline.reason.is_none());
}

#[test]
fn sensor_report_no_baseline_capability() {
    let report = make_pass_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
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
fn sensor_report_engine_capability() {
    let report = make_pass_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let sensor_report = builder.build(&report);

    let engine = sensor_report.run.capabilities.engine.unwrap();
    if cfg!(unix) {
        assert_eq!(engine.status, CapabilityStatus::Available);
    } else {
        assert_eq!(engine.status, CapabilityStatus::Unavailable);
    }
}

#[test]
fn sensor_fingerprint_deterministic() {
    let fp1 = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail", "wall_ms"]);
    let fp2 = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail", "wall_ms"]);

    assert_eq!(fp1, fp2);
    assert_eq!(fp1.len(), 64);
}

#[test]
fn sensor_fingerprint_different_for_different_inputs() {
    let fp1 = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail", "wall_ms"]);
    let fp2 = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail", "max_rss_kb"]);

    assert_ne!(fp1, fp2);
}

#[test]
fn sensor_fingerprint_trims_trailing_empty() {
    let fp1 = sensor_fingerprint(&["a", "b", ""]);
    let fp2 = sensor_fingerprint(&["a", "b"]);

    assert_eq!(fp1, fp2);
}

#[test]
fn sensor_report_with_artifacts() {
    let report = make_pass_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None)
        .artifact("report.json".to_string(), "sensor_report".to_string())
        .artifact("comment.md".to_string(), "markdown".to_string());

    let sensor_report = builder.build(&report);

    assert_eq!(sensor_report.artifacts.len(), 2);
}

#[test]
fn sensor_report_aggregated_single_bench() {
    let report = make_pass_report();
    let outcome = BenchOutcome::Success {
        bench_name: "test-bench".to_string(),
        report,
        has_compare: true,
        baseline_available: true,
        markdown: "## Results\n\nPass".to_string(),
        extras_prefix: "extras".to_string(),
    };

    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let (sensor_report, markdown) = builder.build_aggregated(&[outcome]);

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Pass);
    assert!(markdown.contains("Results"));
}

#[test]
fn sensor_report_aggregated_multiple_benches() {
    let report1 = make_pass_report();
    let report2 = make_fail_report();

    let outcomes = vec![
        BenchOutcome::Success {
            bench_name: "bench1".to_string(),
            report: report1,
            has_compare: true,
            baseline_available: true,
            markdown: "## Bench1\n\nPass".to_string(),
            extras_prefix: "extras/bench1".to_string(),
        },
        BenchOutcome::Success {
            bench_name: "bench2".to_string(),
            report: report2,
            has_compare: true,
            baseline_available: true,
            markdown: "## Bench2\n\nFail".to_string(),
            extras_prefix: "extras/bench2".to_string(),
        },
    ];

    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let (sensor_report, markdown) = builder.build_aggregated(&outcomes);

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
    assert!(markdown.contains("---"));
    assert!(markdown.contains("Bench1"));
    assert!(markdown.contains("Bench2"));
}

#[test]
fn sensor_report_aggregated_with_error() {
    let report = make_pass_report();

    let outcomes = vec![
        BenchOutcome::Success {
            bench_name: "bench1".to_string(),
            report,
            has_compare: true,
            baseline_available: true,
            markdown: "## Bench1\n\nPass".to_string(),
            extras_prefix: "extras/bench1".to_string(),
        },
        BenchOutcome::Error {
            bench_name: "bench2".to_string(),
            error_message: "Command failed".to_string(),
            stage: "run_command",
            error_kind: "exec_error",
        },
    ];

    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let (sensor_report, _markdown) = builder.build_aggregated(&outcomes);

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
    assert!(
        sensor_report
            .verdict
            .reasons
            .contains(&"tool_error".to_string())
    );
}

#[test]
fn default_engine_capability_platform_specific() {
    let cap = default_engine_capability();

    if cfg!(unix) {
        assert_eq!(cap.status, CapabilityStatus::Available);
        assert!(cap.reason.is_none());
    } else {
        assert_eq!(cap.status, CapabilityStatus::Unavailable);
        assert_eq!(cap.reason, Some("platform_limited".to_string()));
    }
}

#[test]
fn sensor_report_serialization() {
    let report = make_pass_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .ended_at("2024-01-01T00:01:00Z".to_string(), 60000)
        .baseline(true, None);

    let sensor_report = builder.build(&report);

    let json = serde_json::to_string(&sensor_report).unwrap();
    let deserialized: perfgate_types::SensorReport = serde_json::from_str(&json).unwrap();

    assert_eq!(sensor_report.schema, deserialized.schema);
    assert_eq!(sensor_report.verdict.status, deserialized.verdict.status);
}
