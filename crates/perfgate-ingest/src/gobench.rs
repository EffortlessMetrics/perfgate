//! Parser for Go benchmark output.
//!
//! Parses text lines produced by `go test -bench . -benchmem`, e.g.:
//! ```text
//! BenchmarkFoo-8    1000    1234 ns/op    567 B/op    3 allocs/op
//! ```
//!
//! Maps `ns/op` to `wall_ms` and `B/op` to `max_rss_kb` (as a proxy for
//! memory usage per operation).

use anyhow::Context;
use perfgate_types::{RunReceipt, Sample, Stats, U64Summary};
use regex::Regex;

use crate::make_receipt;

/// A parsed Go benchmark line.
#[derive(Debug)]
struct GoBenchLine {
    name: String,
    iterations: u64,
    ns_per_op: f64,
    bytes_per_op: Option<u64>,
    allocs_per_op: Option<u64>,
}

/// Parse Go benchmark text output into a `RunReceipt`.
///
/// The parser recognizes lines matching the standard Go benchmark output format:
/// `BenchmarkName-N  iterations  value ns/op  [value B/op  value allocs/op]`
///
/// If multiple benchmark functions are present, only the first is used.
/// Use `name` to override the benchmark name.
pub fn parse_gobench(input: &str, name: Option<&str>) -> anyhow::Result<RunReceipt> {
    let lines = parse_gobench_lines(input)?;

    let first = lines
        .first()
        .context("no benchmark results found in Go bench output")?;

    let bench_name = name
        .map(|n| n.to_string())
        .unwrap_or_else(|| first.name.clone());

    // Convert ns/op to milliseconds.
    let wall_ms = ns_to_ms(first.ns_per_op);

    // Since Go bench gives us a single aggregated result (not individual samples),
    // we create a single sample. The `iterations` field tells us how many runs
    // the Go testing framework performed internally.
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

    let wall_stats = U64Summary {
        median: wall_ms,
        min: wall_ms,
        max: wall_ms,
        // IMPORTANT: Use f64 division here, NOT ns_to_ms(). See the GOTCHA
        // on ns_to_ms — integer truncation would lose sub-ms precision that
        // budget evaluation and significance testing rely on.
        mean: Some(first.ns_per_op / 1_000_000.0),
        stddev: None,
    };

    // Map B/op to max_rss_kb if available (B -> KB).
    let max_rss_kb = first.bytes_per_op.map(|b| {
        let kb = b.div_ceil(1024);
        U64Summary {
            median: kb,
            min: kb,
            max: kb,
            mean: Some(b as f64 / 1024.0),
            stddev: None,
        }
    });

    let stats = Stats {
        wall_ms: wall_stats,
        cpu_ms: None,
        page_faults: None,
        ctx_switches: None,
        max_rss_kb,
        io_read_bytes: None,
        io_write_bytes: None,
        network_packets: None,
        energy_uj: None,
        binary_bytes: None,
        throughput_per_s: None,
        custom_metrics: std::collections::BTreeMap::new(),
    };

    let mut receipt = make_receipt(&bench_name, vec![sample], stats);

    // Store allocs/op info in the bench command metadata for reference.
    if let Some(allocs) = first.allocs_per_op {
        receipt.bench.command = vec![
            format!(
                "(go bench: {} iterations, {} ns/op",
                first.iterations, first.ns_per_op
            ),
            format!("{} allocs/op)", allocs),
        ];
    } else {
        receipt.bench.command = vec![format!(
            "(go bench: {} iterations, {} ns/op)",
            first.iterations, first.ns_per_op
        )];
    }

    Ok(receipt)
}

/// Parse all Go benchmark lines and return them. Supports multiple benchmarks
/// being parsed, though only the first is currently used for the receipt.
fn parse_gobench_lines(input: &str) -> anyhow::Result<Vec<GoBenchLine>> {
    // Match lines like: BenchmarkXxx-8    1000    1234 ns/op
    // Optional: 567 B/op, 3 allocs/op
    let re = Regex::new(
        r"(?m)^(Benchmark\S+)\s+(\d+)\s+([\d.]+)\s+ns/op(?:\s+(\d+)\s+B/op)?(?:\s+(\d+)\s+allocs/op)?",
    )?;

    let mut lines = Vec::new();
    for cap in re.captures_iter(input) {
        let name = cap[1].to_string();
        let iterations: u64 = cap[2].parse().context("invalid iteration count")?;
        let ns_per_op: f64 = cap[3].parse().context("invalid ns/op value")?;
        let bytes_per_op = cap
            .get(4)
            .map(|m| m.as_str().parse::<u64>())
            .transpose()
            .context("invalid B/op value")?;
        let allocs_per_op = cap
            .get(5)
            .map(|m| m.as_str().parse::<u64>())
            .transpose()
            .context("invalid allocs/op value")?;

        lines.push(GoBenchLine {
            name,
            iterations,
            ns_per_op,
            bytes_per_op,
            allocs_per_op,
        });
    }

    Ok(lines)
}

/// Integer ns-to-ms conversion for sample `wall_ms` values (u64).
///
/// GOTCHA: This intentionally truncates to integer milliseconds -- it is only
/// appropriate for per-sample u64 fields where sub-ms precision is not needed.
/// For stats fields (mean, stddev) you MUST use floating-point division
/// (`ns / 1_000_000.0`) to preserve sub-millisecond precision. Using this
/// function for stats would silently destroy the fractional component that
/// downstream budget evaluation and significance testing depend on.
fn ns_to_ms(ns: f64) -> u64 {
    let ms = ns / 1_000_000.0;
    if ms < 1.0 && ms > 0.0 {
        1
    } else {
        ms.round() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::RUN_SCHEMA_V1;

    #[test]
    fn parse_gobench_basic() {
        // 50_000_000 ns = 50 ms
        let input = "BenchmarkFoo-8\t  1000\t  50000000 ns/op\t  567 B/op\t  3 allocs/op\n";
        let receipt = parse_gobench(input, Some("foo-bench")).unwrap();
        assert_eq!(receipt.schema, RUN_SCHEMA_V1);
        assert_eq!(receipt.bench.name, "foo-bench");
        assert_eq!(receipt.samples.len(), 1);
        assert_eq!(receipt.stats.wall_ms.median, 50);
        // 567 B = 1 KB (rounded up)
        assert!(receipt.stats.max_rss_kb.is_some());
        assert_eq!(receipt.stats.max_rss_kb.unwrap().median, 1);
    }

    #[test]
    fn parse_gobench_default_name() {
        let input = "BenchmarkBar-4\t  500\t  2000000 ns/op\n";
        let receipt = parse_gobench(input, None).unwrap();
        assert_eq!(receipt.bench.name, "BenchmarkBar-4");
    }

    #[test]
    fn parse_gobench_no_memory() {
        let input = "BenchmarkSimple-8\t  10000\t  500 ns/op\n";
        let receipt = parse_gobench(input, None).unwrap();
        assert!(receipt.stats.max_rss_kb.is_none());
        // 500 ns = sub-millisecond, should clamp to 1ms
        assert_eq!(receipt.stats.wall_ms.median, 1);
    }

    #[test]
    fn parse_gobench_multiple_benchmarks() {
        let input = "\
BenchmarkA-8\t  1000\t  100000 ns/op\n\
BenchmarkB-8\t  2000\t  200000 ns/op\n";
        // Should use first benchmark
        let receipt = parse_gobench(input, None).unwrap();
        assert_eq!(receipt.bench.name, "BenchmarkA-8");
    }

    #[test]
    fn parse_gobench_with_surrounding_text() {
        // Real go test output has headers and PASS/FAIL lines
        let input = "\
goos: linux
goarch: amd64
pkg: example.com/mypackage
BenchmarkHash-8\t  5000\t  300000 ns/op\t  128 B/op\t  2 allocs/op
PASS
ok  \texample.com/mypackage\t1.523s
";
        let receipt = parse_gobench(input, None).unwrap();
        assert_eq!(receipt.bench.name, "BenchmarkHash-8");
        // 300_000 ns = 0.3 ms -> sub-millisecond clamp to 1
        assert_eq!(receipt.stats.wall_ms.median, 1);
    }

    #[test]
    fn parse_gobench_empty_input() {
        let result = parse_gobench("no benchmark lines here", None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_gobench_fractional_ns() {
        let input = "BenchmarkFrac-8\t  1000\t  1234.56 ns/op\n";
        let receipt = parse_gobench(input, None).unwrap();
        // 1234.56 ns = ~0.001 ms -> should clamp to 1
        assert_eq!(receipt.stats.wall_ms.median, 1);
    }
}
