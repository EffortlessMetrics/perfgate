//! Integration tests: stats crate → domain crate.
//!
//! These tests verify that the perfgate-stats crate integrates correctly
//! with perfgate-domain, and that errors propagate correctly between them.

use perfgate_stats::{StatsError, summarize_f64, summarize_u64};
use perfgate_types::{F64Summary, U64Summary};

#[test]
fn stats_summarize_u64_produces_types_compatible_summary() {
    let values = vec![10, 20, 30, 40, 50];
    let summary: U64Summary = summarize_u64(&values).unwrap();

    assert_eq!(summary.median, 30);
    assert_eq!(summary.min, 10);
    assert_eq!(summary.max, 50);
}

#[test]
fn stats_summarize_f64_produces_types_compatible_summary() {
    let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    let summary: F64Summary = summarize_f64(&values).unwrap();

    assert!((summary.median - 30.0).abs() < f64::EPSILON);
    assert!((summary.min - 10.0).abs() < f64::EPSILON);
    assert!((summary.max - 50.0).abs() < f64::EPSILON);
}

#[test]
fn stats_error_propagates_as_domain_error() {
    let result = summarize_u64(&[]);
    assert!(matches!(result, Err(StatsError::NoSamples)));
}

#[test]
fn stats_empty_f64_returns_no_samples_error() {
    let result = summarize_f64(&[]);
    assert!(matches!(result, Err(StatsError::NoSamples)));
}

#[test]
fn stats_u64_summary_is_used_by_domain_stats() {
    use perfgate_domain::compute_stats;
    use perfgate_types::Sample;

    let samples = vec![Sample {
        wall_ms: 100,
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
    }];

    let stats = compute_stats(&samples, None).unwrap();

    assert_eq!(stats.wall_ms.median, 100);
    assert_eq!(stats.wall_ms.min, 100);
    assert_eq!(stats.wall_ms.max, 100);
}

#[test]
fn stats_error_propagates_through_domain() {
    use perfgate_domain::{DomainError, compute_stats};
    use perfgate_types::Sample;

    let samples: Vec<Sample> = vec![];

    let result = compute_stats(&samples, None);
    assert!(matches!(result, Err(DomainError::NoSamples)));
}

#[test]
fn stats_large_values_handled_correctly() {
    let values = vec![u64::MAX, u64::MAX - 1, u64::MAX - 2];
    let summary = summarize_u64(&values).unwrap();

    assert_eq!(summary.max, u64::MAX);
    assert_eq!(summary.min, u64::MAX - 2);
}

#[test]
fn stats_single_value_summary() {
    let values = vec![42u64];
    let summary = summarize_u64(&values).unwrap();

    assert_eq!(summary.median, 42);
    assert_eq!(summary.min, 42);
    assert_eq!(summary.max, 42);
}

#[test]
fn stats_two_value_median_averages() {
    let values = vec![10u64, 20];
    let summary = summarize_u64(&values).unwrap();

    assert_eq!(summary.median, 15);
    assert_eq!(summary.min, 10);
    assert_eq!(summary.max, 20);
}

#[test]
fn domain_uses_stats_for_metric_values() {
    use perfgate_domain::compare_stats;
    use perfgate_types::{Budget, Direction, Metric, Stats, U64Summary};
    use std::collections::BTreeMap;

    let baseline = Stats {
        wall_ms: U64Summary::new(100, 90, 110),
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
    };

    let current = Stats {
        wall_ms: U64Summary::new(120, 110, 130),
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
    };

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.15, 0.10, Direction::Lower));

    let comparison = compare_stats(&baseline, &current, &budgets).unwrap();

    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert!((delta.baseline - 100.0).abs() < f64::EPSILON);
    assert!((delta.current - 120.0).abs() < f64::EPSILON);
    assert!((delta.regression - 0.20).abs() < f64::EPSILON);
}

#[test]
fn stats_percentile_works_with_domain_percentile_logic() {
    use perfgate_stats::percentile;

    let values = vec![10.0, 20.0, 30.0, 40.0, 50.0];

    let p50 = percentile(values.clone(), 0.5).unwrap();
    assert!((p50 - 30.0).abs() < f64::EPSILON);

    let p95 = percentile(values.clone(), 0.95).unwrap();
    assert!(p95 > 40.0 && p95 <= 50.0);

    let p0 = percentile(values.clone(), 0.0).unwrap();
    assert!((p0 - 10.0).abs() < f64::EPSILON);

    let p100 = percentile(values, 1.0).unwrap();
    assert!((p100 - 50.0).abs() < f64::EPSILON);
}

#[test]
fn stats_mean_and_variance_integrate_with_significance_testing() {
    use perfgate_stats::mean_and_variance;

    let values = vec![100.0, 102.0, 98.0, 101.0, 99.0];
    let (mean, variance) = mean_and_variance(&values).unwrap();

    assert!((mean - 100.0).abs() < 1.0);
    assert!(variance >= 0.0);
    assert!(variance < 10.0);
}
