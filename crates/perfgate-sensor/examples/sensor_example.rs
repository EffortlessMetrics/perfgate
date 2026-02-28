//! Demonstrates SensorReportBuilder for creating sensor.report.v1 envelopes.

use perfgate_sensor::{SensorReportBuilder, default_engine_capability, sensor_fingerprint};
use perfgate_types::{
    PerfgateReport, REPORT_SCHEMA_V1, ReportSummary, ToolInfo, Verdict, VerdictCounts,
    VerdictStatus,
};

fn main() {
    let tool = ToolInfo {
        name: "perfgate".to_string(),
        version: "0.1.0".to_string(),
    };

    // Build a minimal PerfgateReport (pass, no compare data)
    let report = PerfgateReport {
        report_type: REPORT_SCHEMA_V1.to_string(),
        verdict: Verdict {
            status: VerdictStatus::Pass,
            counts: VerdictCounts {
                pass: 1,
                warn: 0,
                fail: 0,
            },
            reasons: vec![],
        },
        compare: None,
        findings: vec![],
        summary: ReportSummary {
            pass_count: 1,
            warn_count: 0,
            fail_count: 0,
            total_count: 1,
        },
    };

    // Build a sensor report envelope
    let sensor_report = SensorReportBuilder::new(tool, "2025-01-15T10:00:00Z".to_string())
        .ended_at("2025-01-15T10:00:05Z".to_string(), 5000)
        .baseline(true, None)
        .engine(default_engine_capability())
        .build(&report);

    println!("Sensor report schema: {}", sensor_report.schema);
    println!("Verdict: {:?}", sensor_report.verdict.status);
    println!("Findings: {}", sensor_report.findings.len());

    // Demonstrate fingerprinting
    let fp = sensor_fingerprint(&["perfgate", "perf.budget", "metric_fail", "wall_ms"]);
    println!("\nFingerprint (SHA-256): {}", fp);
    assert_eq!(fp.len(), 64);

    // Serialize to JSON
    let json = serde_json::to_string_pretty(&sensor_report).expect("serialize");
    println!("\n{}", &json[..json.len().min(500)]);
    println!("...");
}
