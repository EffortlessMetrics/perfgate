//! Budget evaluation logic for performance thresholds.
//!
//! This crate provides pure budget evaluation functions with no I/O dependencies.
//! It handles threshold checking, regression calculation, and verdict aggregation.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Overview
//!
//! The crate provides:
//! - [`evaluate_budget`] - Evaluate a single metric against a budget threshold
//! - [`calculate_regression`] - Calculate regression percentage between baseline and current
//! - [`determine_status`] - Determine metric status (Pass/Warn/Fail) from regression
//! - [`aggregate_verdict`] - Aggregate multiple metric statuses into a final verdict
//!
//! # Example
//!
//! ```
//! use perfgate_budget::{evaluate_budget, calculate_regression, determine_status};
//! use perfgate_types::{Budget, Direction, MetricStatus};
//! use std::collections::BTreeMap;
//!
//! // Create a budget for a lower-is-better metric (e.g., wall time)
//! let budget = Budget {
//!     noise_threshold: None,
//!     noise_policy: perfgate_types::NoisePolicy::Ignore,
//!     threshold: 0.20,       // 20% regression fails
//!     warn_threshold: 0.10,  // 10% regression warns
//!     direction: Direction::Lower,
//! };
//!
//! // Evaluate baseline vs current
//! let baseline = 100.0;
//! let current = 115.0;
//!
//! let result = evaluate_budget(baseline, current, &budget, None).unwrap();
//!
//! assert_eq!(result.status, MetricStatus::Warn);
//! assert!((result.regression - 0.15).abs() < 1e-10);
//! ```

use perfgate_types::{
    Budget, Direction, Metric, MetricStatus, Verdict, VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;
use thiserror::Error;

/// Errors that can occur during budget evaluation.
///
/// # Examples
///
/// ```
/// use perfgate_budget::{evaluate_budget, BudgetError};
/// use perfgate_types::{Budget, Direction};
///
/// let budget = Budget {
///     noise_threshold: None,
///     noise_policy: perfgate_types::NoisePolicy::Ignore,
///     threshold: 0.20,
///     warn_threshold: 0.10,
///     direction: Direction::Lower,
/// };
///
/// // A zero baseline results in InvalidBaseline error
/// let result = evaluate_budget(0.0, 100.0, &budget, None);
/// assert!(matches!(result, Err(BudgetError::InvalidBaseline)));
/// ```
#[derive(Debug, Error)]
pub enum BudgetError {
    #[error("no samples to summarize")]
    NoSamples,

    #[error("baseline value must be > 0")]
    InvalidBaseline,
}

/// Result of evaluating a single metric against a budget.
///
/// # Examples
///
/// ```
/// use perfgate_budget::evaluate_budget;
/// use perfgate_types::{Budget, Direction, MetricStatus};
///
/// let budget = Budget {
///     noise_threshold: None,
///     noise_policy: perfgate_types::NoisePolicy::Ignore,
///     threshold: 0.20,
///     warn_threshold: 0.10,
///     direction: Direction::Lower,
/// };
///
/// let result = evaluate_budget(100.0, 110.0, &budget, None).unwrap();
/// assert_eq!(result.baseline, 100.0);
/// assert_eq!(result.current, 110.0);
/// assert!((result.ratio - 1.10).abs() < 1e-10);
/// assert!((result.pct - 0.10).abs() < 1e-10);
/// assert!((result.regression - 0.10).abs() < 1e-10);
/// assert_eq!(result.status, MetricStatus::Warn);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetResult {
    /// The baseline value.
    pub baseline: f64,
    /// The current value.
    pub current: f64,
    /// Ratio: current / baseline.
    pub ratio: f64,
    /// Percentage change: (current - baseline) / baseline.
    pub pct: f64,
    /// Positive regression amount (0 if improvement).
    pub regression: f64,
    /// Detected noise level (coefficient of variation), if available.
    pub cv: Option<f64>,
    /// Noise threshold used for this comparison.
    pub noise_threshold: Option<f64>,
    /// Determined status based on budget thresholds.
    pub status: MetricStatus,
}

/// Evaluates a metric against a budget threshold.
///
/// This is the core budget evaluation function that:
/// 1. Validates the baseline is positive
/// 2. Calculates ratio, percentage change, and regression
/// 3. Determines the metric status based on budget thresholds and noise
///
/// # Arguments
///
/// * `baseline` - The baseline value (must be > 0)
/// * `current` - The current value to compare
/// * `budget` - The budget configuration with thresholds and direction
/// * `current_cv` - Optional coefficient of variation for the current run
///
/// # Returns
///
/// A `BudgetResult` containing the computed values and status.
///
/// # Errors
///
/// Returns `BudgetError::InvalidBaseline` if baseline is <= 0.
pub fn evaluate_budget(
    baseline: f64,
    current: f64,
    budget: &Budget,
    current_cv: Option<f64>,
) -> Result<BudgetResult, BudgetError> {
    if baseline <= 0.0 {
        return Err(BudgetError::InvalidBaseline);
    }

    let ratio = current / baseline;
    let pct = (current - baseline) / baseline;
    let regression = calculate_regression(baseline, current, budget.direction);

    let mut status = determine_status(regression, budget.threshold, budget.warn_threshold);

    // Noise detection: if CV exceeds noise_threshold, apply noise_policy
    if let (Some(cv), Some(limit)) = (current_cv, budget.noise_threshold)
        && cv > limit
    {
        match budget.noise_policy {
            perfgate_types::NoisePolicy::Ignore => {
                // Even if Ignore, we used to escalate Pass to Warn if noisy?
                // Actually, if Ignore, we should probably do nothing.
                // But maybe "Ignore" means "don't demote failures" but still "warn on noise"?
                // No, let's follow the policy strictly.
            }
            perfgate_types::NoisePolicy::Warn => {
                status = MetricStatus::Warn;
            }
            perfgate_types::NoisePolicy::Skip => {
                status = MetricStatus::Skip;
            }
        }
    }

    Ok(BudgetResult {
        baseline,
        current,
        ratio,
        pct,
        regression,
        cv: current_cv,
        noise_threshold: budget.noise_threshold,
        status,
    })
}

/// Calculates the regression percentage between baseline and current values.
///
/// For `Direction::Lower` (lower is better, e.g., latency):
/// - Regression = max(0, (current - baseline) / baseline)
/// - Positive when current > baseline (slower is worse)
///
/// For `Direction::Higher` (higher is better, e.g., throughput):
/// - Regression = max(0, (baseline - current) / baseline)
/// - Positive when current < baseline (slower is worse)
///
/// # Arguments
///
/// * `baseline` - The baseline value
/// * `current` - The current value
/// * `direction` - Whether lower or higher values are better
///
/// # Returns
///
/// The regression as a positive fraction (0.15 = 15% regression).
/// Returns 0.0 if there's an improvement.
///
/// # Example
///
/// ```
/// use perfgate_budget::calculate_regression;
/// use perfgate_types::Direction;
///
/// // Lower is better: 10% slower = 10% regression
/// let reg = calculate_regression(100.0, 110.0, Direction::Lower);
/// assert!((reg - 0.10).abs() < 1e-10);
///
/// // Lower is better: 10% faster = 0% regression
/// let reg = calculate_regression(100.0, 90.0, Direction::Lower);
/// assert!((reg - 0.0).abs() < 1e-10);
///
/// // Higher is better: 10% slower = 10% regression
/// let reg = calculate_regression(100.0, 90.0, Direction::Higher);
/// assert!((reg - 0.10).abs() < 1e-10);
/// ```
pub fn calculate_regression(baseline: f64, current: f64, direction: Direction) -> f64 {
    let pct = (current - baseline) / baseline;
    match direction {
        Direction::Lower => pct.max(0.0),
        Direction::Higher => (-pct).max(0.0),
    }
}

/// Determines the metric status based on regression and thresholds.
///
/// # Status Rules
///
/// - `Fail`: regression > threshold
/// - `Warn`: warn_threshold <= regression <= threshold
/// - `Pass`: regression < warn_threshold
///
/// # Arguments
///
/// * `regression` - The regression percentage (as fraction, e.g., 0.15 for 15%)
/// * `threshold` - The fail threshold (as fraction)
/// * `warn_threshold` - The warn threshold (as fraction)
///
/// # Example
///
/// ```
/// use perfgate_budget::determine_status;
/// use perfgate_types::MetricStatus;
///
/// let threshold = 0.20;
/// let warn_threshold = 0.10;
///
/// assert_eq!(determine_status(0.25, threshold, warn_threshold), MetricStatus::Fail);
/// assert_eq!(determine_status(0.15, threshold, warn_threshold), MetricStatus::Warn);
/// assert_eq!(determine_status(0.05, threshold, warn_threshold), MetricStatus::Pass);
///
/// // At exact threshold: Warn (not Fail)
/// assert_eq!(determine_status(0.20, threshold, warn_threshold), MetricStatus::Warn);
///
/// // At exact warn threshold: Warn (not Pass)
/// assert_eq!(determine_status(0.10, threshold, warn_threshold), MetricStatus::Warn);
/// ```
pub fn determine_status(regression: f64, threshold: f64, warn_threshold: f64) -> MetricStatus {
    if regression > threshold {
        MetricStatus::Fail
    } else if regression >= warn_threshold {
        MetricStatus::Warn
    } else {
        MetricStatus::Pass
    }
}

/// Calculates a tighter threshold from an observed improvement.
///
/// Returns `None` when the observed result is not an improvement for the metric direction.
/// The returned threshold is always <= `current_threshold`.
pub fn tightened_threshold(
    baseline: f64,
    current: f64,
    current_threshold: f64,
    direction: Direction,
    max_tightening: f64,
) -> Option<f64> {
    if baseline <= 0.0 || current_threshold <= 0.0 {
        return None;
    }

    let raw_improvement = match direction {
        Direction::Lower => (baseline - current) / baseline,
        Direction::Higher => (current - baseline) / baseline,
    };
    if raw_improvement <= 0.0 {
        return None;
    }

    let tightening = raw_improvement.min(max_tightening).max(0.0);
    Some(current_threshold * (1.0 - tightening))
}

/// Aggregates multiple metric statuses into a final verdict.
///
/// # Verdict Rules
///
/// - `Fail`: if any metric has `Fail` status
/// - `Warn`: if no `Fail` but at least one `Warn`
/// - `Pass`: if all metrics are `Pass`
///
/// # Example
///
/// ```
/// use perfgate_budget::aggregate_verdict;
/// use perfgate_types::{MetricStatus, VerdictStatus};
///
/// // Fail dominates
/// let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Fail, MetricStatus::Warn]);
/// assert_eq!(verdict.status, VerdictStatus::Fail);
///
/// // Warn without fail
/// let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Warn, MetricStatus::Pass]);
/// assert_eq!(verdict.status, VerdictStatus::Warn);
///
/// // All pass
/// let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Pass]);
/// assert_eq!(verdict.status, VerdictStatus::Pass);
/// ```
pub fn aggregate_verdict(statuses: &[MetricStatus]) -> Verdict {
    let mut counts = VerdictCounts {
        pass: 0,
        warn: 0,
        fail: 0,
        skip: 0,
    };

    for status in statuses {
        match status {
            MetricStatus::Pass => counts.pass += 1,
            MetricStatus::Warn => counts.warn += 1,
            MetricStatus::Fail => counts.fail += 1,
            MetricStatus::Skip => counts.skip += 1,
        }
    }

    let status = if counts.fail > 0 {
        VerdictStatus::Fail
    } else if counts.warn > 0 {
        VerdictStatus::Warn
    } else if counts.pass > 0 {
        VerdictStatus::Pass
    } else {
        VerdictStatus::Skip
    };

    Verdict {
        status,
        counts,
        reasons: Vec::new(),
    }
}

/// Generates a reason token for a metric status.
///
/// Format: `{metric}_{status}` (e.g., "wall_ms_warn", "max_rss_kb_fail")
///
/// # Examples
///
/// ```
/// use perfgate_budget::reason_token;
/// use perfgate_types::{Metric, MetricStatus};
///
/// assert_eq!(reason_token(Metric::WallMs, MetricStatus::Warn), "wall_ms_warn");
/// assert_eq!(reason_token(Metric::MaxRssKb, MetricStatus::Fail), "max_rss_kb_fail");
/// assert_eq!(reason_token(Metric::ThroughputPerS, MetricStatus::Pass), "throughput_per_s_pass");
/// ```
pub fn reason_token(metric: Metric, status: MetricStatus) -> String {
    format!("{}_{}", metric.as_str(), status.as_str())
}

/// Evaluates multiple metrics against their budgets.
///
/// This function combines individual budget evaluations and aggregates
/// the results into a single verdict with detailed delta information.
///
/// # Arguments
///
/// * `metrics` - Iterator of (metric, baseline, current, current_cv) tuples
/// * `budgets` - Map of metrics to their budget configurations
///
/// # Returns
///
/// A tuple of (deltas map, verdict) where deltas contains per-metric results.
pub fn evaluate_budgets<'a, I>(
    metrics: I,
    budgets: &BTreeMap<Metric, Budget>,
) -> Result<(BTreeMap<Metric, BudgetResult>, Verdict), BudgetError>
where
    I: Iterator<Item = (Metric, f64, f64, Option<f64>)> + 'a,
{
    let mut deltas: BTreeMap<Metric, BudgetResult> = BTreeMap::new();
    let mut statuses: Vec<MetricStatus> = Vec::new();
    let mut reasons: Vec<String> = Vec::new();

    for (metric, baseline, current, cv) in metrics {
        if let Some(budget) = budgets.get(&metric) {
            let result = evaluate_budget(baseline, current, budget, cv)?;

            if result.status != MetricStatus::Pass {
                reasons.push(reason_token(metric, result.status));
            }

            statuses.push(result.status);
            deltas.insert(metric, result);
        }
    }

    let mut verdict = aggregate_verdict(&statuses);
    verdict.reasons = reasons;

    Ok((deltas, verdict))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_budget() -> Budget {
        Budget::new(0.20, 0.10, Direction::Lower)
    }

    #[test]
    fn evaluate_budget_pass() {
        let budget = test_budget();
        let result = evaluate_budget(100.0, 105.0, &budget, None).unwrap();
        assert_eq!(result.status, MetricStatus::Pass);
        assert!((result.regression - 0.05).abs() < 1e-10);
    }

    #[test]
    fn evaluate_budget_warn() {
        let budget = test_budget();
        let result = evaluate_budget(100.0, 115.0, &budget, None).unwrap();
        assert_eq!(result.status, MetricStatus::Warn);
        assert!((result.regression - 0.15).abs() < 1e-10);
    }

    #[test]
    fn evaluate_budget_fail() {
        let budget = test_budget();
        let result = evaluate_budget(100.0, 130.0, &budget, None).unwrap();
        assert_eq!(result.status, MetricStatus::Fail);
        assert!((result.regression - 0.30).abs() < 1e-10);
    }

    #[test]
    fn evaluate_budget_zero_baseline() {
        let budget = test_budget();
        let result = evaluate_budget(0.0, 100.0, &budget, None);
        assert!(matches!(result, Err(BudgetError::InvalidBaseline)));
    }

    #[test]
    fn evaluate_budget_negative_baseline() {
        let budget = test_budget();
        let result = evaluate_budget(-10.0, 100.0, &budget, None);
        assert!(matches!(result, Err(BudgetError::InvalidBaseline)));
    }

    #[test]
    fn calculate_regression_lower_is_better_improvement() {
        let reg = calculate_regression(100.0, 90.0, Direction::Lower);
        assert!((reg - 0.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_regression_lower_is_better_regression() {
        let reg = calculate_regression(100.0, 115.0, Direction::Lower);
        assert!((reg - 0.15).abs() < 1e-10);
    }

    #[test]
    fn calculate_regression_higher_is_better_improvement() {
        let reg = calculate_regression(100.0, 120.0, Direction::Higher);
        assert!((reg - 0.0).abs() < 1e-10);
    }

    #[test]
    fn calculate_regression_higher_is_better_regression() {
        let reg = calculate_regression(100.0, 80.0, Direction::Higher);
        assert!((reg - 0.20).abs() < 1e-10);
    }

    #[test]
    fn determine_status_at_threshold_boundaries() {
        let threshold = 0.20;
        let warn_threshold = 0.10;

        // At exact threshold: Warn (not Fail) because condition is >
        assert_eq!(
            determine_status(0.20, threshold, warn_threshold),
            MetricStatus::Warn
        );

        // Just over threshold: Fail
        assert_eq!(
            determine_status(0.2001, threshold, warn_threshold),
            MetricStatus::Fail
        );

        // At exact warn threshold: Warn (not Pass) because condition is >=
        assert_eq!(
            determine_status(0.10, threshold, warn_threshold),
            MetricStatus::Warn
        );

        // Just under warn threshold: Pass
        assert_eq!(
            determine_status(0.0999, threshold, warn_threshold),
            MetricStatus::Pass
        );
    }

    #[test]
    fn aggregate_verdict_fail_dominates() {
        let verdict =
            aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Fail, MetricStatus::Warn]);
        assert_eq!(verdict.status, VerdictStatus::Fail);
        assert_eq!(verdict.counts.pass, 1);
        assert_eq!(verdict.counts.warn, 1);
        assert_eq!(verdict.counts.fail, 1);
    }

    #[test]
    fn aggregate_verdict_warn_without_fail() {
        let verdict =
            aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Warn, MetricStatus::Pass]);
        assert_eq!(verdict.status, VerdictStatus::Warn);
        assert_eq!(verdict.counts.pass, 2);
        assert_eq!(verdict.counts.warn, 1);
        assert_eq!(verdict.counts.fail, 0);
    }

    #[test]
    fn aggregate_verdict_all_pass() {
        let verdict =
            aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Pass, MetricStatus::Pass]);
        assert_eq!(verdict.status, VerdictStatus::Pass);
        assert_eq!(verdict.counts.pass, 3);
        assert_eq!(verdict.counts.warn, 0);
        assert_eq!(verdict.counts.fail, 0);
    }

    #[test]
    fn reason_token_format() {
        assert_eq!(
            reason_token(Metric::WallMs, MetricStatus::Warn),
            "wall_ms_warn"
        );
        assert_eq!(
            reason_token(Metric::MaxRssKb, MetricStatus::Fail),
            "max_rss_kb_fail"
        );
        assert_eq!(
            reason_token(Metric::ThroughputPerS, MetricStatus::Pass),
            "throughput_per_s_pass"
        );
    }

    #[test]
    fn evaluate_budgets_multiple_metrics() {
        let mut budgets = BTreeMap::new();
        budgets.insert(Metric::WallMs, Budget::new(0.20, 0.10, Direction::Lower));
        budgets.insert(Metric::MaxRssKb, Budget::new(0.30, 0.15, Direction::Lower));

        let metrics = vec![
            (Metric::WallMs, 100.0, 115.0),    // 15% regression -> Warn
            (Metric::MaxRssKb, 1000.0, 900.0), // 0% regression (improvement) -> Pass
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
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn budget_strategy() -> impl Strategy<Value = Budget> {
        (0.01f64..1.0, 0.0f64..=1.0).prop_map(|(threshold, warn_factor)| {
            let warn_threshold = threshold * warn_factor;
            Budget {
                noise_threshold: None,
                noise_policy: perfgate_types::NoisePolicy::Ignore,
                threshold,
                warn_threshold,
                direction: Direction::Lower,
            }
        })
    }

    proptest! {
        #[test]
        fn prop_regression_is_non_negative(
            baseline in 1.0f64..10000.0,
            current in 0.1f64..20000.0,
            direction in prop_oneof![Just(Direction::Lower), Just(Direction::Higher)],
        ) {
            let regression = calculate_regression(baseline, current, direction);
            prop_assert!(regression >= 0.0, "regression should be non-negative");
        }

        #[test]
        fn prop_evaluate_budget_consistency(
            baseline in 1.0f64..10000.0,
            current in 0.1f64..20000.0,
            budget in budget_strategy(),
        ) {
            let result = evaluate_budget(baseline, current, &budget, None).unwrap();

            // Check ratio calculation
            let expected_ratio = current / baseline;
            prop_assert!((result.ratio - expected_ratio).abs() < 1e-10);

            // Check pct calculation
            let expected_pct = (current - baseline) / baseline;
            prop_assert!((result.pct - expected_pct).abs() < 1e-10);

            // Check regression calculation
            let expected_regression = calculate_regression(baseline, current, budget.direction);
            prop_assert!((result.regression - expected_regression).abs() < 1e-10);

            // Check status consistency
            let expected_status = determine_status(result.regression, budget.threshold, budget.warn_threshold);
            prop_assert_eq!(result.status, expected_status);
        }

        #[test]
        fn prop_determine_status_ordering(
            regression in 0.0f64..2.0,
            threshold in 0.01f64..1.0,
            warn_factor in 0.0f64..=1.0,
        ) {
            let warn_threshold = threshold * warn_factor;
            let status = determine_status(regression, threshold, warn_threshold);

            // Check status boundaries
            match status {
                MetricStatus::Fail => prop_assert!(regression > threshold),
                MetricStatus::Warn => {
                    prop_assert!(regression >= warn_threshold);
                    prop_assert!(regression <= threshold);
                }
                MetricStatus::Pass => prop_assert!(regression < warn_threshold),
                MetricStatus::Skip => {
                    // Skip only happens if CV > limit and noise_policy is Skip.
                    // determine_status doesn't return Skip, so if we have Skip here,
                    // it means the noise detection logic applied it.
                }
            }
        }

        #[test]
        fn prop_aggregate_verdict_consistency(statuses in prop::collection::vec(
            prop_oneof![
                Just(MetricStatus::Pass),
                Just(MetricStatus::Warn),
                Just(MetricStatus::Fail),
                Just(MetricStatus::Skip)
            ],
            0..20
        )) {
            let verdict = aggregate_verdict(&statuses);

            // Check counts
            let expected_pass = statuses.iter().filter(|&&s| s == MetricStatus::Pass).count() as u32;
            let expected_warn = statuses.iter().filter(|&&s| s == MetricStatus::Warn).count() as u32;
            let expected_fail = statuses.iter().filter(|&&s| s == MetricStatus::Fail).count() as u32;
            let expected_skip = statuses.iter().filter(|&&s| s == MetricStatus::Skip).count() as u32;

            prop_assert_eq!(verdict.counts.pass, expected_pass);
            prop_assert_eq!(verdict.counts.warn, expected_warn);
            prop_assert_eq!(verdict.counts.fail, expected_fail);
            prop_assert_eq!(verdict.counts.skip, expected_skip);

            // Check status aggregation
            if expected_fail > 0 {
                prop_assert_eq!(verdict.status, VerdictStatus::Fail);
            } else if expected_warn > 0 {
                prop_assert_eq!(verdict.status, VerdictStatus::Warn);
            } else if expected_pass > 0 {
                prop_assert_eq!(verdict.status, VerdictStatus::Pass);
            } else {
                prop_assert_eq!(verdict.status, VerdictStatus::Skip);
            }
        }

        #[test]
        fn prop_evaluate_budget_deterministic(
            baseline in 1.0f64..10000.0,
            current in 0.1f64..20000.0,
            budget in budget_strategy(),
        ) {
            let r1 = evaluate_budget(baseline, current, &budget, None).unwrap();
            let r2 = evaluate_budget(baseline, current, &budget, None).unwrap();
            prop_assert_eq!(r1, r2, "evaluate_budget must be deterministic");
        }

        #[test]
        fn prop_zero_regression_is_pass(
            threshold in 0.01f64..1.0,
            warn_factor in 0.01f64..=1.0,
        ) {
            let warn_threshold = threshold * warn_factor;
            let status = determine_status(0.0, threshold, warn_threshold);
            prop_assert_eq!(status, MetricStatus::Pass, "zero regression should always be Pass");
        }

        #[test]
        fn prop_negative_regression_clamped(
            baseline in 1.0f64..10000.0,
            improvement_factor in 0.01f64..1.0,
            direction in prop_oneof![Just(Direction::Lower), Just(Direction::Higher)],
        ) {
            // When current is an improvement, regression should be 0
            let current = match direction {
                Direction::Lower => baseline * (1.0 - improvement_factor),   // lower = better
                Direction::Higher => baseline * (1.0 + improvement_factor),  // higher = better
            };
            let regression = calculate_regression(baseline, current, direction);
            prop_assert_eq!(regression, 0.0, "improvements should yield zero regression");
        }
    }
}
