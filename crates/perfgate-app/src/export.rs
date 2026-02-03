//! Export use case for converting receipts to CSV or JSONL formats.
//!
//! This module provides functionality for exporting run and compare receipts
//! to formats suitable for trend analysis and time-series ingestion.

use perfgate_types::{CompareReceipt, Metric, MetricStatus, RunReceipt};

/// Supported export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// RFC 4180 compliant CSV with header row.
    Csv,
    /// JSON Lines format (one JSON object per line).
    Jsonl,
}

impl ExportFormat {
    /// Parse format from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "csv" => Some(ExportFormat::Csv),
            "jsonl" => Some(ExportFormat::Jsonl),
            _ => None,
        }
    }
}

/// Row structure for RunReceipt export.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RunExportRow {
    pub bench_name: String,
    pub wall_ms_median: u64,
    pub wall_ms_min: u64,
    pub wall_ms_max: u64,
    pub max_rss_kb_median: Option<u64>,
    pub throughput_median: Option<f64>,
    pub sample_count: usize,
    pub timestamp: String,
}

/// Row structure for CompareReceipt export.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CompareExportRow {
    pub bench_name: String,
    pub metric: String,
    pub baseline_value: f64,
    pub current_value: f64,
    pub regression_pct: f64,
    pub status: String,
    pub threshold: f64,
}

/// Use case for exporting receipts to different formats.
pub struct ExportUseCase;

impl ExportUseCase {
    /// Export a RunReceipt to the specified format.
    pub fn export_run(receipt: &RunReceipt, format: ExportFormat) -> anyhow::Result<String> {
        let row = Self::run_to_row(receipt);

        match format {
            ExportFormat::Csv => Self::run_row_to_csv(&row),
            ExportFormat::Jsonl => Self::run_row_to_jsonl(&row),
        }
    }

    /// Export a CompareReceipt to the specified format.
    pub fn export_compare(
        receipt: &CompareReceipt,
        format: ExportFormat,
    ) -> anyhow::Result<String> {
        let rows = Self::compare_to_rows(receipt);

        match format {
            ExportFormat::Csv => Self::compare_rows_to_csv(&rows),
            ExportFormat::Jsonl => Self::compare_rows_to_jsonl(&rows),
        }
    }

    /// Convert RunReceipt to exportable row.
    fn run_to_row(receipt: &RunReceipt) -> RunExportRow {
        let sample_count = receipt.samples.iter().filter(|s| !s.warmup).count();

        RunExportRow {
            bench_name: receipt.bench.name.clone(),
            wall_ms_median: receipt.stats.wall_ms.median,
            wall_ms_min: receipt.stats.wall_ms.min,
            wall_ms_max: receipt.stats.wall_ms.max,
            max_rss_kb_median: receipt.stats.max_rss_kb.as_ref().map(|s| s.median),
            throughput_median: receipt.stats.throughput_per_s.as_ref().map(|s| s.median),
            sample_count,
            timestamp: receipt.run.started_at.clone(),
        }
    }

    /// Convert CompareReceipt to exportable rows (one per metric, sorted by metric name).
    fn compare_to_rows(receipt: &CompareReceipt) -> Vec<CompareExportRow> {
        let mut rows: Vec<CompareExportRow> = receipt
            .deltas
            .iter()
            .map(|(metric, delta)| {
                let threshold = receipt
                    .budgets
                    .get(metric)
                    .map(|b| b.threshold)
                    .unwrap_or(0.0);

                CompareExportRow {
                    bench_name: receipt.bench.name.clone(),
                    metric: metric_to_string(*metric),
                    baseline_value: delta.baseline,
                    current_value: delta.current,
                    regression_pct: delta.pct * 100.0,
                    status: status_to_string(delta.status),
                    threshold: threshold * 100.0,
                }
            })
            .collect();

        // Sort by metric name for stable ordering
        rows.sort_by(|a, b| a.metric.cmp(&b.metric));
        rows
    }

    /// Format RunExportRow as CSV (RFC 4180).
    fn run_row_to_csv(row: &RunExportRow) -> anyhow::Result<String> {
        let mut output = String::new();

        // Header row
        output.push_str("bench_name,wall_ms_median,wall_ms_min,wall_ms_max,max_rss_kb_median,throughput_median,sample_count,timestamp\n");

        // Data row
        output.push_str(&csv_escape(&row.bench_name));
        output.push(',');
        output.push_str(&row.wall_ms_median.to_string());
        output.push(',');
        output.push_str(&row.wall_ms_min.to_string());
        output.push(',');
        output.push_str(&row.wall_ms_max.to_string());
        output.push(',');
        output.push_str(
            &row.max_rss_kb_median
                .map_or(String::new(), |v| v.to_string()),
        );
        output.push(',');
        output.push_str(
            &row.throughput_median
                .map_or(String::new(), |v| format!("{:.6}", v)),
        );
        output.push(',');
        output.push_str(&row.sample_count.to_string());
        output.push(',');
        output.push_str(&csv_escape(&row.timestamp));
        output.push('\n');

        Ok(output)
    }

    /// Format RunExportRow as JSONL.
    fn run_row_to_jsonl(row: &RunExportRow) -> anyhow::Result<String> {
        let json = serde_json::to_string(row)?;
        Ok(format!("{}\n", json))
    }

    /// Format CompareExportRows as CSV (RFC 4180).
    fn compare_rows_to_csv(rows: &[CompareExportRow]) -> anyhow::Result<String> {
        let mut output = String::new();

        // Header row
        output.push_str(
            "bench_name,metric,baseline_value,current_value,regression_pct,status,threshold\n",
        );

        // Data rows
        for row in rows {
            output.push_str(&csv_escape(&row.bench_name));
            output.push(',');
            output.push_str(&csv_escape(&row.metric));
            output.push(',');
            output.push_str(&format!("{:.6}", row.baseline_value));
            output.push(',');
            output.push_str(&format!("{:.6}", row.current_value));
            output.push(',');
            output.push_str(&format!("{:.6}", row.regression_pct));
            output.push(',');
            output.push_str(&csv_escape(&row.status));
            output.push(',');
            output.push_str(&format!("{:.6}", row.threshold));
            output.push('\n');
        }

        Ok(output)
    }

    /// Format CompareExportRows as JSONL.
    fn compare_rows_to_jsonl(rows: &[CompareExportRow]) -> anyhow::Result<String> {
        let mut output = String::new();

        for row in rows {
            let json = serde_json::to_string(row)?;
            output.push_str(&json);
            output.push('\n');
        }

        Ok(output)
    }
}

/// Convert Metric enum to snake_case string.
fn metric_to_string(metric: Metric) -> String {
    match metric {
        Metric::WallMs => "wall_ms".to_string(),
        Metric::MaxRssKb => "max_rss_kb".to_string(),
        Metric::ThroughputPerS => "throughput_per_s".to_string(),
    }
}

/// Convert MetricStatus enum to lowercase string.
fn status_to_string(status: MetricStatus) -> String {
    match status {
        MetricStatus::Pass => "pass".to_string(),
        MetricStatus::Warn => "warn".to_string(),
        MetricStatus::Fail => "fail".to_string(),
    }
}

/// Escape a string for CSV per RFC 4180.
/// If the string contains comma, double quote, or newline, wrap in quotes and escape quotes.
fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, Direction, HostInfo, Metric, MetricStatus, RunMeta,
        Sample, Stats, ToolInfo, U64Summary, Verdict, VerdictCounts, VerdictStatus,
        COMPARE_SCHEMA_V1, RUN_SCHEMA_V1,
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
                    max_rss_kb: Some(1024),
                    stdout: None,
                    stderr: None,
                },
                Sample {
                    wall_ms: 102,
                    exit_code: 0,
                    warmup: false,
                    timed_out: false,
                    max_rss_kb: Some(1028),
                    stdout: None,
                    stderr: None,
                },
            ],
            stats: Stats {
                wall_ms: U64Summary {
                    median: 100,
                    min: 98,
                    max: 102,
                },
                max_rss_kb: Some(U64Summary {
                    median: 1024,
                    min: 1020,
                    max: 1028,
                }),
                throughput_per_s: None,
            },
        }
    }

    fn create_test_compare_receipt() -> CompareReceipt {
        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.2,
                warn_threshold: 0.18,
                direction: Direction::Lower,
            },
        );
        budgets.insert(
            Metric::MaxRssKb,
            Budget {
                threshold: 0.15,
                warn_threshold: 0.135,
                direction: Direction::Lower,
            },
        );

        let mut deltas = BTreeMap::new();
        deltas.insert(
            Metric::WallMs,
            Delta {
                baseline: 100.0,
                current: 110.0,
                ratio: 1.1,
                pct: 0.1,
                regression: 0.1,
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
                    fail: 1,
                },
                reasons: vec!["max_rss_kb: +25% exceeds 15% threshold".to_string()],
            },
        }
    }

    #[test]
    fn test_run_export_csv() {
        let receipt = create_test_run_receipt();
        let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();

        assert!(csv.starts_with("bench_name,wall_ms_median,"));
        assert!(csv.contains("test-benchmark"));
        assert!(csv.contains("100,98,102")); // wall_ms stats
        assert!(csv.contains("1024")); // max_rss_kb median
        assert!(csv.contains("2024-01-15T10:00:00Z"));
    }

    #[test]
    fn test_run_export_jsonl() {
        let receipt = create_test_run_receipt();
        let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();

        // Should be valid JSON on a single line
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
        // max_rss_kb should come before wall_ms (alphabetical)
        let max_rss_pos = csv.find("max_rss_kb").unwrap();
        let wall_ms_pos = csv.find("wall_ms").unwrap();
        assert!(max_rss_pos < wall_ms_pos);
    }

    #[test]
    fn test_compare_export_jsonl() {
        let receipt = create_test_compare_receipt();
        let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();

        let lines: Vec<&str> = jsonl.trim().split('\n').collect();
        assert_eq!(lines.len(), 2); // Two metrics

        // Both lines should be valid JSON
        for line in &lines {
            let _: serde_json::Value = serde_json::from_str(line).unwrap();
        }

        // First line should be max_rss_kb (alphabetical)
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
        assert_eq!(ExportFormat::from_str("csv"), Some(ExportFormat::Csv));
        assert_eq!(ExportFormat::from_str("CSV"), Some(ExportFormat::Csv));
        assert_eq!(ExportFormat::from_str("jsonl"), Some(ExportFormat::Jsonl));
        assert_eq!(ExportFormat::from_str("JSONL"), Some(ExportFormat::Jsonl));
        assert_eq!(ExportFormat::from_str("invalid"), None);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use perfgate_types::{
        BenchMeta, Budget, CompareRef, Delta, Direction, F64Summary, HostInfo, Metric,
        MetricStatus, RunMeta, Sample, Stats, ToolInfo, U64Summary, Verdict, VerdictCounts,
        VerdictStatus, COMPARE_SCHEMA_V1, RUN_SCHEMA_V1,
    };
    use proptest::prelude::*;
    use std::collections::BTreeMap;

    // Strategy for generating valid non-empty strings (for names, IDs, etc.)
    fn non_empty_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,20}".prop_map(|s| s)
    }

    // Strategy for generating valid RFC3339 timestamps
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

    // Strategy for ToolInfo
    fn tool_info_strategy() -> impl Strategy<Value = ToolInfo> {
        (non_empty_string(), non_empty_string())
            .prop_map(|(name, version)| ToolInfo { name, version })
    }

    // Strategy for HostInfo
    fn host_info_strategy() -> impl Strategy<Value = HostInfo> {
        (non_empty_string(), non_empty_string()).prop_map(|(os, arch)| HostInfo {
            os,
            arch,
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        })
    }

    // Strategy for RunMeta
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

    // Strategy for BenchMeta
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

    // Strategy for Sample
    fn sample_strategy() -> impl Strategy<Value = Sample> {
        (
            0u64..100000,
            -128i32..128,
            any::<bool>(),
            any::<bool>(),
            proptest::option::of(0u64..1000000),
        )
            .prop_map(
                |(wall_ms, exit_code, warmup, timed_out, max_rss_kb)| Sample {
                    wall_ms,
                    exit_code,
                    warmup,
                    timed_out,
                    max_rss_kb,
                    stdout: None,
                    stderr: None,
                },
            )
    }

    // Strategy for U64Summary
    fn u64_summary_strategy() -> impl Strategy<Value = U64Summary> {
        (0u64..1000000, 0u64..1000000, 0u64..1000000).prop_map(|(a, b, c)| {
            let mut vals = [a, b, c];
            vals.sort();
            U64Summary {
                min: vals[0],
                median: vals[1],
                max: vals[2],
            }
        })
    }

    // Strategy for F64Summary - using finite positive floats
    fn f64_summary_strategy() -> impl Strategy<Value = F64Summary> {
        (0.0f64..1000000.0, 0.0f64..1000000.0, 0.0f64..1000000.0).prop_map(|(a, b, c)| {
            let mut vals = [a, b, c];
            vals.sort_by(|x, y| x.partial_cmp(y).unwrap());
            F64Summary {
                min: vals[0],
                median: vals[1],
                max: vals[2],
            }
        })
    }

    // Strategy for Stats
    fn stats_strategy() -> impl Strategy<Value = Stats> {
        (
            u64_summary_strategy(),
            proptest::option::of(u64_summary_strategy()),
            proptest::option::of(f64_summary_strategy()),
        )
            .prop_map(|(wall_ms, max_rss_kb, throughput_per_s)| Stats {
                wall_ms,
                max_rss_kb,
                throughput_per_s,
            })
    }

    // Strategy for RunReceipt
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

    // Strategy for Direction
    fn direction_strategy() -> impl Strategy<Value = Direction> {
        prop_oneof![Just(Direction::Lower), Just(Direction::Higher),]
    }

    // Strategy for Budget
    fn budget_strategy() -> impl Strategy<Value = Budget> {
        (0.01f64..1.0, 0.01f64..1.0, direction_strategy()).prop_map(
            |(threshold, warn_factor, direction)| {
                let warn_threshold = threshold * warn_factor;
                Budget {
                    threshold,
                    warn_threshold,
                    direction,
                }
            },
        )
    }

    // Strategy for MetricStatus
    fn metric_status_strategy() -> impl Strategy<Value = MetricStatus> {
        prop_oneof![
            Just(MetricStatus::Pass),
            Just(MetricStatus::Warn),
            Just(MetricStatus::Fail),
        ]
    }

    // Strategy for Delta
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
                    status,
                }
            },
        )
    }

    // Strategy for VerdictStatus
    fn verdict_status_strategy() -> impl Strategy<Value = VerdictStatus> {
        prop_oneof![
            Just(VerdictStatus::Pass),
            Just(VerdictStatus::Warn),
            Just(VerdictStatus::Fail),
        ]
    }

    // Strategy for VerdictCounts
    fn verdict_counts_strategy() -> impl Strategy<Value = VerdictCounts> {
        (0u32..10, 0u32..10, 0u32..10).prop_map(|(pass, warn, fail)| VerdictCounts {
            pass,
            warn,
            fail,
        })
    }

    // Strategy for Verdict
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

    // Strategy for Metric
    fn metric_strategy() -> impl Strategy<Value = Metric> {
        prop_oneof![
            Just(Metric::WallMs),
            Just(Metric::MaxRssKb),
            Just(Metric::ThroughputPerS),
        ]
    }

    // Strategy for BTreeMap<Metric, Budget>
    fn budgets_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Budget>> {
        proptest::collection::btree_map(metric_strategy(), budget_strategy(), 1..4)
    }

    // Strategy for BTreeMap<Metric, Delta>
    fn deltas_map_strategy() -> impl Strategy<Value = BTreeMap<Metric, Delta>> {
        proptest::collection::btree_map(metric_strategy(), delta_strategy(), 1..4)
    }

    // Strategy for CompareRef
    fn compare_ref_strategy() -> impl Strategy<Value = CompareRef> {
        (
            proptest::option::of(non_empty_string()),
            proptest::option::of(non_empty_string()),
        )
            .prop_map(|(path, run_id)| CompareRef { path, run_id })
    }

    // Strategy for CompareReceipt
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
                |(tool, bench, baseline_ref, current_ref, budgets, deltas, verdict)| {
                    CompareReceipt {
                        schema: COMPARE_SCHEMA_V1.to_string(),
                        tool,
                        bench,
                        baseline_ref,
                        current_ref,
                        budgets,
                        deltas,
                        verdict,
                    }
                },
            )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// Property: CSV export should always produce valid CSV with expected columns
        #[test]
        fn run_export_csv_has_header_and_data(receipt in run_receipt_strategy()) {
            let csv = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();

            // Check header is present
            prop_assert!(csv.starts_with("bench_name,wall_ms_median,wall_ms_min,wall_ms_max,max_rss_kb_median,throughput_median,sample_count,timestamp\n"));

            // Check we have exactly 2 lines (header + data)
            let lines: Vec<&str> = csv.trim().split('\n').collect();
            prop_assert_eq!(lines.len(), 2);

            // Check bench name is in output (may be quoted in CSV)
            let bench_in_csv = csv.contains(&receipt.bench.name) || csv.contains(&format!("\"{}\"", receipt.bench.name));
            prop_assert!(bench_in_csv, "CSV should contain bench name");
        }

        /// Property: JSONL export should produce valid JSON
        #[test]
        fn run_export_jsonl_is_valid_json(receipt in run_receipt_strategy()) {
            let jsonl = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();

            let lines: Vec<&str> = jsonl.trim().split('\n').collect();
            prop_assert_eq!(lines.len(), 1);

            // Should parse as valid JSON
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(lines[0]);
            prop_assert!(parsed.is_ok());

            let json = parsed.unwrap();
            prop_assert_eq!(json["bench_name"].as_str().unwrap(), receipt.bench.name);
        }

        /// Property: Compare export CSV should have all metrics sorted alphabetically
        #[test]
        fn compare_export_csv_metrics_sorted(receipt in compare_receipt_strategy()) {
            let csv = ExportUseCase::export_compare(&receipt, ExportFormat::Csv).unwrap();

            // Skip header line and collect data lines
            let lines: Vec<&str> = csv.trim().split('\n').skip(1).collect();

            // Extract metric names from each line (second column)
            let mut metrics: Vec<String> = vec![];
            for line in &lines {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() > 1 {
                    metrics.push(parts[1].trim_matches('"').to_string());
                }
            }

            // Verify metrics are sorted alphabetically
            let mut sorted_metrics = metrics.clone();
            sorted_metrics.sort();

            prop_assert_eq!(metrics, sorted_metrics, "Metrics should be sorted alphabetically");
        }

        /// Property: Compare export JSONL should produce one line per metric
        #[test]
        fn compare_export_jsonl_line_per_metric(receipt in compare_receipt_strategy()) {
            let jsonl = ExportUseCase::export_compare(&receipt, ExportFormat::Jsonl).unwrap();

            let lines: Vec<&str> = jsonl.trim().split('\n').filter(|s| !s.is_empty()).collect();
            prop_assert_eq!(lines.len(), receipt.deltas.len());

            // All lines should be valid JSON
            for line in &lines {
                let parsed: Result<serde_json::Value, _> = serde_json::from_str(line);
                prop_assert!(parsed.is_ok());
            }
        }

        /// Property: Export should be deterministic (same input = same output)
        #[test]
        fn export_is_deterministic(receipt in run_receipt_strategy()) {
            let csv1 = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
            let csv2 = ExportUseCase::export_run(&receipt, ExportFormat::Csv).unwrap();
            prop_assert_eq!(csv1, csv2);

            let jsonl1 = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
            let jsonl2 = ExportUseCase::export_run(&receipt, ExportFormat::Jsonl).unwrap();
            prop_assert_eq!(jsonl1, jsonl2);
        }
    }
}
