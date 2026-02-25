//! Basic example demonstrating paired statistics for A/B comparison.
//!
//! Run with: cargo run -p perfgate-paired --example basic

use perfgate_paired::{PairedError, compare_paired_stats, compute_paired_stats};
use perfgate_types::{PairedSample, PairedSampleHalf};

fn make_half(wall_ms: u64) -> PairedSampleHalf {
    PairedSampleHalf {
        wall_ms,
        exit_code: 0,
        timed_out: false,
        max_rss_kb: None,
        stdout: None,
        stderr: None,
    }
}

fn make_sample(idx: u32, baseline_ms: u64, current_ms: u64) -> PairedSample {
    PairedSample {
        pair_index: idx,
        warmup: false,
        baseline: make_half(baseline_ms),
        current: make_half(current_ms),
        wall_diff_ms: current_ms as i64 - baseline_ms as i64,
        rss_diff_kb: None,
    }
}

fn main() -> Result<(), PairedError> {
    println!("=== perfgate-paired Basic Example ===\n");

    println!("1. Computing paired statistics for improvement:");
    let improvement_samples = vec![
        make_sample(0, 100, 90),
        make_sample(1, 105, 95),
        make_sample(2, 110, 100),
        make_sample(3, 95, 85),
        make_sample(4, 102, 92),
    ];

    let stats = compute_paired_stats(&improvement_samples, None)?;
    println!("   Baseline median: {} ms", stats.baseline_wall_ms.median);
    println!("   Current median: {} ms", stats.current_wall_ms.median);
    println!("   Mean diff: {:.2} ms", stats.wall_diff_ms.mean);
    println!("   Median diff: {:.2} ms", stats.wall_diff_ms.median);
    println!("   Std dev: {:.2} ms", stats.wall_diff_ms.std_dev);

    println!("\n2. Comparing paired statistics (with significance):");
    let comparison = compare_paired_stats(&stats);
    println!("   Mean diff: {:.2} ms", comparison.mean_diff_ms);
    println!("   % change: {:.2}%", comparison.pct_change * 100.0);
    println!(
        "   95% CI: [{:.2}, {:.2}] ms",
        comparison.ci_95_lower, comparison.ci_95_upper
    );
    println!("   Std error: {:.4} ms", comparison.std_error);
    println!("   Significant: {}", comparison.is_significant);

    println!("\n3. Computing paired statistics for regression:");
    let regression_samples = vec![
        make_sample(0, 100, 115),
        make_sample(1, 105, 120),
        make_sample(2, 110, 125),
        make_sample(3, 95, 110),
        make_sample(4, 102, 118),
    ];

    let reg_stats = compute_paired_stats(&regression_samples, None)?;
    let reg_comparison = compare_paired_stats(&reg_stats);
    println!("   Mean diff: {:.2} ms", reg_comparison.mean_diff_ms);
    println!("   % change: {:.2}%", reg_comparison.pct_change * 100.0);
    println!("   Significant: {}", reg_comparison.is_significant);

    println!("\n4. Computing paired statistics for no change:");
    let no_change_samples = vec![
        make_sample(0, 100, 100),
        make_sample(1, 105, 105),
        make_sample(2, 110, 110),
        make_sample(3, 95, 95),
        make_sample(4, 102, 102),
    ];

    let nc_stats = compute_paired_stats(&no_change_samples, None)?;
    let nc_comparison = compare_paired_stats(&nc_stats);
    println!("   Mean diff: {:.2} ms", nc_comparison.mean_diff_ms);
    println!("   % change: {:.2}%", nc_comparison.pct_change * 100.0);
    println!("   Significant: {}", nc_comparison.is_significant);

    println!("\n5. Large sample (n >= 30) uses normal approximation:");
    let large_samples: Vec<PairedSample> = (0..50).map(|i| make_sample(i, 100, 95)).collect();

    let large_stats = compute_paired_stats(&large_samples, None)?;
    let large_comparison = compare_paired_stats(&large_stats);
    println!("   Sample count: {}", large_stats.wall_diff_ms.count);
    println!("   Mean diff: {:.2} ms", large_comparison.mean_diff_ms);
    println!(
        "   95% CI: [{:.2}, {:.2}] ms",
        large_comparison.ci_95_lower, large_comparison.ci_95_upper
    );

    println!("\n6. Handling warmup samples (filtered out):");
    let mixed_samples = vec![
        PairedSample {
            pair_index: 0,
            warmup: true,
            baseline: make_half(200),
            current: make_half(200),
            wall_diff_ms: 0,
            rss_diff_kb: None,
        },
        PairedSample {
            pair_index: 1,
            warmup: true,
            baseline: make_half(200),
            current: make_half(200),
            wall_diff_ms: 0,
            rss_diff_kb: None,
        },
        make_sample(2, 100, 90),
        make_sample(3, 105, 95),
    ];

    let mixed_stats = compute_paired_stats(&mixed_samples, None)?;
    println!("   Total samples: {}", mixed_samples.len());
    println!(
        "   Measured samples (after warmup filter): {}",
        mixed_stats.wall_diff_ms.count
    );
    println!("   Mean diff: {:.2} ms", mixed_stats.wall_diff_ms.mean);

    println!("\n7. Computing with work units (throughput):");
    let throughput_samples = vec![
        make_sample(0, 1000, 500),
        make_sample(1, 1000, 500),
        make_sample(2, 1000, 500),
    ];

    let tp_stats = compute_paired_stats(&throughput_samples, Some(100))?;
    if let Some(tp) = tp_stats.baseline_throughput_per_s {
        println!("   Baseline throughput: {:.2} /s", tp.median);
    }
    if let Some(tp) = tp_stats.current_throughput_per_s {
        println!("   Current throughput: {:.2} /s", tp.median);
    }
    if let Some(tp_diff) = tp_stats.throughput_diff_per_s {
        println!("   Throughput improvement: {:.2} /s", tp_diff.mean);
    }

    println!("\n=== Example complete ===");
    Ok(())
}
