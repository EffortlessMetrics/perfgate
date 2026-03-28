//! Integration tests: budget evaluation with significance testing.
//!
//! These tests verify that budget evaluation integrates correctly
//! with significance testing and paired statistics.

use perfgate_budget::{
    BudgetError, aggregate_verdict, calculate_regression, determine_status, evaluate_budget,
    evaluate_budgets, reason_token,
};
use perfgate_domain::{
    SignificancePolicy, compare_paired_stats, compare_runs, compute_paired_stats,
};
use perfgate_paired::PairedError;
use perfgate_types::{
    BenchMeta, Budget, Direction, HostInfo, Metric, MetricStatus, PairedSampleHalf, RUN_SCHEMA_V1,
    RunMeta, RunReceipt, Sample, ToolInfo, VerdictStatus,
};
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

fn make_run_receipt(samples: Vec<u64>) -> RunReceipt {
    let sample_vec: Vec<Sample> = samples.iter().map(|&ms| make_sample(ms)).collect();
    let stats = perfgate_domain::compute_stats(&sample_vec, None).unwrap();

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
            runner: None,
        },
        bench: BenchMeta {
            name: "test-bench".to_string(),
            cwd: None,
            command: vec!["echo".to_string()],
            repeat: samples.len() as u32,
            warmup: 0,
            work_units: None,
            timeout_ms: None,
        },
        samples: sample_vec,
        stats,
    }
}

#[test]
fn budget_evaluate_basic_pass() {
    let budget = Budget::new(0.20, 0.10, Direction::Lower);

    let result = evaluate_budget(100.0, 105.0, &budget, None).unwrap();
    assert_eq!(result.status, MetricStatus::Pass);
    assert!((result.regression - 0.05).abs() < 1e-10);
}

#[test]
fn budget_evaluate_basic_warn() {
    let budget = Budget::new(0.20, 0.10, Direction::Lower);

    let result = evaluate_budget(100.0, 115.0, &budget, None).unwrap();
    assert_eq!(result.status, MetricStatus::Warn);
    assert!((result.regression - 0.15).abs() < 1e-10);
}

#[test]
fn budget_evaluate_basic_fail() {
    let budget = Budget::new(0.20, 0.10, Direction::Lower);

    let result = evaluate_budget(100.0, 130.0, &budget, None).unwrap();
    assert_eq!(result.status, MetricStatus::Fail);
    assert!((result.regression - 0.30).abs() < 1e-10);
}

#[test]
fn budget_regression_lower_is_better() {
    let reg = calculate_regression(100.0, 110.0, Direction::Lower);
    assert!((reg - 0.10).abs() < 1e-10);

    let reg = calculate_regression(100.0, 90.0, Direction::Lower);
    assert!((reg - 0.0).abs() < 1e-10);
}

#[test]
fn budget_regression_higher_is_better() {
    let reg = calculate_regression(100.0, 90.0, Direction::Higher);
    assert!((reg - 0.10).abs() < 1e-10);

    let reg = calculate_regression(100.0, 110.0, Direction::Higher);
    assert!((reg - 0.0).abs() < 1e-10);
}

#[test]
fn budget_invalid_baseline() {
    let budget = Budget::new(0.20, 0.10, Direction::Lower);

    let result = evaluate_budget(0.0, 100.0, &budget, None);
    assert!(matches!(result, Err(BudgetError::InvalidBaseline)));

    let result = evaluate_budget(-10.0, 100.0, &budget, None);
    assert!(matches!(result, Err(BudgetError::InvalidBaseline)));
}

#[test]
fn budget_aggregate_verdict_fail_dominates() {
    let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Fail, MetricStatus::Warn]);
    assert_eq!(verdict.status, VerdictStatus::Fail);
}

#[test]
fn budget_aggregate_verdict_warn_without_fail() {
    let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Warn, MetricStatus::Pass]);
    assert_eq!(verdict.status, VerdictStatus::Warn);
}

#[test]
fn budget_aggregate_verdict_all_pass() {
    let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Pass, MetricStatus::Pass]);
    assert_eq!(verdict.status, VerdictStatus::Pass);
}

#[test]
fn significance_testing_with_sufficient_samples() {
    let baseline = make_run_receipt(vec![100, 102, 98, 101, 99, 100, 102, 98, 101, 99]);
    let current = make_run_receipt(vec![110, 112, 108, 111, 109, 110, 112, 108, 111, 109]);

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.05, 0.03, Direction::Lower));

    let comparison = compare_runs(
        &baseline,
        &current,
        &budgets,
        &BTreeMap::new(),
        Some(SignificancePolicy {
            alpha: 0.05,
            min_samples: 8,
            require_significance: true,
        }),
    )
    .unwrap();

    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert!(delta.significance.is_some());
    let sig = delta.significance.as_ref().unwrap();
    assert!(sig.significant);
}

#[test]
fn significance_testing_without_significance_policy() {
    let baseline = make_run_receipt(vec![100, 100, 100]);
    let current = make_run_receipt(vec![110, 110, 110]);

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.05, 0.03, Direction::Lower));

    let comparison = compare_runs(&baseline, &current, &budgets, &BTreeMap::new(), None).unwrap();

    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert!(delta.significance.is_none());
}

#[test]
fn significance_require_significance_passes_non_significant() {
    let baseline = make_run_receipt(vec![100, 102, 98, 101, 99, 100, 102, 98, 101, 99]);
    let current = make_run_receipt(vec![101, 103, 99, 102, 100, 101, 103, 99, 102, 100]);

    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.05, 0.03, Direction::Lower));

    let comparison = compare_runs(
        &baseline,
        &current,
        &budgets,
        &BTreeMap::new(),
        Some(SignificancePolicy {
            alpha: 0.01,
            min_samples: 8,
            require_significance: true,
        }),
    )
    .unwrap();

    let delta = comparison.deltas.get(&Metric::WallMs).unwrap();
    assert_eq!(delta.status, MetricStatus::Pass);
    assert!(delta.significance.is_some());
    let sig = delta.significance.as_ref().unwrap();
    assert!(!sig.significant);
}

#[test]
fn paired_stats_basic() {
    use perfgate_types::PairedSample;

    fn make_paired_half(wall_ms: u64) -> PairedSampleHalf {
        PairedSampleHalf {
            wall_ms,
            exit_code: 0,
            timed_out: false,
            max_rss_kb: None,
            stdout: None,
            stderr: None,
        }
    }

    let samples = vec![
        PairedSample {
            pair_index: 0,
            warmup: false,
            baseline: make_paired_half(100),
            current: make_paired_half(90),
            wall_diff_ms: -10,
            rss_diff_kb: None,
        },
        PairedSample {
            pair_index: 1,
            warmup: false,
            baseline: make_paired_half(100),
            current: make_paired_half(95),
            wall_diff_ms: -5,
            rss_diff_kb: None,
        },
        PairedSample {
            pair_index: 2,
            warmup: false,
            baseline: make_paired_half(100),
            current: make_paired_half(85),
            wall_diff_ms: -15,
            rss_diff_kb: None,
        },
    ];

    let stats = compute_paired_stats(&samples, None, None).unwrap();
    assert_eq!(stats.wall_diff_ms.mean, -10.0);
    assert_eq!(stats.wall_diff_ms.count, 3);

    let comparison = compare_paired_stats(&stats);
    assert!((comparison.mean_diff_ms - (-10.0)).abs() < 1e-10);
    assert!(comparison.is_significant);
}

#[test]
fn paired_stats_empty_samples() {
    use perfgate_types::PairedSample;
    let samples: Vec<PairedSample> = vec![];

    let result = compute_paired_stats(&samples, None, None);
    assert!(matches!(result, Err(PairedError::NoSamples)));
}

#[test]
fn budget_reason_token_format() {
    assert_eq!(
        reason_token(Metric::WallMs, MetricStatus::Warn),
        "wall_ms_warn"
    );
    assert_eq!(
        reason_token(Metric::MaxRssKb, MetricStatus::Fail),
        "max_rss_kb_fail"
    );
}

#[test]
fn budget_evaluate_multiple_metrics() {
    let mut budgets = BTreeMap::new();
    budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));
    budgets.insert(Metric::MaxRssKb, Budget::new(0.30, 0.15, Direction::Lower));

    let metrics = vec![
        (Metric::WallMs, 100.0, 115.0),
        (Metric::MaxRssKb, 1000.0, 900.0),
    ];

    let (deltas, verdict) = evaluate_budgets(
        metrics.into_iter().map(|(m, b, c)| (m, b, c, None)),
        &budgets,
    )
    .unwrap();

    assert_eq!(deltas.len(), 2);
    assert_eq!(verdict.status, VerdictStatus::Warn);
    assert_eq!(verdict.counts.warn, 1);
    assert_eq!(verdict.counts.pass, 1);
}

#[test]
fn determine_status_boundary_conditions() {
    let threshold = 0.20;
    let warn_threshold = 0.10;

    assert_eq!(
        determine_status(0.20, threshold, warn_threshold),
        MetricStatus::Warn
    );
    assert_eq!(
        determine_status(0.2001, threshold, warn_threshold),
        MetricStatus::Fail
    );
    assert_eq!(
        determine_status(0.10, threshold, warn_threshold),
        MetricStatus::Warn
    );
    assert_eq!(
        determine_status(0.0999, threshold, warn_threshold),
        MetricStatus::Pass
    );
}
