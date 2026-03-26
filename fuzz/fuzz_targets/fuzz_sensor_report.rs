//! Fuzz target for sensor report building.
//!
//! This target verifies that `SensorReportBuilder` never panics when
//! constructing sensor reports from arbitrary PerfgateReport inputs,
//! including edge cases in verdict status, finding counts, and truncation.

#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use perfgate_types::{
    Direction, FindingData, PerfgateReport, ReportFinding, ReportSummary, Severity, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus, REPORT_SCHEMA_V1,
};

#[derive(Debug, Arbitrary)]
struct SensorInput {
    tool_name: String,
    tool_version: String,
    started_at: String,
    ended_at: String,
    duration_ms: u64,
    baseline_available: bool,
    max_findings: Option<u8>,
    verdict_status: FuzzVerdictStatus,
    pass_count: u32,
    warn_count: u32,
    fail_count: u32,
    findings: Vec<FuzzFinding>,
}

#[derive(Debug, Arbitrary)]
enum FuzzVerdictStatus {
    Pass,
    Warn,
    Fail,
}

impl FuzzVerdictStatus {
    fn to_verdict_status(&self) -> VerdictStatus {
        match self {
            FuzzVerdictStatus::Pass => VerdictStatus::Pass,
            FuzzVerdictStatus::Warn => VerdictStatus::Warn,
            FuzzVerdictStatus::Fail => VerdictStatus::Fail,
        }
    }
}

#[derive(Debug, Arbitrary)]
enum FuzzSeverity {
    Warn,
    Fail,
}

#[derive(Debug, Arbitrary)]
struct FuzzFinding {
    check_id: String,
    code: String,
    severity: FuzzSeverity,
    message: String,
    metric_name: String,
}

fuzz_target!(|input: SensorInput| {
    let tool = ToolInfo {
        name: input.tool_name,
        version: input.tool_version,
    };

    let findings: Vec<ReportFinding> = input
        .findings
        .iter()
        .take(200)
        .map(|f| ReportFinding {
            check_id: f.check_id.clone(),
            code: f.code.clone(),
            severity: match f.severity {
                FuzzSeverity::Warn => Severity::Warn,
                FuzzSeverity::Fail => Severity::Fail,
            },
            message: f.message.clone(),
            data: Some(FindingData {
                metric_name: f.metric_name.clone(),
                baseline: 0.0,
                current: 0.0,
                threshold: 0.2,
                regression_pct: 0.0,
                direction: Direction::Lower,
            }),
        })
        .collect();

    let total_count = input.pass_count + input.warn_count + input.fail_count;
    let report = PerfgateReport {
        report_type: REPORT_SCHEMA_V1.to_string(),
        verdict: Verdict {
            status: input.verdict_status.to_verdict_status(),
            counts: VerdictCounts {
                pass: input.pass_count,
                warn: input.warn_count,
                fail: input.fail_count,
            },
            reasons: vec![],
        },
        compare: None,
        findings,
        summary: ReportSummary {
            pass_count: input.pass_count,
            warn_count: input.warn_count,
            fail_count: input.fail_count,
            total_count,
        },
    };

    let mut builder = perfgate_sensor::SensorReportBuilder::new(tool, input.started_at)
        .ended_at(input.ended_at, input.duration_ms)
        .baseline(input.baseline_available, None);

    if let Some(limit) = input.max_findings {
        builder = builder.max_findings(limit as usize);
    }

    // Should never panic regardless of input
    let sensor_report = builder.build(&report);

    // Basic invariant: verdict counts should be non-negative (they're u32, so always true)
    let _ = serde_json::to_string(&sensor_report);
});








