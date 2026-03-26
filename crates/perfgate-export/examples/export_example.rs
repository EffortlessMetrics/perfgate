//! Basic example demonstrating export formats.
//!
//! Run with: cargo run -p perfgate-export --example basic

use perfgate_export::{ExportFormat, ExportUseCase};
use perfgate_types::{
    BenchMeta, Budget, COMPARE_SCHEMA_V1, CompareReceipt, CompareRef, Delta, Direction, HostInfo,
    Metric, MetricStatistic, MetricStatus, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, Stats,
    ToolInfo, U64Summary, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

fn create_run_receipt() -> RunReceipt {
    RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: ToolInfo {
            name: "perfgate".to_string(),
            version: "0.1.0".to_string(),
        },
        run: RunMeta {
            id: "run-001".to_string(),
            started_at: "2024-01-15T10:00:00Z".to_string(),
            ended_at: "2024-01-15T10:00:05Z".to_string(),
            host: HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(8),
                memory_bytes: Some(16 * 1024 * 1024 * 1024),
                hostname_hash: None,
            },
        },
        bench: BenchMeta {
            name: "example-bench".to_string(),
            cwd: None,
            command: vec!["./benchmark".to_string()],
            repeat: 5,
            warmup: 2,
            work_units: Some(1000),
            timeout_ms: None,
        },
        samples: vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(80),
                max_rss_kb: Some(2048),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                page_faults: None,
                ctx_switches: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 105,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                cpu_ms: Some(85),
                max_rss_kb: Some(2100),
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
                page_faults: None,
                ctx_switches: None,
                binary_bytes: None,
                stdout: None,
                stderr: None,
            },
        ],
        stats: Stats {
            wall_ms: U64Summary::new(102, 100, 105),
            cpu_ms: Some(U64Summary::new(82, 80, 85)),
            max_rss_kb: Some(U64Summary::new(2074, 2048, 2100)),
            io_read_bytes: None,
            io_write_bytes: None,
            network_packets: None,
            energy_uj: None,
            page_faults: None,
            ctx_switches: None,
            binary_bytes: None,
            throughput_per_s: None,
        },
    }
}

fn create_compare_receipt() -> CompareReceipt {
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.2, 0.15, Direction::Lower));
    budgets.insert(Metric::MaxRssKb, Budget::new(0.15, 0.10, Direction::Lower));

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
            baseline: 2048.0,
            current: 2560.0,
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
            name: "example-bench".to_string(),
            cwd: None,
            command: vec!["./benchmark".to_string()],
            repeat: 5,
            warmup: 2,
            work_units: None,
            timeout_ms: None,
        },
        baseline_ref: CompareRef {
            path: Some("baseline.json".to_string()),
            run_id: Some("run-001".to_string()),
        },
        current_ref: CompareRef {
            path: Some("current.json".to_string()),
            run_id: Some("run-002".to_string()),
        },
        budgets,
        deltas,
        verdict: Verdict {
            status: VerdictStatus::Fail,
            counts: VerdictCounts {
                pass: 1,
                warn: 0,
                fail: 1,
                skip: 0,
            },
            reasons: vec!["max_rss_kb_fail".to_string()],
        },
    }
}

fn main() {
    println!("=== perfgate-export Basic Example ===\n");

    let run_receipt = create_run_receipt();
    let compare_receipt = create_compare_receipt();

    println!("1. Export RunReceipt to CSV:");
    let csv = ExportUseCase::export_run(&run_receipt, ExportFormat::Csv).unwrap();
    println!("{}", csv);

    println!("2. Export RunReceipt to JSONL:");
    let jsonl = ExportUseCase::export_run(&run_receipt, ExportFormat::Jsonl).unwrap();
    println!("{}", jsonl);

    println!("3. Export RunReceipt to Prometheus format:");
    let prom = ExportUseCase::export_run(&run_receipt, ExportFormat::Prometheus).unwrap();
    println!("{}", prom);

    println!("4. Export CompareReceipt to CSV:");
    let compare_csv = ExportUseCase::export_compare(&compare_receipt, ExportFormat::Csv).unwrap();
    println!("{}", compare_csv);

    println!("5. Export CompareReceipt to JSONL:");
    let compare_jsonl =
        ExportUseCase::export_compare(&compare_receipt, ExportFormat::Jsonl).unwrap();
    println!("{}", compare_jsonl);

    println!("6. Export CompareReceipt to Prometheus format:");
    let compare_prom =
        ExportUseCase::export_compare(&compare_receipt, ExportFormat::Prometheus).unwrap();
    println!("{}", compare_prom);

    println!("7. Parsing export format from string:");
    let formats = vec!["csv", "CSV", "jsonl", "html", "prometheus", "prom"];
    for f in formats {
        match ExportFormat::parse(f) {
            Some(format) => println!("   \"{}\" -> {:?}", f, format),
            None => println!("   \"{}\" -> Unknown", f),
        }
    }

    println!("\n=== Example complete ===");
}
