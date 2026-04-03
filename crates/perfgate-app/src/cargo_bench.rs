//! Cargo bench integration: parse Criterion and libtest bench output into RunReceipts.
//!
//! This module provides:
//! - Criterion JSON parsing (`target/criterion/{bench}/new/estimates.json`)
//! - Libtest bench output parsing (`test ... bench: NNN ns/iter (+/- NNN)`)
//! - A use-case struct that runs `cargo bench` and produces a `RunReceipt`

use crate::Clock;
use perfgate_domain::compute_stats;
use perfgate_types::{BenchMeta, HostInfo, RUN_SCHEMA_V1, RunMeta, RunReceipt, Sample, ToolInfo};
use std::path::{Path, PathBuf};

/// A single benchmark result parsed from Criterion or libtest output.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedBenchmark {
    /// Benchmark name (e.g., "my_group/my_bench")
    pub name: String,
    /// Estimated time per iteration in nanoseconds
    pub estimate_ns: f64,
    /// Standard error or deviation in nanoseconds (if available)
    pub error_ns: Option<f64>,
    /// Source format that produced this result
    pub source: BenchSource,
}

/// Which benchmark framework produced the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BenchSource {
    Criterion,
    Libtest,
}

/// Criterion's `estimates.json` structure (subset).
#[derive(Debug, serde::Deserialize)]
pub struct CriterionEstimates {
    pub mean: Option<CriterionEstimate>,
    pub median: Option<CriterionEstimate>,
    pub slope: Option<CriterionEstimate>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CriterionEstimate {
    /// Confidence interval
    pub confidence_interval: CriterionConfidenceInterval,
    /// Point estimate in nanoseconds
    pub point_estimate: f64,
    /// Standard error in nanoseconds
    pub standard_error: f64,
}

#[derive(Debug, serde::Deserialize)]
pub struct CriterionConfidenceInterval {
    pub confidence_level: f64,
    pub lower_bound: f64,
    pub upper_bound: f64,
}

/// Request for the cargo bench use case.
#[derive(Debug, Clone, Default)]
pub struct CargoBenchRequest {
    /// Specific bench target name (--bench <name>)
    pub bench_target: Option<String>,
    /// Extra args to pass to `cargo bench` (after --)
    pub extra_args: Vec<String>,
    /// Output path for the run receipt
    pub out: Option<PathBuf>,
    /// Optional baseline to compare against
    pub compare_baseline: Option<PathBuf>,
    /// Pretty-print JSON output
    pub pretty: bool,
    /// Override the target directory (default: auto-detect)
    pub target_dir: Option<PathBuf>,
    /// Include hostname hash in host fingerprint
    pub include_hostname_hash: bool,
}

/// Outcome from the cargo bench use case.
#[derive(Debug, Clone)]
pub struct CargoBenchOutcome {
    /// One RunReceipt per discovered benchmark
    pub receipts: Vec<RunReceipt>,
    /// Detection mode used
    pub source: BenchSource,
    /// Total benchmarks discovered
    pub bench_count: usize,
}

// ---------------------------------------------------------------------------
// Criterion parsing
// ---------------------------------------------------------------------------

/// Scan `target/criterion/` for benchmark results and parse them.
///
/// Criterion stores results in:
///   `target/criterion/{group}/{bench}/new/estimates.json`
/// or for ungrouped benches:
///   `target/criterion/{bench}/new/estimates.json`
pub fn scan_criterion_dir(criterion_dir: &Path) -> anyhow::Result<Vec<ParsedBenchmark>> {
    let mut results = Vec::new();
    scan_criterion_recursive(criterion_dir, criterion_dir, &mut results)?;
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

fn scan_criterion_recursive(
    base_dir: &Path,
    current_dir: &Path,
    results: &mut Vec<ParsedBenchmark>,
) -> anyhow::Result<()> {
    let estimates_path = current_dir.join("new").join("estimates.json");
    if estimates_path.is_file() {
        let relative = current_dir
            .strip_prefix(base_dir)
            .unwrap_or(current_dir)
            .to_string_lossy()
            .replace('\\', "/");

        if let Ok(parsed) = parse_criterion_estimates(&estimates_path) {
            results.push(ParsedBenchmark {
                name: relative,
                estimate_ns: parsed.estimate_ns,
                error_ns: parsed.error_ns,
                source: BenchSource::Criterion,
            });
        }
        return Ok(());
    }

    if current_dir.is_dir() {
        let entries = std::fs::read_dir(current_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // Skip internal criterion dirs
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str == "report" || name_str.starts_with('.') {
                    continue;
                }
                scan_criterion_recursive(base_dir, &path, results)?;
            }
        }
    }

    Ok(())
}

struct ParsedEstimate {
    estimate_ns: f64,
    error_ns: Option<f64>,
}

fn parse_criterion_estimates(path: &Path) -> anyhow::Result<ParsedEstimate> {
    let content = std::fs::read_to_string(path)?;
    let estimates: CriterionEstimates = serde_json::from_str(&content)?;

    // Prefer slope > mean > median (slope is the per-iteration estimate from regression)
    let est = estimates
        .slope
        .as_ref()
        .or(estimates.mean.as_ref())
        .or(estimates.median.as_ref())
        .ok_or_else(|| anyhow::anyhow!("no estimate found in {}", path.display()))?;

    Ok(ParsedEstimate {
        estimate_ns: est.point_estimate,
        error_ns: Some(est.standard_error),
    })
}

// ---------------------------------------------------------------------------
// Libtest parsing
// ---------------------------------------------------------------------------

/// Parse libtest bench output lines.
///
/// Format: `test bench_name ... bench:       NNN ns/iter (+/- NNN)`
pub fn parse_libtest_output(output: &str) -> Vec<ParsedBenchmark> {
    let mut results = Vec::new();

    for line in output.lines() {
        if let Some(parsed) = parse_libtest_line(line) {
            results.push(parsed);
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    results
}

fn parse_libtest_line(line: &str) -> Option<ParsedBenchmark> {
    // Pattern: test <name> ... bench:  <number> ns/iter (+/- <number>)
    let line = line.trim();
    if !line.starts_with("test ") {
        return None;
    }

    let rest = &line["test ".len()..];

    // Find "... bench:" separator
    let bench_marker = "... bench:";
    let bench_idx = rest.find(bench_marker)?;
    let name = rest[..bench_idx].trim().to_string();
    let after_bench = &rest[bench_idx + bench_marker.len()..];

    // Parse: "       NNN ns/iter (+/- NNN)"
    let after_bench = after_bench.trim();

    // Extract the number before "ns/iter"
    let ns_iter_idx = after_bench.find("ns/iter")?;
    let ns_str = after_bench[..ns_iter_idx].trim().replace(',', "");
    let estimate_ns: f64 = ns_str.parse().ok()?;

    // Extract the +/- value
    let error_ns = if let Some(paren_start) = after_bench.find("(+/- ") {
        let after_paren = &after_bench[paren_start + "(+/- ".len()..];
        if let Some(paren_end) = after_paren.find(')') {
            let error_str = after_paren[..paren_end].trim().replace(',', "");
            error_str.parse().ok()
        } else {
            None
        }
    } else {
        None
    };

    Some(ParsedBenchmark {
        name,
        estimate_ns,
        error_ns,
        source: BenchSource::Libtest,
    })
}

// ---------------------------------------------------------------------------
// Detect which framework was used
// ---------------------------------------------------------------------------

/// Detect whether Criterion results are available by checking for a recent
/// `target/criterion/` directory with `estimates.json` files.
pub fn detect_criterion(target_dir: &Path) -> bool {
    let criterion_dir = target_dir.join("criterion");
    if !criterion_dir.is_dir() {
        return false;
    }
    // Check if any estimates.json exists
    has_estimates_json(&criterion_dir)
}

fn has_estimates_json(dir: &Path) -> bool {
    let estimates = dir.join("new").join("estimates.json");
    if estimates.is_file() {
        return true;
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str != "report" && !name_str.starts_with('.') && has_estimates_json(&path) {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Convert parsed benchmarks to RunReceipts
// ---------------------------------------------------------------------------

/// Convert a list of parsed benchmarks into a single RunReceipt with
/// one "sample" per benchmark (since Criterion/libtest already aggregate).
pub fn benchmarks_to_receipt(
    benchmarks: &[ParsedBenchmark],
    name: &str,
    tool: &ToolInfo,
    host: &HostInfo,
    clock: &dyn Clock,
    command: &[String],
) -> anyhow::Result<RunReceipt> {
    if benchmarks.is_empty() {
        anyhow::bail!("no benchmarks found");
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    let started_at = clock.now_rfc3339();

    // For the receipt, we create synthetic samples from the parsed data.
    // Each benchmark's estimate_ns is converted to wall_ms.
    let mut samples: Vec<Sample> = Vec::new();

    for bench in benchmarks {
        let wall_ms = (bench.estimate_ns / 1_000_000.0).round().max(1.0) as u64;
        samples.push(Sample {
            wall_ms,
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
    }

    let stats = compute_stats(&samples, None)?;
    let ended_at = clock.now_rfc3339();

    Ok(RunReceipt {
        schema: RUN_SCHEMA_V1.to_string(),
        tool: tool.clone(),
        run: RunMeta {
            id: run_id,
            started_at,
            ended_at,
            host: host.clone(),
        },
        bench: BenchMeta {
            name: name.to_string(),
            cwd: None,
            command: command.to_vec(),
            repeat: benchmarks.len() as u32,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples,
        stats,
    })
}

/// Convert each parsed benchmark into its own RunReceipt (one receipt per benchmark).
pub fn benchmarks_to_individual_receipts(
    benchmarks: &[ParsedBenchmark],
    tool: &ToolInfo,
    host: &HostInfo,
    clock: &dyn Clock,
    command: &[String],
) -> anyhow::Result<Vec<RunReceipt>> {
    let mut receipts = Vec::new();

    for bench in benchmarks {
        let wall_ms = (bench.estimate_ns / 1_000_000.0).round().max(1.0) as u64;

        let sample = Sample {
            wall_ms,
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
        };

        let run_id = uuid::Uuid::new_v4().to_string();
        let ts = clock.now_rfc3339();
        let stats = compute_stats(std::slice::from_ref(&sample), None)?;

        receipts.push(RunReceipt {
            schema: RUN_SCHEMA_V1.to_string(),
            tool: tool.clone(),
            run: RunMeta {
                id: run_id,
                started_at: ts.clone(),
                ended_at: ts,
                host: host.clone(),
            },
            bench: BenchMeta {
                name: bench.name.clone(),
                cwd: None,
                command: command.to_vec(),
                repeat: 1,
                warmup: 0,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![sample],
            stats,
        });
    }

    Ok(receipts)
}

/// Build the `cargo bench` command line.
pub fn build_cargo_bench_command(bench_target: Option<&str>, extra_args: &[String]) -> Vec<String> {
    let mut cmd = vec!["cargo".to_string(), "bench".to_string()];

    if let Some(target) = bench_target {
        cmd.push("--bench".to_string());
        cmd.push(target.to_string());
    }

    if !extra_args.is_empty() {
        cmd.push("--".to_string());
        cmd.extend(extra_args.iter().cloned());
    }

    cmd
}

/// Auto-detect the cargo target directory.
pub fn detect_target_dir() -> PathBuf {
    // Check CARGO_TARGET_DIR first
    if let Ok(dir) = std::env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(dir);
    }

    // Default to ./target
    PathBuf::from("target")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Criterion parsing tests ----

    #[test]
    fn parse_criterion_estimates_with_slope() {
        let json = r#"{
            "mean": {
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 100.0, "upper_bound": 200.0},
                "point_estimate": 150.0,
                "standard_error": 5.0
            },
            "median": {
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 90.0, "upper_bound": 180.0},
                "point_estimate": 140.0,
                "standard_error": 4.0
            },
            "slope": {
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 95.0, "upper_bound": 190.0},
                "point_estimate": 145.0,
                "standard_error": 3.0
            }
        }"#;

        let estimates: CriterionEstimates = serde_json::from_str(json).unwrap();
        // Should prefer slope
        let est = estimates
            .slope
            .as_ref()
            .or(estimates.mean.as_ref())
            .or(estimates.median.as_ref())
            .unwrap();
        assert!((est.point_estimate - 145.0).abs() < f64::EPSILON);
        assert!((est.standard_error - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_criterion_estimates_fallback_to_mean() {
        let json = r#"{
            "mean": {
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 100.0, "upper_bound": 200.0},
                "point_estimate": 150.0,
                "standard_error": 5.0
            },
            "median": {
                "confidence_interval": {"confidence_level": 0.95, "lower_bound": 90.0, "upper_bound": 180.0},
                "point_estimate": 140.0,
                "standard_error": 4.0
            },
            "slope": null
        }"#;

        let estimates: CriterionEstimates = serde_json::from_str(json).unwrap();
        let est = estimates
            .slope
            .as_ref()
            .or(estimates.mean.as_ref())
            .or(estimates.median.as_ref())
            .unwrap();
        assert!((est.point_estimate - 150.0).abs() < f64::EPSILON);
    }

    // ---- Libtest parsing tests ----

    #[test]
    fn parse_libtest_basic_line() {
        let line = "test bench_sort ... bench:       5,000 ns/iter (+/- 150)";
        let result = parse_libtest_line(line).unwrap();
        assert_eq!(result.name, "bench_sort");
        assert!((result.estimate_ns - 5000.0).abs() < f64::EPSILON);
        assert!((result.error_ns.unwrap() - 150.0).abs() < f64::EPSILON);
        assert_eq!(result.source, BenchSource::Libtest);
    }

    #[test]
    fn parse_libtest_no_comma() {
        let line = "test bench_add ... bench:         100 ns/iter (+/- 10)";
        let result = parse_libtest_line(line).unwrap();
        assert_eq!(result.name, "bench_add");
        assert!((result.estimate_ns - 100.0).abs() < f64::EPSILON);
        assert!((result.error_ns.unwrap() - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_libtest_large_number() {
        let line = "test bench_heavy ... bench:   1,234,567 ns/iter (+/- 12,345)";
        let result = parse_libtest_line(line).unwrap();
        assert_eq!(result.name, "bench_heavy");
        assert!((result.estimate_ns - 1_234_567.0).abs() < f64::EPSILON);
        assert!((result.error_ns.unwrap() - 12_345.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_libtest_ignores_non_bench_lines() {
        assert!(parse_libtest_line("running 3 tests").is_none());
        assert!(parse_libtest_line("test bench_ok ... ok").is_none());
        assert!(parse_libtest_line("").is_none());
        assert!(parse_libtest_line("test result: ok").is_none());
    }

    #[test]
    fn parse_libtest_output_multiple_lines() {
        let output = r#"
running 3 tests
test bench_add  ... bench:         100 ns/iter (+/- 10)
test bench_mul  ... bench:         200 ns/iter (+/- 20)
test bench_sort ... bench:       5,000 ns/iter (+/- 150)

test result: ok. 0 passed; 0 failed; 0 ignored; 3 measured; 0 filtered out
"#;
        let results = parse_libtest_output(output);
        assert_eq!(results.len(), 3);
        // Should be sorted by name
        assert_eq!(results[0].name, "bench_add");
        assert_eq!(results[1].name, "bench_mul");
        assert_eq!(results[2].name, "bench_sort");
    }

    #[test]
    fn parse_libtest_empty_output() {
        let output = "running 0 tests\n\ntest result: ok.\n";
        let results = parse_libtest_output(output);
        assert!(results.is_empty());
    }

    // ---- Command building tests ----

    #[test]
    fn build_command_no_args() {
        let cmd = build_cargo_bench_command(None, &[]);
        assert_eq!(cmd, vec!["cargo", "bench"]);
    }

    #[test]
    fn build_command_with_bench_target() {
        let cmd = build_cargo_bench_command(Some("my_bench"), &[]);
        assert_eq!(cmd, vec!["cargo", "bench", "--bench", "my_bench"]);
    }

    #[test]
    fn build_command_with_extra_args() {
        let cmd =
            build_cargo_bench_command(None, &["--features".to_string(), "my-feature".to_string()]);
        assert_eq!(
            cmd,
            vec!["cargo", "bench", "--", "--features", "my-feature"]
        );
    }

    #[test]
    fn build_command_with_both() {
        let cmd = build_cargo_bench_command(Some("my_bench"), &["--nocapture".to_string()]);
        assert_eq!(
            cmd,
            vec!["cargo", "bench", "--bench", "my_bench", "--", "--nocapture"]
        );
    }

    // ---- Detection tests ----

    #[test]
    fn detect_criterion_returns_false_on_missing_dir() {
        assert!(!detect_criterion(Path::new("/nonexistent/target")));
    }

    #[test]
    #[allow(unsafe_code)]
    fn detect_target_dir_default() {
        // When CARGO_TARGET_DIR is not set, should default to "target"
        // SAFETY: This test is the only thread accessing this env var.
        unsafe { std::env::remove_var("CARGO_TARGET_DIR") };
        let dir = detect_target_dir();
        assert_eq!(dir, PathBuf::from("target"));
    }

    // ---- Benchmark to receipt conversion tests ----

    #[test]
    fn benchmarks_to_receipt_empty_fails() {
        struct FakeClock;
        impl Clock for FakeClock {
            fn now_rfc3339(&self) -> String {
                "2024-01-01T00:00:00Z".to_string()
            }
        }

        let tool = ToolInfo {
            name: "perfgate".into(),
            version: "0.1.0".into(),
        };
        let host = HostInfo {
            os: "linux".into(),
            arch: "x86_64".into(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let clock = FakeClock;

        let result = benchmarks_to_receipt(&[], "test", &tool, &host, &clock, &["cargo".into()]);
        assert!(result.is_err());
    }

    #[test]
    fn benchmarks_to_receipt_creates_valid_receipt() {
        struct FakeClock;
        impl Clock for FakeClock {
            fn now_rfc3339(&self) -> String {
                "2024-01-01T00:00:00Z".to_string()
            }
        }

        let benchmarks = vec![
            ParsedBenchmark {
                name: "bench_a".into(),
                estimate_ns: 5_000_000.0, // 5ms
                error_ns: Some(100_000.0),
                source: BenchSource::Criterion,
            },
            ParsedBenchmark {
                name: "bench_b".into(),
                estimate_ns: 10_000_000.0, // 10ms
                error_ns: Some(200_000.0),
                source: BenchSource::Criterion,
            },
        ];

        let tool = ToolInfo {
            name: "perfgate".into(),
            version: "0.1.0".into(),
        };
        let host = HostInfo {
            os: "linux".into(),
            arch: "x86_64".into(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let clock = FakeClock;

        let receipt = benchmarks_to_receipt(
            &benchmarks,
            "cargo-bench",
            &tool,
            &host,
            &clock,
            &["cargo".into(), "bench".into()],
        )
        .unwrap();

        assert_eq!(receipt.schema, "perfgate.run.v1");
        assert_eq!(receipt.bench.name, "cargo-bench");
        assert_eq!(receipt.samples.len(), 2);
        assert_eq!(receipt.bench.repeat, 2);
        assert_eq!(receipt.samples[0].wall_ms, 5);
        assert_eq!(receipt.samples[1].wall_ms, 10);
    }

    #[test]
    fn benchmarks_to_individual_receipts_creates_one_per_bench() {
        struct FakeClock;
        impl Clock for FakeClock {
            fn now_rfc3339(&self) -> String {
                "2024-01-01T00:00:00Z".to_string()
            }
        }

        let benchmarks = vec![
            ParsedBenchmark {
                name: "bench_a".into(),
                estimate_ns: 5_000_000.0,
                error_ns: Some(100_000.0),
                source: BenchSource::Libtest,
            },
            ParsedBenchmark {
                name: "bench_b".into(),
                estimate_ns: 10_000_000.0,
                error_ns: None,
                source: BenchSource::Libtest,
            },
        ];

        let tool = ToolInfo {
            name: "perfgate".into(),
            version: "0.1.0".into(),
        };
        let host = HostInfo {
            os: "linux".into(),
            arch: "x86_64".into(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let clock = FakeClock;

        let receipts = benchmarks_to_individual_receipts(
            &benchmarks,
            &tool,
            &host,
            &clock,
            &["cargo".into(), "bench".into()],
        )
        .unwrap();

        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].bench.name, "bench_a");
        assert_eq!(receipts[0].samples[0].wall_ms, 5);
        assert_eq!(receipts[1].bench.name, "bench_b");
        assert_eq!(receipts[1].samples[0].wall_ms, 10);
    }

    // ---- Criterion file system tests ----

    #[test]
    fn scan_criterion_dir_on_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let criterion_dir = tmp.path().join("criterion");

        // Create a bench result
        let bench_dir = criterion_dir.join("my_group").join("my_bench").join("new");
        std::fs::create_dir_all(&bench_dir).unwrap();
        std::fs::write(
            bench_dir.join("estimates.json"),
            r#"{
                "mean": {
                    "confidence_interval": {"confidence_level": 0.95, "lower_bound": 100.0, "upper_bound": 200.0},
                    "point_estimate": 150.0,
                    "standard_error": 5.0
                },
                "median": {
                    "confidence_interval": {"confidence_level": 0.95, "lower_bound": 90.0, "upper_bound": 180.0},
                    "point_estimate": 140.0,
                    "standard_error": 4.0
                },
                "slope": null
            }"#,
        )
        .unwrap();

        let results = scan_criterion_dir(&criterion_dir).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "my_group/my_bench");
        assert!((results[0].estimate_ns - 150.0).abs() < f64::EPSILON);
        assert_eq!(results[0].source, BenchSource::Criterion);
    }

    #[test]
    fn scan_criterion_dir_multiple_benches() {
        let tmp = tempfile::tempdir().unwrap();
        let criterion_dir = tmp.path().join("criterion");

        let estimates_json = |ns: f64| {
            format!(
                r#"{{"mean": {{
                    "confidence_interval": {{"confidence_level": 0.95, "lower_bound": 0.0, "upper_bound": 1000.0}},
                    "point_estimate": {},
                    "standard_error": 1.0
                }}, "median": null, "slope": null}}"#,
                ns
            )
        };

        // Create two benches
        for (name, ns) in &[("bench_a", 100.0), ("bench_b", 200.0)] {
            let dir = criterion_dir.join(name).join("new");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("estimates.json"), estimates_json(*ns)).unwrap();
        }

        let results = scan_criterion_dir(&criterion_dir).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "bench_a");
        assert_eq!(results[1].name, "bench_b");
    }

    #[test]
    fn scan_criterion_dir_skips_report_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let criterion_dir = tmp.path().join("criterion");

        // Create a "report" directory that should be skipped
        let report_dir = criterion_dir.join("report").join("new");
        std::fs::create_dir_all(&report_dir).unwrap();
        std::fs::write(report_dir.join("estimates.json"), "{}").unwrap();

        let results = scan_criterion_dir(&criterion_dir).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn scan_criterion_dir_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let criterion_dir = tmp.path().join("criterion");
        std::fs::create_dir_all(&criterion_dir).unwrap();

        let results = scan_criterion_dir(&criterion_dir).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn detect_criterion_with_results() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a criterion result
        let bench_dir = tmp.path().join("criterion").join("my_bench").join("new");
        std::fs::create_dir_all(&bench_dir).unwrap();
        std::fs::write(
            bench_dir.join("estimates.json"),
            r#"{"mean": null, "median": null, "slope": null}"#,
        )
        .unwrap();

        assert!(detect_criterion(tmp.path()));
    }

    #[test]
    fn detect_criterion_without_results() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("criterion")).unwrap();
        assert!(!detect_criterion(tmp.path()));
    }

    // ---- Nanosecond to millisecond conversion edge case ----

    #[test]
    fn sub_millisecond_bench_rounds_to_1ms_minimum() {
        struct FakeClock;
        impl Clock for FakeClock {
            fn now_rfc3339(&self) -> String {
                "2024-01-01T00:00:00Z".to_string()
            }
        }

        let benchmarks = vec![ParsedBenchmark {
            name: "fast_bench".into(),
            estimate_ns: 500.0, // 0.0005 ms
            error_ns: None,
            source: BenchSource::Libtest,
        }];

        let tool = ToolInfo {
            name: "perfgate".into(),
            version: "0.1.0".into(),
        };
        let host = HostInfo {
            os: "linux".into(),
            arch: "x86_64".into(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let clock = FakeClock;

        let receipt = benchmarks_to_receipt(
            &benchmarks,
            "test",
            &tool,
            &host,
            &clock,
            &["cargo".into(), "bench".into()],
        )
        .unwrap();

        // Should floor to 1ms minimum, not 0
        assert_eq!(receipt.samples[0].wall_ms, 1);
    }
}
