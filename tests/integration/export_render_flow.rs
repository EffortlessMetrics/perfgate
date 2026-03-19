//! Integration tests: export → render flow.
//!
//! These tests verify that export and render crates work together
//! correctly, testing the full receipt → markdown flow.

use perfgate_export::{ExportFormat, ExportUseCase};
use perfgate_render::{github_annotations, render_markdown};
use perfgate_types::{
    BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareReceipt, CompareRef, Delta, Direction, HostInfo,
    Metric, MetricStatistic, MetricStatus, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, Stats,
    ToolInfo, U64Summary, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

fn make_run_receipt() -> RunReceipt {
    RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        run: RunMeta {
            id: "test-run".to_string(),
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
            name: "test-bench".to_string(),
            cwd: None,
            command: vec!["echo".to_string(), "hello".to_string()],
            repeat: 3,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples: vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(50),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(1024),
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 102,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(52),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(1028),
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 98,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(48),
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: Some(1020),
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ],
        stats: Stats {
            wall_ms: U64Summary::new(100, 98, 102),
            cpu_ms: Some(U64Summary::new(50, 48, 52)),
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: Some(U64Summary::new(1024, 1020, 1028)),
            binary_bytes: None,
            throughput_per_s: None,
        },
    }
}

fn make_compare_receipt(status: MetricStatus) -> CompareReceipt {
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));

    let mut deltas = BTreeMap::new();
    deltas.insert(
        Metric::WallMs,
        Delta {
            baseline: 100.0,
            current: match status {
                MetricStatus::Pass => 105.0,
                MetricStatus::Warn => 115.0,
                MetricStatus::Fail => 130.0,
            },
            ratio: match status {
                MetricStatus::Pass => 1.05,
                MetricStatus::Warn => 1.15,
                MetricStatus::Fail => 1.30,
            },
            pct: match status {
                MetricStatus::Pass => 0.05,
                MetricStatus::Warn => 0.15,
                MetricStatus::Fail => 0.30,
            },
            regression: match status {
                MetricStatus::Pass => 0.05,
                MetricStatus::Warn => 0.15,
                MetricStatus::Fail => 0.30,
            },
            statistic: MetricStatistic::Median,
            significance: None,
            status,
        },
    );

    let verdict_status = match status {
        MetricStatus::Pass => VerdictStatus::Pass,
        MetricStatus::Warn => VerdictStatus::Warn,
        MetricStatus::Fail => VerdictStatus::Fail,
    };

    CompareReceipt {
        schema: COMPARE_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        bench: BenchMeta {
            name: "test-bench".to_string(),
            cwd: None,
            command: vec!["echo".to_string()],
            repeat: 3,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        baseline_ref: CompareRef {
            path: Some("baseline.json".to_string()),
            run_id: None,
        },
        current_ref: CompareRef {
            path: Some("current.json".to_string()),
            run_id: None,
        },
        budgets,
        deltas,
        verdict: Verdict {
            status: verdict_status,
            counts: VerdictCounts {
                pass: if status == MetricStatus::Pass { 1 } else { 0 },
                warn: if status == MetricStatus::Warn { 1 } else { 0 },
                fail: if status == MetricStatus::Fail { 1 } else { 0 },
            },
            reasons: match status {
                MetricStatus::Pass => vec![],
                MetricStatus::Warn => vec!["wall_ms_warn".to_string()],
                MetricStatus::Fail => vec!["wall_ms_fail".to_string()],
            },
        },
    }
}

#[test]
fn run_receipt_to_csv_to_markdown_flow() {
    let receipt = make_run_receipt();

    let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
    assert!(csv.contains("test-bench"));
    assert!(csv.contains("100"));
}

#[test]
fn run_receipt_to_jsonl_flow() {
    let receipt = make_run_receipt();

    let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
    assert!(jsonl.contains("\"bench_name\":\"test-bench\""));
}

#[test]
fn compare_receipt_to_csv_to_markdown_flow() {
    let receipt = make_compare_receipt(MetricStatus::Warn);

    let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();
    assert!(csv.contains("test-bench"));
    assert!(csv.contains("wall_ms"));

    let md = render_markdown(&receipt);
    assert!(md.contains("test-bench"));
    assert!(md.contains("wall_ms"));
    assert!(md.contains("⚠️"));
}

#[test]
fn compare_receipt_to_jsonl_to_annotations_flow() {
    let receipt = make_compare_receipt(MetricStatus::Fail);

    let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();
    assert!(jsonl.contains("\"metric\":\"wall_ms\""));
    assert!(jsonl.contains("\"status\":\"fail\""));

    let annotations = github_annotations(&receipt);
    assert_eq!(annotations.len(), 1);
    assert!(annotations[0].starts_with("::error::"));
}

#[test]
fn pass_receipt_renders_correctly() {
    let receipt = make_compare_receipt(MetricStatus::Pass);

    let md = render_markdown(&receipt);
    assert!(md.contains("✅"));
    assert!(md.contains("pass"));

    let annotations = github_annotations(&receipt);
    assert!(annotations.is_empty());
}

#[test]
fn warn_receipt_renders_correctly() {
    let receipt = make_compare_receipt(MetricStatus::Warn);

    let md = render_markdown(&receipt);
    assert!(md.contains("⚠️"));
    assert!(md.contains("warn"));

    let annotations = github_annotations(&receipt);
    assert_eq!(annotations.len(), 1);
    assert!(annotations[0].starts_with("::warning::"));
}

#[test]
fn fail_receipt_renders_correctly() {
    let receipt = make_compare_receipt(MetricStatus::Fail);

    let md = render_markdown(&receipt);
    assert!(md.contains("❌"));
    assert!(md.contains("fail"));

    let annotations = github_annotations(&receipt);
    assert_eq!(annotations.len(), 1);
    assert!(annotations[0].starts_with("::error::"));
}

#[test]
fn export_formats_are_consistent() {
    let receipt = make_compare_receipt(MetricStatus::Warn);

    let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();
    let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();

    assert!(csv.contains("wall_ms"));
    assert!(jsonl.contains("\"metric\":\"wall_ms\""));
}

#[test]
fn markdown_contains_budget_thresholds() {
    let receipt = make_compare_receipt(MetricStatus::Warn);

    let md = render_markdown(&receipt);
    assert!(md.contains("20.0%"));
    assert!(md.contains("lower"));
}

#[test]
fn markdown_contains_reasons() {
    let receipt = make_compare_receipt(MetricStatus::Warn);

    let md = render_markdown(&receipt);
    assert!(md.contains("wall_ms_warn"));
}

#[test]
fn export_prometheus_format() {
    let receipt = make_compare_receipt(MetricStatus::Pass);

    let prom = ExportUseCase::export_compare(&receipt, ExportFormat::Prometheus).unwrap();
    assert!(prom.contains("perfgate_compare_baseline_value"));
    assert!(prom.contains("perfgate_compare_current_value"));
    assert!(prom.contains("metric=\"wall_ms\""));
}

#[test]
fn export_html_format() {
    let receipt = make_compare_receipt(MetricStatus::Pass);

    let html = ExportUseCase::export_compare(&receipt, ExportFormat::Html).unwrap();
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("<table"));
    assert!(html.contains("wall_ms"));
}

#[test]
fn run_export_to_html() {
    let receipt = make_run_receipt();

    let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
    assert!(html.contains("<!doctype html>"));
    assert!(html.contains("test-bench"));
}

#[test]
fn run_export_to_prometheus() {
    let receipt = make_run_receipt();

    let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
    assert!(prom.contains("perfgate_run_wall_ms_median"));
    assert!(prom.contains("bench=\"test-bench\""));
}

#[test]
fn multiple_metrics_in_compare_flow() {
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));
    budgets.insert(Metric::MaxRssKb, Budget::new(0.30, 0.15, Direction::Lower));

    let mut deltas = BTreeMap::new();
    deltas.insert(
        Metric::WallMs,
        Delta {
            baseline: 100.0,
            current: 115.0,
            ratio: 1.15,
            pct: 0.15,
            regression: 0.15,
            statistic: MetricStatistic::Median,
            significance: None,
            status: MetricStatus::Warn,
        },
    );
    deltas.insert(
        Metric::MaxRssKb,
        Delta {
            baseline: 1000.0,
            current: 1500.0,
            ratio: 1.50,
            pct: 0.50,
            regression: 0.50,
            statistic: MetricStatistic::Median,
            significance: None,
            status: MetricStatus::Fail,
        },
    );

    let receipt = CompareReceipt {
        schema: COMPARE_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        bench: BenchMeta {
            name: "multi-metric-bench".to_string(),
            cwd: None,
            command: vec!["test".to_string()],
            repeat: 3,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        baseline_ref: CompareRef {
            path: None,
            run_id: None,
        },
        current_ref: CompareRef {
            path: None,
            run_id: None,
        },
        budgets,
        deltas,
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                pass: 0,
                warn: 1,
                fail: 1,
            },
            reasons: vec!["wall_ms_warn".to_string(), "max_rss_kb_fail".to_string()],
        },
    };

    let md = render_markdown(&receipt);
    assert!(md.contains("wall_ms"));
    assert!(md.contains("max_rss_kb"));

    let annotations = github_annotations(&receipt);
    assert_eq!(annotations.len(), 2);
}
