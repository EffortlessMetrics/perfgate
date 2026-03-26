//! Demonstrates summarize_u64, compute_stats, and compare_stats.

use perfgate_domain::{compare_stats, compute_stats, summarize_u64};
use perfgate_types::{Budget, Direction, Metric, Sample};
use std::collections::BTreeMap;

fn make_sample(wall_ms: u64) -> Sample {
    Sample {
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
    }
}

fn main() {
    // 1. summarize_u64: compute median/min/max from raw values
    let values = vec![120, 115, 118, 122, 117];
    let summary = summarize_u64(&values).expect("summarize");
    println!(
        "summarize_u64: median={}, min={}, max={}",
        summary.median, summary.min, summary.max
    );

    // 2. compute_stats: aggregate samples into Stats
    let baseline_samples: Vec<Sample> = vec![100, 102, 98, 101, 99]
        .into_iter()
        .map(make_sample)
        .collect();
    let current_samples: Vec<Sample> = vec![115, 118, 112, 116, 114]
        .into_iter()
        .map(make_sample)
        .collect();

    let baseline_stats = compute_stats(&baseline_samples, None).expect("baseline stats");
    let current_stats = compute_stats(&current_samples, None).expect("current stats");

    println!(
        "\nBaseline wall_ms: median={}, min={}, max={}",
        baseline_stats.wall_ms.median, baseline_stats.wall_ms.min, baseline_stats.wall_ms.max
    );
    println!(
        "Current  wall_ms: median={}, min={}, max={}",
        current_stats.wall_ms.median, current_stats.wall_ms.min, current_stats.wall_ms.max
    );

    // 3. compare_stats: evaluate against budgets
    let mut budgets = BTreeMap::new();
    budgets.insert(
        Metric::WallMs,
        Budget {
            noise_threshold: None,
            noise_policy: perfgate_types::NoisePolicy::Ignore,
            threshold: 0.20,      // 20% regression = fail
            warn_threshold: 0.10, // 10% regression = warn
            direction: Direction::Lower,
        },
    );

    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets).expect("compare");
    println!("\nVerdict: {:?}", comparison.verdict.status);

    if let Some(delta) = comparison.deltas.get(&Metric::WallMs) {
        println!(
            "wall_ms delta: baseline={:.0}, current={:.0}, regression={:.1}%, status={:?}",
            delta.baseline,
            delta.current,
            delta.regression * 100.0,
            delta.status,
        );
    }
}
