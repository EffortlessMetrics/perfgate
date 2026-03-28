//! Integration tests: sensor report building flow.
//!
//! These tests verify the full sensor report building flow,
//! including error classification through to sensor report.

use perfgate_sensor::{
    BenchOutcome, SensorFinding, SensorReportBuilder, SensorSeverity, SensorVerdictStatus,
    sensor_fingerprint,
};
use perfgate_types::{
    CapabilityStatus, Direction, FindingData, PerfgateReport, REPORT_SCHEMA_V1, ReportFinding,
    ReportSummary, Severity, ToolInfo, Verdict, VerdictCounts, VerdictStatus,
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
                pass: 1,
                warn: 0,
                fail: 0,
                skip: 0,
            },
            reasons: vec![],
        },
        compare: None,
        findings: vec![],
        summary: ReportSummary {
            pass_count: 1,
            warn_count: 0,
            fail_count: 0,
            skip_count: 0,
            total_count: 1,
        },
        complexity: None,
        profile_path: None,
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
                skip: 0,
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
            skip_count: 0,
            total_count: 1,
        },
        complexity: None,
        profile_path: None,
    }
}

#[test]
fn sensor_report_pass_verdict() {
    let report = make_pass_report();
    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let sensor_report = builder.build(&report);

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Pass);
    assert_eq!(sensor_report.verdict.counts.info, 1);
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

fn mock_finding(check_id: &str, code: &str) -> SensorFinding {
    SensorFinding {
        check_id: check_id.to_string(),
        code: code.to_string(),
        severity: SensorSeverity::Error,
        message: "msg".to_string(),
        fingerprint: None,
        data: None,
    }
}

#[test]
fn sensor_fingerprint_deterministic() {
    let f1 = mock_finding("perfgate", "perf.budget");
    let f2 = mock_finding("perfgate", "perf.budget");

    let fp1 = sensor_fingerprint(std::slice::from_ref(&f1));
    let fp2 = sensor_fingerprint(std::slice::from_ref(&f2));

    assert_eq!(fp1, fp2);
    assert_eq!(fp1.len(), 64);
}

#[test]
fn sensor_fingerprint_different_for_different_inputs() {
    let f1 = mock_finding("perfgate", "perf.budget");
    let f2 = mock_finding("perfgate", "other");

    let fp1 = sensor_fingerprint(std::slice::from_ref(&f1));
    let fp2 = sensor_fingerprint(std::slice::from_ref(&f2));

    assert_ne!(fp1, fp2);
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
        report: Box::new(report),
        markdown: "## Results\n\nPass".to_string(),
        extras_prefix: Some("extras".to_string()),
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
            report: Box::new(report1),
            markdown: "## Bench1\n\nPass".to_string(),
            extras_prefix: Some("extras/bench1".to_string()),
        },
        BenchOutcome::Success {
            bench_name: "bench2".to_string(),
            report: Box::new(report2),
            markdown: "## Bench2\n\nFail".to_string(),
            extras_prefix: Some("extras/bench2".to_string()),
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
            report: Box::new(report),
            markdown: "## Bench1\n\nPass".to_string(),
            extras_prefix: Some("extras/bench1".to_string()),
        },
        BenchOutcome::Error {
            bench_name: "bench2".to_string(),
            error: "Command failed".to_string(),
            stage: "run_command".to_string(),
            kind: "exec_error".to_string(),
        },
    ];

    let builder = SensorReportBuilder::new(make_tool_info(), "2024-01-01T00:00:00Z".to_string())
        .baseline(true, None);

    let (sensor_report, _markdown) = builder.build_aggregated(&outcomes);

    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Fail);
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
