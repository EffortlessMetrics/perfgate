use super::*;
use perfgate_types::{
    BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareRef, Delta, Direction, F64Summary, HostInfo,
    Metric, MetricStatistic, MetricStatus, RUN_SCHEMA_V1, RunMeta, Sample, Stats, ToolInfo,
    U64Summary, Verdict, VerdictCounts, VerdictStatus,
};
use proptest::prelude::*;
use std::collections::BTreeMap;

fn non_empty_string() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,20}".prop_map(|s| s)
}

fn rfc3339_timestamp() -> impl Strategy<Value = String> {
    (
        2020u32..2030,
        1u32..13,
        1u32..29,
        0u32..24,
        0u32..60,
        0u32..60,
    )
        .prop_map(|(year, month, day, hour, min, sec)| {
            format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
                year, month, day, hour, min, sec
            )
        })
}

fn tool_info_strategy() -> impl Strategy<Value = ToolInfo> {
    (non_empty_string(), non_empty_string()).prop_map(|(name, version)| ToolInfo { name, version })
}

fn host_info_strategy() -> impl Strategy<Value = HostInfo> {
    (non_empty_string(), non_empty_string()).prop_map(|(os, arch)| HostInfo {
        os,
        arch,
        cpu_count: None,
        memory_bytes: None,
        hostname_hash: None,
    })
}

fn run_meta_strategy() -> impl Strategy<Value = RunMeta> {
    (
        non_empty_string(),
        rfc3339_timestamp(),
        rfc3339_timestamp(),
        host_info_strategy(),
    )
        .prop_map(|(id, started_at, ended_at, host)| RunMeta {
            id,
            started_at,
            ended_at,
            host,
        })
}

fn bench_meta_strategy() -> impl Strategy<Value = BenchMeta> {
    (
        non_empty_string(),
        proptest::option::of(non_empty_string()),
        proptest::collection::vec(non_empty_string(), 1..5),
        1u32..100,
        0u32..10,
        proptest::option::of(1u64..10000),
        proptest::option::of(100u64..60000),
    )
        .prop_map(
            |(name, cwd, command, repeat, warmup, work_units, timeout_ms)| BenchMeta {
                name,
                cwd,
                command,
                repeat,
                warmup,
                work_units,
                timeout_ms,
            },
        )
}

fn sample_strategy() -> impl Strategy<Value = Sample> {
    (
        0u64..100000,
        -128i32..128,
        any::<bool>(),
        any::<bool>(),
        (
            proptest::option::of(0u64..1000000), // cpu_ms
            proptest::option::of(0u64..1000000), // page_faults
            proptest::option::of(0u64..1000000), // ctx_switches
            proptest::option::of(0u64..1000000), // max_rss_kb
        ),
        (
            proptest::option::of(0u64..1000000),   // io_read_bytes
            proptest::option::of(0u64..1000000),   // io_write_bytes
            proptest::option::of(0u64..1000000),   // network_packets
            proptest::option::of(0u64..1000000),   // energy_uj
            proptest::option::of(0u64..100000000), // binary_bytes
        ),
    )
        .prop_map(
            |(
                wall_ms,
                exit_code,
                warmup,
                timed_out,
                (cpu_ms, page_faults, ctx_switches, max_rss_kb),
                (io_read_bytes, io_write_bytes, network_packets, energy_uj, binary_bytes),
            )| Sample {
                wall_ms,
                exit_code,
                warmup,
                timed_out,
                cpu_ms,
                page_faults,
                ctx_switches,
                max_rss_kb,
                io_read_bytes,
                io_write_bytes,
                network_packets,
                energy_uj,
                binary_bytes,
                stdout: None,
                stderr: None,
            },
        )
}

fn u64_summary_strategy() -> impl Strategy<Value = U64Summary> {
    (0u64..1000000, 0u64..1000000, 0u64..1000000).prop_map(|(a, b, c)| {
        let mut vals = [a, b, c];
        vals.sort();
        U64Summary::new(vals[1], vals[0], vals[2])
    })
}

fn f64_summary_strategy() -> impl Strategy<Value = F64Summary> {
    (0.0f64..1000000.0, 0.0f64..1000000.0, 0.0f64..1000000.0).prop_map(|(a, b, c)| {
        let mut vals = [a, b, c];
        vals.sort_by(|x, y| x.partial_cmp(y).unwrap());
        F64Summary::new(vals[1], vals[0], vals[2])
    })
}

fn stats_strategy() -> impl Strategy<Value = Stats> {
    (
        u64_summary_strategy(),
        (
            proptest::option::of(u64_summary_strategy()), // cpu_ms
            proptest::option::of(u64_summary_strategy()), // page_faults
            proptest::option::of(u64_summary_strategy()), // ctx_switches
            proptest::option::of(u64_summary_strategy()), // max_rss_kb
        ),
        (
            proptest::option::of(u64_summary_strategy()), // io_read_bytes
            proptest::option::of(u64_summary_strategy()), // io_write_bytes
            proptest::option::of(u64_summary_strategy()), // network_packets
            proptest::option::of(u64_summary_strategy()), // energy_uj
            proptest::option::of(u64_summary_strategy()), // binary_bytes
        ),
        proptest::option::of(f64_summary_strategy()),
    )
        .prop_map(
            |(
                wall_ms,
                (cpu_ms, page_faults, ctx_switches, max_rss_kb),
                (io_read_bytes, io_write_bytes, network_packets, energy_uj, binary_bytes),
                throughput_per_s,
            )| Stats {
                wall_ms,
                cpu_ms,
                page_faults,
                ctx_switches,
                max_rss_kb,
                io_read_bytes,
                io_write_bytes,
                network_packets,
                energy_uj,
                binary_bytes,
                throughput_per_s,
            },
        )
}

fn run_receipt_strategy() -> impl Strategy<Value = RunReceipt> {
    (
        tool_info_strategy(),
        run_meta_strategy(),
        bench_meta_strategy(),
        proptest::collection::vec(sample_strategy(), 1..10),
        stats_strategy(),
    )
        .prop_map(|(tool, run, bench, samples, stats)| RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool,
            run,
            bench,
            samples,
            stats,
        })
}

fn direction_strategy() -> impl Strategy<Value = Direction> {
    prop_oneof![Just(Direction::Lower), Just(Direction::Higher),]
}

fn budget_strategy() -> impl Strategy<Value = Budget> {
    (0.01f64..1.0, 0.01f64..1.0, direction_strategy()).prop_map(
        |(threshold, warn_factor, direction)| {
            let warn_threshold = threshold * warn_factor;
            Budget {
                noise_threshold: None,
                noise_policy: perfgate_types::NoisePolicy::Ignore,
                threshold,
                warn_threshold,
                direction,
            }
        },
    )
}

fn metric_status_strategy() -> impl Strategy<Value = MetricStatus> {
    prop_oneof![
        Just(MetricStatus::Pass),
        Just(MetricStatus::Warn),
        Just(MetricStatus::Fail),
        Just(MetricStatus::Skip),
    ]
}

fn delta_strategy() -> impl Strategy<Value = Delta> {
    (0.1f64..10000.0, 0.1f64..10000.0, metric_status_strategy()).prop_map(
        |(baseline, current, status)| {
            let ratio = current / baseline;
            let pct = (current - baseline) / baseline;
            let regression = if pct > 0.0 { pct } else { 0.0 };
            Delta {
                baseline,
                current,
                ratio,
                pct,
                regression,
                cv: None,
                noise_threshold: None,
                statistic: MetricStatistic::Median,
                significance: None,
                status,
            }
        },
    )
}

fn verdict_status_strategy() -> impl Strategy<Value = VerdictStatus> {
    prop_oneof![
        Just(VerdictStatus::Pass),
        Just(VerdictStatus::Warn),
        Just(VerdictStatus::Fail),
        Just(VerdictStatus::Skip),
    ]
}

fn verdict_counts_strategy() -> impl Strategy<Value = VerdictCounts> {
    (0u32..10, 0u32..10, 0u32..10, 0u32..10).prop_map(|(pass, warn, fail, skip)| VerdictCounts {
        pass,
        warn,
        fail,
        skip,
    })
}

fn verdict_strategy() -> impl Strategy<Value = Verdict> {
    (
        verdict_status_strategy(),
        verdict_counts_strategy(),
        proptest::collection::vec("[a-zA-Z0-9 ]{1,50}", 0..5),
    )
        .prop_map(|(status, counts, reasons)| Verdict {
            status,
            counts,
            reasons,
        })
}

fn metric_strategy() -> impl Strategy<Value = Metric> {
    prop_oneof![
        Just(Metric::BinaryBytes),
        Just(Metric::CpuMs),
        Just(Metric::CtxSwitches),
        Just(Metric::IoReadBytes),
        Just(Metric::IoWriteBytes),
        Just(Metric::MaxRssKb),
        Just(Metric::NetworkPackets),
        Just(Metric::PageFaults),
        Just(Metric::ThroughputPerS),
        Just(Metric::WallMs),
    ]
}

fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
    proptest::collection::btree_map(metric_strategy(), budget_strategy(), 1..8)
}

fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
    proptest::collection::btree_map(metric_strategy(), delta_strategy(), 1..8)
}

fn compare_ref_strategy() -> impl Strategy<Value = CompareRef> {
    (
        proptest::option::of(non_empty_string()),
        proptest::option::of(non_empty_string()),
    )
        .prop_map(|(path, run_id)| CompareRef { path, run_id })
}

fn compare_receipt_strategy() -> impl Strategy<Value = CompareReceipt> {
    (
        tool_info_strategy(),
        bench_meta_strategy(),
        compare_ref_strategy(),
        compare_ref_strategy(),
        budgets_map_strategy(),
        deltas_map_strategy(),
        verdict_strategy(),
    )
        .prop_map(
            |(tool, bench, baseline_ref, current_ref, budgets, deltas, verdict)| CompareReceipt {
                schema: COMPARE_SCHEMA_V1.to_string(),
                tool,
                bench,
                baseline_ref,
                current_ref,
                budgets,
                deltas,
                verdict,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn run_export_csv_has_header_and_data(receipt in run_receipt_strategy()) {
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();

        prop_assert!(csv.starts_with("bench_name,wall_ms_median,wall_ms_min,wall_ms_max,binary_bytes_median,cpu_ms_median,ctx_switches_median,max_rss_kb_median,page_faults_median,io_read_bytes_median,io_write_bytes_median,network_packets_median,energy_uj_median,throughput_median,sample_count,timestamp\n"));

        let lines: Vec<&str> = csv.trim().split('\n').collect();
        prop_assert_eq!(lines.len(), 2);

        let bench_in_csv = csv.contains(&receipt.bench.name) || csv.contains(&format!("\"{}\"", receipt.bench.name));
        prop_assert!(bench_in_csv, "CSV should contain bench name");
    }

    #[test]
    fn run_export_jsonl_is_valid_json(receipt in run_receipt_strategy()) {
        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();

        let lines: Vec<&str> = jsonl.trim().split('\n').collect();
        prop_assert_eq!(lines.len(), 1);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(lines[0]);
        prop_assert!(parsed.is_ok());

        let json = parsed.unwrap();
        prop_assert_eq!(json["bench_name"].as_str().unwrap(), receipt.bench.name);
    }

    #[test]
    fn compare_export_csv_metrics_sorted(receipt in compare_receipt_strategy()) {
        let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();

        let lines: Vec<&str> = csv.trim().split('\n').skip(1).collect();

        let mut metrics: Vec<String> = vec![];
        for line in &lines {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() > 1 {
                metrics.push(parts[1].trim_matches('"').to_string());
            }
        }

        let mut sorted_metrics = metrics.clone();
        sorted_metrics.sort();

        prop_assert_eq!(metrics, sorted_metrics, "Metrics should be sorted alphabetically");
    }

    #[test]
    fn compare_export_jsonl_line_per_metric(receipt in compare_receipt_strategy()) {
        let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();

        let lines: Vec<&str> = jsonl.trim().split('\n').filter(|s| !s.is_empty()).collect();
        prop_assert_eq!(lines.len(), receipt.deltas.len());

        for line in &lines {
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
            prop_assert!(parsed.is_ok());
        }
    }

    #[test]
    fn export_is_deterministic(receipt in run_receipt_strategy()) {
        let csv1 = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        let csv2 = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
        prop_assert_eq!(csv1, csv2);

        let jsonl1 = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        let jsonl2 = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
        prop_assert_eq!(jsonl1, jsonl2);
    }

    #[test]
    fn html_output_contains_valid_structure(receipt in run_receipt_strategy()) {
        let html = ExportUseCase::export_run(&receipt, ExportFormat::Html).unwrap();

        prop_assert!(html.starts_with("<!doctype html>"));
        prop_assert!(html.contains("<html>"));
        prop_assert!(html.contains("</html>"));
        prop_assert!(html.contains("<table"));
        prop_assert!(html.contains("</table>"));
        prop_assert!(html.contains(&receipt.bench.name));
    }

    #[test]
    fn prometheus_output_valid_format(receipt in run_receipt_strategy()) {
        let prom = ExportUseCase::export_run(&receipt, ExportFormat::Prometheus).unwrap();

        prop_assert!(prom.contains("perfgate_run_wall_ms_median"));
        let bench_label = format!("bench=\"{}\"", receipt.bench.name);
        prop_assert!(prom.contains(&bench_label));

        for line in prom.lines() {
            if !line.is_empty() {
                let has_open = line.chars().any(|c| c == '{');
                let has_close = line.chars().any(|c| c == '}');
                prop_assert!(has_open, "Prometheus line should contain opening brace");
                prop_assert!(has_close, "Prometheus line should contain closing brace");
            }
        }
    }

    #[test]
    fn csv_escape_preserves_content(receipt in run_receipt_strategy()) {
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();

        let quoted_bench = format!("\"{}\"", receipt.bench.name);
        prop_assert!(csv.contains(&receipt.bench.name) || csv.contains(&quoted_bench));

        for line in csv.lines() {
            let quoted_count = line.matches('"').count();
            prop_assert!(quoted_count % 2 == 0, "Quotes should be balanced in CSV");
        }
    }
}
