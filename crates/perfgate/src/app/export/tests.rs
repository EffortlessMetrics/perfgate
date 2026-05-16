use super::*;
use perfgate_types::{
    BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareRef, Delta, Direction, F64Summary, HostInfo,
    Metric, MetricStatistic, MetricStatus, RUN_SCHEMA_V1, RunMeta, Sample, Stats, ToolInfo,
    U64Summary, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

fn create_test_run_receipt() -> RunReceipt {
    RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        run: RunMeta {
            id: "test-run-001".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            ended_at: "2024-01-15T10:00:05Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: "test-benchmark".to_string(),
            cwd: None,
            command: vec!["echo".to_string(), "hello".to_string()],
            repeat: 5,
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
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
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
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
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
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            throughput_per_s: None,
        },
    }
}

fn create_test_compare_receipt() -> CompareReceipt {
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.2, 0.18, Direction::Lower));
    budgets.insert(Metric::MaxRssKb, Budget::new(0.15, 0.135, Direction::Lower));

    let mut deltas = BTreeMap::new();
    deltas.insert(
        Metric::WallMs,
        Delta {
            baseline: 100.0,
            current: 110.0,
            ratio: 1.1,
            pct: 0.1,
            regression: 0.1,
            cv: None,
            noise_threshold: None,
            statistic: MetricStatistic::Median,
            significance: None,
            status: MetricStatus::Pass,
        },
    );
    deltas.insert(
        Metric::MaxRssKb,
        Delta {
            baseline: 1024.0,
            current: 1280.0,
            ratio: 1.25,
            pct: 0.25,
            regression: 0.25,
            cv: None,
            noise_threshold: None,
            statistic: MetricStatistic::Median,
            significance: None,
            status: MetricStatus::Fail,
        },
    );

    CompareReceipt {
        schema: COMPARE_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        bench: BenchMeta {
            name: "alpha-bench".to_string(),
            cwd: None,
            command: vec!["test".to_string()],
            repeat: 5,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        baseline_ref: CompareRef {
            path: Some("baseline.json".to_string()),
            run_id: Some("baseline-001".to_string()),
        },
        current_ref: CompareRef {
            path: Some("current.json".to_string()),
            run_id: Some("current-001".to_string()),
        },
        budgets,
        deltas,
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                pass: 1,
                warn: 0,
                fail: 0,
                skip: 0,
            },
            reasons: vec!["max_rss_kb_fail".to_string()],
        },
    }
}

#[test]
fn test_run_export_csv() {
    let receipt = create_test_run_receipt();
    let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();

    assert!(csv.starts_with("bench_name,wall_ms_median,"));
    assert!(csv.contains("test-benchmark"));
    assert!(csv.contains("100,98,102"));
    assert!(csv.contains("1024"));
    assert!(csv.contains("2024-01-15T10:00:00Z"));
}

#[test]
fn test_run_export_jsonl() {
    let receipt = create_test_run_receipt();
    let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();

    let lines: Vec<&str> = jsonl.trim().split('\n').collect();
    assert_eq!(lines.len(), 1);

    let parsed: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(parsed["bench_name"], "test-benchmark");
    assert_eq!(parsed["wall_ms_median"], 100);
}

#[test]
fn test_compare_export_csv() {
    let receipt = create_test_compare_receipt();
    let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();

    assert!(csv.starts_with("bench_name,metric,baseline_value,"));
    assert!(csv.contains("alpha-bench"));
    assert!(csv.contains("max_rss_kb"));
    assert!(csv.contains("wall_ms"));
    let max_rss_pos = csv.find("max_rss_kb").unwrap();
    let wall_ms_pos = csv.find("wall_ms").unwrap();
    assert!(max_rss_pos < wall_ms_pos);
}

#[test]
fn test_compare_export_jsonl() {
    let receipt = create_test_compare_receipt();
    let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();

    let lines: Vec<&str> = jsonl.trim().split('\n').collect();
    assert_eq!(lines.len(), 2);

    for line in &lines {
        let _: serde_json::Value = serde_json::from_str(line).unwrap();
    }

    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["metric"], "max_rss_kb");
}

#[test]
fn test_csv_escape() {
    assert_eq!(csv_escape("simple"), "simple");
    assert_eq!(csv_escape("has,comma"), "\"has,comma\"");
    assert_eq!(csv_escape("has\"quote"), "\"has\"\"quote\"");
    assert_eq!(csv_escape("has\nnewline"), "\"has\nnewline\"");
}

#[test]
fn test_stable_ordering_across_runs() {
    let receipt = create_test_compare_receipt();

    let csv1 = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();
    let csv2 = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();

    assert_eq!(csv1, csv2, "CSV output should be deterministic");
}

#[test]
fn test_export_format_from_str() {
    assert_eq!(ExportFormat::parse("csv"), Some(ExportFormat::Csv));
    assert_eq!(ExportFormat::parse("CSV"), Some(ExportFormat::Csv));
    assert_eq!(ExportFormat::parse("jsonl"), Some(ExportFormat::Jsonl));
    assert_eq!(ExportFormat::parse("JSONL"), Some(ExportFormat::Jsonl));
    assert_eq!(ExportFormat::parse("html"), Some(ExportFormat::Html));
    assert_eq!(
        ExportFormat::parse("prometheus"),
        Some(ExportFormat::Prometheus)
    );
    assert_eq!(ExportFormat::parse("invalid"), None);
}

#[test]
fn test_run_export_html_and_prometheus() {
    let receipt = create_test_run_receipt();

    let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
    assert!(html.contains("<table"), "html output should contain table");
    assert!(html.contains("test-benchmark"));

    let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
    assert!(prom.contains("perfgate_run_wall_ms_median"));
    assert!(prom.contains("bench=\"test-benchmark\""));
}

#[test]
fn test_compare_export_prometheus() {
    let receipt = create_test_compare_receipt();
    let prom = ExportUseCase::export_compare(&receipt, ExportFormat::Prometheus).unwrap();
    assert!(prom.contains("perfgate_compare_regression_pct"));
    assert!(prom.contains("metric=\"max_rss_kb\""));
}

#[test]
fn test_compare_export_junit() {
    let receipt = create_test_compare_receipt();
    let junit = ExportUseCase::export_compare(&receipt, ExportFormat::JUnit).unwrap();

    assert!(junit.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(junit.contains("<testsuites name=\"perfgate\""));
    assert!(junit.contains("testsuite name=\"alpha-bench\""));
    assert!(junit.contains("testcase name=\"wall_ms\""));
    assert!(junit.contains("testcase name=\"max_rss_kb\""));
    assert!(junit.contains("<failure message=\"Performance regression detected for max_rss_kb\">"));
    assert!(junit.contains("Baseline: 1024.000000"));
    assert!(junit.contains("Current: 1280.000000"));
}

#[test]
fn test_html_escape() {
    assert_eq!(html_escape("simple"), "simple");
    assert_eq!(html_escape("<script>"), "&lt;script&gt;");
    assert_eq!(html_escape("a&b"), "a&amp;b");
    assert_eq!(html_escape("\"quoted\""), "&quot;quoted&quot;");
}

#[test]
fn test_prometheus_escape() {
    assert_eq!(prometheus_escape_label_value("simple"), "simple");
    assert_eq!(prometheus_escape_label_value("has\"quote"), "has\\\"quote");
    assert_eq!(
        prometheus_escape_label_value("has\\backslash"),
        "has\\\\backslash"
    );
}

mod snapshot_tests {
    use super::*;
    use insta::assert_snapshot;

    #[test]
    fn test_run_html_snapshot() {
        let receipt = create_test_run_receipt();
        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert_snapshot!("run_html", html);
    }

    #[test]
    fn test_run_prometheus_snapshot() {
        let receipt = create_test_run_receipt();
        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert_snapshot!("run_prometheus", prom);
    }

    #[test]
    fn test_compare_html_snapshot() {
        let receipt = create_test_compare_receipt();
        let html = ExportUseCase::export_compare(&receipt, ExportFormat::Html).unwrap();
        assert_snapshot!("compare_html", html);
    }

    #[test]
    fn test_compare_prometheus_snapshot() {
        let receipt = create_test_compare_receipt();
        let prom = ExportUseCase::export_compare(&receipt, ExportFormat::Prometheus).unwrap();
        assert_snapshot!("compare_prometheus", prom);
    }
}

mod edge_case_tests {
    use super::*;

    fn create_empty_run_receipt() -> RunReceipt {
        RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "empty-run".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                ended_at: "2024-01-01T00:00:01Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: None,
                    memory_bytes: None,
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: "empty-bench".to_string(),
                cwd: None,
                command: vec!["true".to_string()],
                repeat: 0,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![],
            stats: Stats {
                wall_ms: U64Summary::new(0, 0, 0),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    fn create_empty_compare_receipt() -> CompareReceipt {
        CompareReceipt {
            schema: COMPARE_SCHEMA_V1.to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            bench: BenchMeta {
                name: "empty-bench".to_string(),
                cwd: None,
                command: vec!["true".to_string()],
                repeat: 0,
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
            budgets: BTreeMap::new(),
            deltas: BTreeMap::new(),
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
        }
    }

    fn create_run_receipt_with_bench_name(name: &str) -> RunReceipt {
        let mut receipt = create_empty_run_receipt();
        receipt.bench.name = name.to_string();
        receipt.samples.push(Sample {
            wall_ms: 42,
            exit_code: 0,
            warmup: false,
            timed_out: false,
            cpu_ms: None,
            page_faults: None,
            ctx_switches: None,
            max_rss_kb: None,
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            binary_bytes: None,
            stdout: None,
            stderr: None,
        });
        receipt.stats.wall_ms = U64Summary::new(42, 42, 42);
        receipt
    }

    // --- Empty receipt tests ---

    #[test]
    fn empty_run_receipt_csv_has_header_and_one_row() {
        let receipt = create_empty_run_receipt();
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        let lines: Vec<&str> = csv.trim().split('\n').collect();
        assert_eq!(lines.len(), 2, "should have header + 1 data row");
        assert!(lines[0].starts_with("bench_name,"));
        assert!(csv.contains("empty-bench"));
    }

    #[test]
    fn empty_run_receipt_jsonl_is_valid() {
        let receipt = create_empty_run_receipt();
        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["bench_name"], "empty-bench");
        assert_eq!(parsed["sample_count"], 0);
    }

    #[test]
    fn empty_run_receipt_html_is_valid() {
        let receipt = create_empty_run_receipt();
        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<table"));
        assert!(html.contains("</table>"));
        assert!(html.contains("empty-bench"));
    }

    #[test]
    fn empty_run_receipt_prometheus_is_valid() {
        let receipt = create_empty_run_receipt();
        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("perfgate_run_wall_ms_median"));
        assert!(prom.contains("bench=\"empty-bench\""));
        assert!(prom.contains("perfgate_run_sample_count"));
    }

    #[test]
    fn empty_compare_receipt_csv_has_header_only() {
        let receipt = create_empty_compare_receipt();
        let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();
        let lines: Vec<&str> = csv.trim().split('\n').collect();
        assert_eq!(lines.len(), 1, "should have header only with no deltas");
        assert!(lines[0].starts_with("bench_name,metric,"));
    }

    #[test]
    fn empty_compare_receipt_jsonl_is_empty() {
        let receipt = create_empty_compare_receipt();
        let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();
        assert!(
            jsonl.trim().is_empty(),
            "JSONL should be empty for no deltas"
        );
    }

    #[test]
    fn empty_compare_receipt_html_has_valid_structure() {
        let receipt = create_empty_compare_receipt();
        let html = ExportUseCase::export_compare(&receipt, ExportFormat::Html).unwrap();
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<table"));
        assert!(html.contains("</table>"));
        assert!(html.contains("<thead>"));
        assert!(html.contains("</tbody>"));
    }

    #[test]
    fn empty_compare_receipt_prometheus_is_empty() {
        let receipt = create_empty_compare_receipt();
        let prom = ExportUseCase::export_compare(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(
            prom.trim().is_empty(),
            "Prometheus output should be empty for no deltas"
        );
    }

    // --- CSV special characters tests ---

    #[test]
    fn csv_bench_name_with_comma() {
        let receipt = create_run_receipt_with_bench_name("bench,with,commas");
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        assert!(
            csv.contains("\"bench,with,commas\""),
            "comma-containing bench name should be quoted"
        );
        let lines: Vec<&str> = csv.trim().split('\n').collect();
        assert_eq!(lines.len(), 2, "should still have exactly 2 lines");
    }

    #[test]
    fn csv_bench_name_with_quotes() {
        let receipt = create_run_receipt_with_bench_name("bench\"quoted\"name");
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        assert!(
            csv.contains("\"bench\"\"quoted\"\"name\""),
            "quotes should be escaped as double-quotes in CSV"
        );
    }

    #[test]
    fn csv_bench_name_with_newline() {
        let receipt = create_run_receipt_with_bench_name("bench\nwith\nnewlines");
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        assert!(
            csv.contains("\"bench\nwith\nnewlines\""),
            "newline-containing bench name should be quoted"
        );
    }

    #[test]
    fn csv_bench_name_with_commas_and_quotes() {
        let receipt = create_run_receipt_with_bench_name("a,\"b\",c");
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        // Must be properly escaped per RFC 4180
        assert!(csv.contains("\"a,\"\"b\"\",c\""));
    }

    // --- JSONL unicode tests ---

    #[test]
    fn jsonl_bench_name_with_unicode() {
        let receipt = create_run_receipt_with_bench_name("ベンチマーク-速度");
        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["bench_name"], "ベンチマーク-速度");
    }

    #[test]
    fn jsonl_bench_name_with_emoji() {
        let receipt = create_run_receipt_with_bench_name("bench-🚀-fast");
        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["bench_name"], "bench-🚀-fast");
    }

    #[test]
    fn jsonl_bench_name_with_special_json_chars() {
        let receipt = create_run_receipt_with_bench_name("bench\\with\"special\tchars");
        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["bench_name"], "bench\\with\"special\tchars");
    }

    // --- HTML empty data tests ---

    #[test]
    fn html_run_with_all_optional_metrics_none() {
        let receipt = create_empty_run_receipt();
        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(html.contains("<html>"));
        assert!(html.contains("</html>"));
        // Should not panic or error even with all None optional metrics
        assert!(html.contains("empty-bench"));
    }

    #[test]
    fn html_bench_name_with_html_chars() {
        let receipt = create_run_receipt_with_bench_name("<script>alert('xss')</script>");
        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(
            !html.contains("<script>"),
            "HTML special chars should be escaped"
        );
        assert!(html.contains("&lt;script&gt;"));
    }

    // --- Prometheus metric name tests ---

    #[test]
    fn prometheus_bench_name_with_quotes() {
        let receipt = create_run_receipt_with_bench_name("bench\"name");
        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(
            prom.contains("bench="),
            "Prometheus output should have bench label"
        );
        assert!(
            !prom.contains("bench=\"bench\"name\""),
            "raw quotes should be escaped"
        );
        assert!(prom.contains("bench=\"bench\\\"name\""));
    }

    #[test]
    fn prometheus_bench_name_with_backslash() {
        let receipt = create_run_receipt_with_bench_name("bench\\path");
        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("bench=\"bench\\\\path\""));
    }

    #[test]
    fn prometheus_compare_with_all_metric_types() {
        let mut receipt = create_empty_compare_receipt();
        receipt.bench.name = "full-metrics".to_string();
        receipt.deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 105.0,
                ratio: 1.05,
                pct: 0.05,
                regression: 0.05,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        receipt.deltas.insert(
            Metric::MaxRssKb,
            Delta {
                baseline: 100.0,
                current: 105.0,
                ratio: 1.05,
                pct: 0.05,
                regression: 0.05,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        let prom = ExportUseCase::export_compare(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("metric=\"wall_ms\""));
        assert!(prom.contains("metric=\"max_rss_kb\""));
        assert!(prom.contains("perfgate_compare_baseline_value"));
        assert!(prom.contains("perfgate_compare_current_value"));
        assert!(prom.contains("perfgate_compare_status"));
    }

    // --- Single-sample run receipt ---

    #[test]
    fn single_sample_run_exports_all_formats() {
        let receipt = create_run_receipt_with_bench_name("single");

        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        assert!(csv.contains("single"));
        assert_eq!(csv.trim().lines().count(), 2);

        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["sample_count"], 1);

        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(html.contains("<td>single</td>"));

        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("perfgate_run_sample_count{bench=\"single\"} 1"));
    }

    // --- Huge values ---

    #[test]
    fn huge_values_run_receipt() {
        let mut receipt = create_empty_run_receipt();
        receipt.bench.name = "huge".to_string();
        receipt.stats.wall_ms = U64Summary::new(u64::MAX, u64::MAX - 1, u64::MAX);
        receipt.stats.max_rss_kb = Some(U64Summary::new(u64::MAX, u64::MAX, u64::MAX));
        receipt.stats.io_read_bytes = Some(U64Summary::new(u64::MAX, u64::MAX, u64::MAX));

        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        assert!(csv.contains(&u64::MAX.to_string()));

        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["wall_ms_median"], u64::MAX);

        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(html.contains(&u64::MAX.to_string()));

        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains(&u64::MAX.to_string()));
    }

    // --- Warmup-only samples yield sample_count == 0 ---

    #[test]
    fn warmup_only_samples_count_zero() {
        let mut receipt = create_empty_run_receipt();
        receipt.samples = vec![
            Sample {
                wall_ms: 10,
                exit_code: 0,
                warmup: true,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 11,
                exit_code: 0,
                warmup: true,
                timed_out: false,
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ];

        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["sample_count"], 0);

        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        // sample_count column is second-to-last; verify 0
        let data_line = csv.lines().nth(1).unwrap();
        assert!(
            data_line.contains(",0,"),
            "warmup-only should yield sample_count 0"
        );
    }

    // --- CSV with carriage return ---

    #[test]
    fn csv_bench_name_with_carriage_return() {
        let receipt = create_run_receipt_with_bench_name("bench\rwith\rcr");
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        assert!(
            csv.contains("\"bench\rwith\rcr\""),
            "carriage-return-containing bench name should be quoted"
        );
    }

    // --- CSV compare with special chars in bench name ---

    #[test]
    fn csv_compare_special_chars_in_bench_name() {
        let mut receipt = create_empty_compare_receipt();
        receipt.bench.name = "bench,\"special\"\nname".to_string();
        receipt.deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 105.0,
                ratio: 1.05,
                pct: 0.05,
                regression: 0.05,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status: MetricStatus::Pass,
            },
        );
        let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();
        // RFC 4180: commas/quotes/newlines inside must be quoted, quotes doubled
        assert!(csv.contains("\"bench,\"\"special\"\"\nname\""));
    }

    // --- Unicode bench name across all formats ---

    #[test]
    fn unicode_bench_name_all_formats() {
        let name = "日本語ベンチ_αβγ_🚀";
        let receipt = create_run_receipt_with_bench_name(name);

        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        assert!(csv.contains(name));

        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();
        assert_eq!(parsed["bench_name"], name);

        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(html.contains(name));

        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains(name));
    }

    // --- HTML compare with mixed statuses ---

    #[test]
    fn html_compare_mixed_statuses() {
        let mut receipt = create_empty_compare_receipt();
        receipt.bench.name = "mixed".to_string();
        for (metric, status) in [
            (Metric::WallMs, MetricStatus::Pass),
            (Metric::CpuMs, MetricStatus::Warn),
            (Metric::MaxRssKb, MetricStatus::Fail),
        ] {
            receipt.deltas.insert(
                metric,
                Delta {
                    baseline: 100.0,
                    current: 120.0,
                    ratio: 1.2,
                    pct: 0.2,
                    regression: 0.2,
                    cv: None,
                    noise_threshold: None,
                    statistic: MetricStatistic::Median,
                    significance: None,
                    status,
                },
            );
        }
        let html = ExportUseCase::export_compare(&receipt, ExportFormat::Html).unwrap();
        assert!(html.contains("<td>pass</td>"));
        assert!(html.contains("<td>warn</td>"));
        assert!(html.contains("<td>fail</td>"));
        // 3 data rows
        assert_eq!(html.matches("<tr><td>").count(), 3);
    }

    // --- HTML empty bench name ---

    #[test]
    fn html_empty_bench_name() {
        let receipt = create_run_receipt_with_bench_name("");
        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(html.contains("<td></td>"));
        assert!(html.contains("<html>"));
    }

    // --- Prometheus run with all optional metrics present ---

    #[test]
    fn prometheus_run_all_optional_metrics_present() {
        let mut receipt = create_empty_run_receipt();
        receipt.bench.name = "full".to_string();
        receipt.stats.cpu_ms = Some(U64Summary::new(50, 48, 52));
        receipt.stats.page_faults = Some(U64Summary::new(10, 8, 12));
        receipt.stats.ctx_switches = Some(U64Summary::new(5, 3, 7));
        receipt.stats.max_rss_kb = Some(U64Summary::new(2048, 2000, 2100));
        receipt.stats.io_read_bytes = Some(U64Summary::new(1000, 900, 1100));
        receipt.stats.io_write_bytes = Some(U64Summary::new(500, 400, 600));
        receipt.stats.network_packets = Some(U64Summary::new(10, 8, 12));
        receipt.stats.energy_uj = Some(U64Summary::new(1000, 900, 1100));
        receipt.stats.binary_bytes = Some(U64Summary::new(100000, 99000, 101000));
        receipt.stats.throughput_per_s = Some(F64Summary::new(1234.567890, 1200.0, 1300.0));

        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("perfgate_run_cpu_ms_median{bench=\"full\"} 50"));
        assert!(prom.contains("perfgate_run_page_faults_median{bench=\"full\"} 10"));
        assert!(prom.contains("perfgate_run_ctx_switches_median{bench=\"full\"} 5"));
        assert!(prom.contains("perfgate_run_max_rss_kb_median{bench=\"full\"} 2048"));
        assert!(prom.contains("perfgate_run_io_read_bytes_median{bench=\"full\"} 1000"));
        assert!(prom.contains("perfgate_run_io_write_bytes_median{bench=\"full\"} 500"));
        assert!(prom.contains("perfgate_run_network_packets_median{bench=\"full\"} 10"));
        assert!(prom.contains("perfgate_run_energy_uj_median{bench=\"full\"} 1000"));
        assert!(prom.contains("perfgate_run_binary_bytes_median{bench=\"full\"} 100000"));
        assert!(prom.contains("perfgate_run_throughput_per_s_median{bench=\"full\"} 1234.567890"));
    }

    // --- Prometheus compare status code mapping ---

    #[test]
    fn prometheus_compare_status_codes() {
        let mut receipt = create_empty_compare_receipt();
        receipt.bench.name = "status-test".to_string();
        for (metric, status, expected_code) in [
            (Metric::WallMs, MetricStatus::Pass, "0"),
            (Metric::CpuMs, MetricStatus::Warn, "1"),
            (Metric::MaxRssKb, MetricStatus::Fail, "2"),
        ] {
            receipt.deltas.insert(
                metric,
                Delta {
                    baseline: 100.0,
                    current: 110.0,
                    ratio: 1.1,
                    pct: 0.1,
                    regression: 0.1,
                    cv: None,
                    noise_threshold: None,
                    statistic: MetricStatistic::Median,
                    significance: None,
                    status,
                },
            );
            receipt
                .budgets
                .insert(metric, Budget::new(0.2, 0.15, Direction::Lower));
            let _ = expected_code; // used below
        }

        let prom = ExportUseCase::export_compare(&receipt, ExportFormat::Prometheus).unwrap();
        assert!(prom.contains("status=\"pass\"} 0"));
        assert!(prom.contains("status=\"warn\"} 1"));
        assert!(prom.contains("status=\"fail\"} 2"));
    }

    // --- JSONL compare round-trip field validation ---

    #[test]
    fn jsonl_compare_fields_match_receipt() {
        let receipt = create_test_compare_receipt();
        let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();

        let lines: Vec<&str> = jsonl.trim().lines().collect();
        assert_eq!(lines.len(), receipt.deltas.len());

        for line in lines {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(parsed["bench_name"], "alpha-bench");
            let metric_name = parsed["metric"].as_str().unwrap();
            assert!(
                ["wall_ms", "max_rss_kb"].contains(&metric_name),
                "unexpected metric: {}",
                metric_name
            );
            assert!(parsed["baseline_value"].as_f64().unwrap() > 0.0);
            assert!(parsed["current_value"].as_f64().unwrap() > 0.0);
            let status = parsed["status"].as_str().unwrap();
            assert!(
                ["pass", "warn", "fail"].contains(&status),
                "unexpected status: {}",
                status
            );
        }
    }

    // --- JSONL run round-trip ---

    #[test]
    fn jsonl_run_round_trip() {
        let receipt = create_test_run_receipt();
        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(jsonl.trim()).unwrap();

        assert_eq!(parsed["bench_name"], receipt.bench.name);
        assert_eq!(parsed["wall_ms_median"], receipt.stats.wall_ms.median);
        assert_eq!(parsed["wall_ms_min"], receipt.stats.wall_ms.min);
        assert_eq!(parsed["wall_ms_max"], receipt.stats.wall_ms.max);
        assert_eq!(
            parsed["cpu_ms_median"],
            receipt.stats.cpu_ms.as_ref().unwrap().median
        );
        assert_eq!(
            parsed["max_rss_kb_median"],
            receipt.stats.max_rss_kb.as_ref().unwrap().median
        );
        assert_eq!(
            parsed["sample_count"],
            receipt.samples.iter().filter(|s| !s.warmup).count()
        );
        assert_eq!(parsed["timestamp"], receipt.run.started_at);
    }

    // --- HTML structure tests ---

    #[test]
    fn html_run_all_optional_metrics_present() {
        let mut receipt = create_empty_run_receipt();
        receipt.bench.name = "full-html".to_string();
        receipt.stats.cpu_ms = Some(U64Summary::new(50, 48, 52));
        receipt.stats.io_read_bytes = Some(U64Summary::new(1000, 900, 1100));
        receipt.stats.throughput_per_s = Some(F64Summary::new(999.123456, 900.0, 1100.0));

        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();
        assert!(html.contains("<td>50</td>"));
        assert!(html.contains("<td>1000</td>"));
        assert!(html.contains("999.123456"));
        assert!(html.contains("full-html"));
    }

    // --- CSV escape edge cases ---

    #[test]
    fn csv_escape_empty_string() {
        assert_eq!(csv_escape(""), "");
    }

    #[test]
    fn csv_escape_only_quotes() {
        assert_eq!(csv_escape("\"\"\""), "\"\"\"\"\"\"\"\"");
    }

    #[test]
    fn csv_escape_no_special_chars() {
        assert_eq!(csv_escape("plain-bench_name.v2"), "plain-bench_name.v2");
    }

    // --- Prometheus escape edge cases ---

    #[test]
    fn prometheus_escape_newline_preserved() {
        // Newlines are not escaped by prometheus_escape_label_value
        // (the function only escapes backslash and double-quote)
        let result = prometheus_escape_label_value("a\nb");
        assert_eq!(result, "a\nb");
    }

    #[test]
    fn prometheus_escape_empty() {
        assert_eq!(prometheus_escape_label_value(""), "");
    }

    // --- HTML escape edge cases ---

    #[test]
    fn html_escape_all_special_chars_combined() {
        assert_eq!(
            html_escape("<tag attr=\"val\">&</tag>"),
            "&lt;tag attr=&quot;val&quot;&gt;&amp;&lt;/tag&gt;"
        );
    }

    #[test]
    fn html_escape_empty() {
        assert_eq!(html_escape(""), "");
    }

    // --- ExportFormat::parse edge cases ---

    #[test]
    fn format_parse_prom_alias() {
        assert_eq!(ExportFormat::parse("prom"), Some(ExportFormat::Prometheus));
        assert_eq!(ExportFormat::parse("PROM"), Some(ExportFormat::Prometheus));
    }

    #[test]
    fn format_parse_empty_string() {
        assert_eq!(ExportFormat::parse(""), None);
    }

    // --- Compare CSV threshold values ---

    #[test]
    fn compare_csv_threshold_percentage() {
        let receipt = create_test_compare_receipt();
        let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();
        // Budget threshold 0.2 → exported as 20.000000
        assert!(csv.contains("20.000000"));
        // Budget threshold 0.15 → exported as 15.000000
        assert!(csv.contains("15.000000"));
    }

    // --- Compare regression_pct is percentage ---

    #[test]
    fn compare_regression_pct_is_percentage() {
        let receipt = create_test_compare_receipt();
        let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();

        for line in jsonl.trim().lines() {
            let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
            let metric = parsed["metric"].as_str().unwrap();
            let regression_pct = parsed["regression_pct"].as_f64().unwrap();
            match metric {
                "wall_ms" => {
                    // pct=0.1 → regression_pct=10.0
                    assert!((regression_pct - 10.0).abs() < 0.01);
                }
                "max_rss_kb" => {
                    // pct=0.25 → regression_pct=25.0
                    assert!((regression_pct - 25.0).abs() < 0.01);
                }
                _ => panic!("unexpected metric: {}", metric),
            }
        }
    }
}
