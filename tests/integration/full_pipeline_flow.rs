//! Integration tests: full pipeline flow across crates.
//!
//! These tests verify end-to-end flows through multiple crates:
//! - Samples → stats → compare → budget → verdict
//! - RunReceipt → sensor report → structure verification
//! - RunReceipt → CSV export → structure verification

use perfgate_budget::evaluate_budget;
use perfgate_domain::{compare_stats, compute_stats};
use perfgate_export::{ExportFormat, ExportUseCase};
use perfgate_sensor::SensorReportBuilder;
use perfgate_types::{
    BenchMeta, Budget, Direction, HostInfo, Metric, MetricStatus, PerfgateReport, REPORT_SCHEMA_V1,
    RUN_SCHEMA_V1, ReportSummary, RunMeta, RunReceipt, Sample, SensorVerdictStatus, ToolInfo,
    Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

fn make_sample(wall_ms: u64) -> Sample {
    Sample {
        wall_ms,
        exit_code: 0,
        warmup: false,
        timed_out: false,
        cpu_ms: Some(wall_ms / 2),
        page_faults: None,
        ctx_switches: None,
        max_rss_kb: Some(1024),
        binary_bytes: None,
        stdout: None,
        stderr: None,
    }
}

fn make_run_receipt_from_samples(samples: Vec<Sample>) -> RunReceipt {
    let stats = compute_stats(&samples, None).unwrap();
    RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        run: RunMeta {
            id: "pipeline-test".to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            ended_at: "2024-01-01T00:01:00Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: "pipeline-bench".to_string(),
            cwd: None,
            command: vec!["echo".to_string(), "hello".to_string()],
            repeat: samples.len() as u32,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples,
        stats,
    }
}

// ---------- Full pipeline tests ----------

#[test]
fn full_pipeline_pass_verdict() {
    // 1. Create samples
    let baseline_samples: Vec<Sample> = vec![100, 102, 98, 101, 99]
        .into_iter()
        .map(make_sample)
        .collect();
    let current_samples: Vec<Sample> = vec![103, 105, 101, 104, 102]
        .into_iter()
        .map(make_sample)
        .collect();

    // 2. Compute stats via compute_stats (which uses summarize_u64 internally)
    let baseline_stats = compute_stats(&baseline_samples, None).unwrap();
    let current_stats = compute_stats(&current_samples, None).unwrap();

    assert_eq!(baseline_stats.wall_ms.median, 100);
    assert_eq!(current_stats.wall_ms.median, 103);

    // 3. Compare stats with budgets
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));

    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets).unwrap();

    // 4. Verify verdict is Pass (3% regression < 10% warn threshold)
    assert_eq!(comparison.verdict.status, VerdictStatus::Pass);
    assert_eq!(comparison.verdict.counts.pass, 1);
    assert_eq!(comparison.verdict.counts.warn, 0);
    assert_eq!(comparison.verdict.counts.fail, 0);

    // 5. Verify delta details
    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert_eq!(delta.status, MetricStatus::Pass);
    assert!((delta.baseline - 100.0).abs() < f64::EPSILON);
    assert!((delta.current - 103.0).abs() < f64::EPSILON);
}

#[test]
fn full_pipeline_warn_verdict() {
    let baseline_samples: Vec<Sample> = vec![100, 100, 100].into_iter().map(make_sample).collect();
    let current_samples: Vec<Sample> = vec![115, 115, 115].into_iter().map(make_sample).collect();

    let baseline_stats = compute_stats(&baseline_samples, None).unwrap();
    let current_stats = compute_stats(&current_samples, None).unwrap();

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));

    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets).unwrap();

    assert_eq!(comparison.verdict.status, VerdictStatus::Warn);
    assert_eq!(comparison.verdict.counts.warn, 1);

    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert_eq!(delta.status, MetricStatus::Warn);
    assert!((delta.regression - 0.15).abs() < 1e-10);
}

#[test]
fn full_pipeline_fail_verdict() {
    let baseline_samples: Vec<Sample> = vec![100, 100, 100].into_iter().map(make_sample).collect();
    let current_samples: Vec<Sample> = vec![130, 130, 130].into_iter().map(make_sample).collect();

    let baseline_stats = compute_stats(&baseline_samples, None).unwrap();
    let current_stats = compute_stats(&current_samples, None).unwrap();

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));

    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets).unwrap();

    assert_eq!(comparison.verdict.status, VerdictStatus::Fail);
    assert_eq!(comparison.verdict.counts.fail, 1);
    assert!(
        comparison
            .verdict
            .reasons
            .contains(&"wall_ms_fail".to_string())
    );

    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert_eq!(delta.status, MetricStatus::Fail);
    assert!((delta.regression - 0.30).abs() < 1e-10);
}

#[test]
fn full_pipeline_multiple_metrics() {
    let baseline_samples: Vec<Sample> = vec![
        Sample {
            wall_ms: 100,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: Some(1000),
            binary_bytes: None,
            stdout: None,
            stderr: None,
        },
        Sample {
            wall_ms: 100,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: Some(1000),
            binary_bytes: None,
            stdout: None,
            stderr: None,
        },
    ];
    let current_samples: Vec<Sample> = vec![
        Sample {
            wall_ms: 105,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: Some(1500),
            binary_bytes: None,
            stdout: None,
            stderr: None,
        },
        Sample {
            wall_ms: 105,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: Some(1500),
            binary_bytes: None,
            stdout: None,
            stderr: None,
        },
    ];

    let baseline_stats = compute_stats(&baseline_samples, None).unwrap();
    let current_stats = compute_stats(&current_samples, None).unwrap();

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));
    budgets.insert(Metric::MaxRssKb, Budget::new(0.30, 0.15, Direction::Lower));

    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets).unwrap();

    // wall_ms: 5% regression -> pass
    assert_eq!(
        comparison.deltas.get(&Metric::WallMs).unwrap().status,
        MetricStatus::Pass
    );
    // max_rss_kb: 50% regression -> fail
    assert_eq!(
        comparison.deltas.get(&Metric::MaxRssKb).unwrap().status,
        MetricStatus::Fail
    );
    // Overall: fail dominates
    assert_eq!(comparison.verdict.status, VerdictStatus::Fail);
}

#[test]
fn full_pipeline_evaluate_budget_directly() {
    // Test the individual budget evaluation step
    let budget = Budget::new(0.20, 0.10, Direction::Lower);

    let pass = evaluate_budget(100.0, 105.0, &budget, None).unwrap();
    assert_eq!(pass.status, MetricStatus::Pass);

    let warn = evaluate_budget(100.0, 115.0, &budget, None).unwrap();
    assert_eq!(warn.status, MetricStatus::Warn);

    let fail = evaluate_budget(100.0, 125.0, &budget, None).unwrap();
    assert_eq!(fail.status, MetricStatus::Fail);
}

#[test]
fn full_pipeline_improvement_is_pass() {
    let baseline_samples: Vec<Sample> = vec![200, 200, 200].into_iter().map(make_sample).collect();
    let current_samples: Vec<Sample> = vec![100, 100, 100].into_iter().map(make_sample).collect();

    let baseline_stats = compute_stats(&baseline_samples, None).unwrap();
    let current_stats = compute_stats(&current_samples, None).unwrap();

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.05, 0.03, Direction::Lower));

    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets).unwrap();

    // Improvement (current < baseline for lower-is-better) -> pass
    assert_eq!(comparison.verdict.status, VerdictStatus::Pass);
    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert!((delta.regression - 0.0).abs() < 1e-10);
}

// ---------- Sensor report flow tests ----------

#[test]
fn sensor_report_from_run_receipt_pass() {
    let samples: Vec<Sample> = vec![100, 102, 98].into_iter().map(make_sample).collect();
    let receipt = make_run_receipt_from_samples(samples);

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
    };

    let builder = SensorReportBuilder::new(receipt.tool.clone(), receipt.run.started_at.clone())
        .ended_at(receipt.run.ended_at.clone(), 60000)
        .baseline(true, None);

    let sensor_report = builder.build(&perfgate_report);

    // Verify structure
    assert_eq!(sensor_report.schema, "sensor.report.v1");
    assert_eq!(sensor_report.tool.name, "perfgate");
    assert_eq!(sensor_report.verdict.status, SensorVerdictStatus::Pass);
    assert_eq!(sensor_report.run.started_at, "2024-01-01T00:00:00Z");
    assert_eq!(
        sensor_report.run.ended_at,
        Some("2024-01-01T00:01:00Z".to_string())
    );
    assert_eq!(sensor_report.run.duration_ms, Some(60000));
    assert!(sensor_report.findings.is_empty());
}

#[test]
fn sensor_report_serializes_to_valid_json() {
    let samples: Vec<Sample> = vec![100, 100].into_iter().map(make_sample).collect();
    let receipt = make_run_receipt_from_samples(samples);

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
    };

    let builder = SensorReportBuilder::new(receipt.tool.clone(), receipt.run.started_at.clone())
        .baseline(true, None);

    let sensor_report = builder.build(&perfgate_report);

    let json = serde_json::to_string_pretty(&sensor_report).unwrap();
    assert!(json.contains("sensor.report.v1"));
    assert!(json.contains("\"pass\""));

    // Round-trip
    let deserialized: perfgate_types::SensorReport = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.verdict.status, SensorVerdictStatus::Pass);
}

// ---------- Export flow tests ----------

#[test]
fn export_run_receipt_csv_structure() {
    let samples: Vec<Sample> = vec![100, 102, 98, 101, 99]
        .into_iter()
        .map(make_sample)
        .collect();
    let receipt = make_run_receipt_from_samples(samples);

    let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();

    // Verify CSV has header and data rows
    let lines: Vec<&str> = csv.trim().lines().collect();
    assert!(lines.len() >= 2, "CSV should have header + data rows");

    // Header should contain expected columns
    let header = lines[0];
    assert!(header.contains("bench_name"));
    assert!(header.contains("wall_ms"));

    // Data rows should reference the bench name
    for line in &lines[1..] {
        assert!(line.contains("pipeline-bench"));
    }
}

#[test]
fn export_run_receipt_jsonl_structure() {
    let samples: Vec<Sample> = vec![100, 102, 98].into_iter().map(make_sample).collect();
    let receipt = make_run_receipt_from_samples(samples);

    let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();

    // Each line should be valid JSON
    for line in jsonl.trim().lines() {
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(parsed["bench_name"], "pipeline-bench");
        assert!(parsed["wall_ms_median"].is_number());
    }
}

#[test]
fn export_run_receipt_csv_has_summary_row() {
    let samples: Vec<Sample> = (0..5).map(|i| make_sample(100 + i)).collect();
    let receipt = make_run_receipt_from_samples(samples);

    let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
    let lines: Vec<&str> = csv.trim().lines().collect();

    // 1 header + 1 summary row (export produces one row per receipt, not per sample)
    assert_eq!(lines.len(), 2);
}

#[test]
fn export_run_receipt_csv_optional_metrics_present() {
    let samples = vec![Sample {
        wall_ms: 100,
        exit_code: 0,
        warmup: false,
        timed_out: false,
        cpu_ms: Some(50),
        page_faults: None,
        ctx_switches: None,
        max_rss_kb: Some(2048),
        binary_bytes: None,
        stdout: None,
        stderr: None,
    }];
    let receipt = make_run_receipt_from_samples(samples);

    let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
    assert!(csv.contains("cpu_ms"));
    assert!(csv.contains("max_rss_kb"));
    assert!(csv.contains("50"));
    assert!(csv.contains("2048"));
}
