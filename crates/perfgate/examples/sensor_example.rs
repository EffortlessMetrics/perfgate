//! Demonstrates SensorReportBuilder for creating sensor.report.v1 envelopes.
//!
//! Run with: cargo run -p perfgate --example sensor_example

use perfgate::presentation::sensor::{SensorReportBuilder, sensor_fingerprint};
use perfgate_types::{
    PerfgateReport, REPORT_SCHEMA_V1, ReportSummary, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tool = ToolInfo {
        name: "perfgate-demo".to_string(),
        version: "0.1.0".to_string(),
    };

    let perfgate_report = PerfgateReport {
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
    };

    let sensor_report = SensorReportBuilder::new(tool, "2024-01-15T10:30:00Z".to_string())
        .ended_at("2024-01-15T10:31:00Z".to_string(), 60000)
        .baseline(true, None)
        .build(&perfgate_report);

    println!("Sensor report schema: {}", sensor_report.schema);
    println!("Verdict: {:?}", sensor_report.verdict.status);
    println!("Findings: {}", sensor_report.findings.len());

    let fp = sensor_fingerprint(&sensor_report.findings);
    println!("Fingerprint (SHA-256): {}", fp);

    let json = serde_json::to_string_pretty(&sensor_report)?;
    println!("\nJSON snippet:\n{}", &json[..json.len().min(500)]);
    println!("...");
    Ok(())
}
