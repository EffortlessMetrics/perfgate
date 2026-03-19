//! Statistical functions for benchmarking analysis.
//!
//! This crate provides pure statistical functions with no I/O dependencies.
//! It is designed to be used by `perfgate-domain` and can be independently
//! tested and versioned.
//!
//! # Overview
//!
//! The crate provides:
//! - Summary statistics (median, min, max) for `u64` and `f64` slices
//! - Percentile calculation
//! - Mean and variance computation

pub use perfgate_error::StatsError;

use perfgate_types::{F64Summary, U64Summary};
use std::cmp::Ordering;

/// Compute min, max, and median for a `u64` slice.
///
/// # Errors
///
/// Returns [`StatsError::NoSamples`] if the slice is empty.
///
/// # Examples
///
/// ```
/// use perfgate_stats::summarize_u64;
///
/// let s = summarize_u64(&[10, 30, 20]).unwrap();
/// assert_eq!(s.median, 20);
/// assert_eq!(s.min, 10);
/// assert_eq!(s.max, 30);
/// ```
pub fn summarize_u64(values: &[u64]) -> Result<U64Summary, StatsError> {
    if values.is_empty() {
        return Err(StatsError::NoSamples);
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let min = *v.first().unwrap();
    let max = *v.last().unwrap();
    let median = median_u64_sorted(&v);

    let f64_vals: Vec<f64> = values.iter().map(|&x| x as f64).collect();
    let (mean, stddev) = if let Some((m, var)) = mean_and_variance(&f64_vals) {
        (Some(m), Some(var.sqrt()))
    } else {
        (None, None)
    };

    Ok(U64Summary {
        median,
        min,
        max,
        mean,
        stddev,
    })
}

/// Compute min, max, and median for an `f64` slice.
///
/// # Errors
///
/// Returns [`StatsError::NoSamples`] if the slice is empty.
///
/// # Examples
///
/// ```
/// use perfgate_stats::summarize_f64;
///
/// let s = summarize_f64(&[1.0, 3.0, 2.0]).unwrap();
/// assert_eq!(s.median, 2.0);
/// assert_eq!(s.min, 1.0);
/// assert_eq!(s.max, 3.0);
/// ```
pub fn summarize_f64(values: &[f64]) -> Result<F64Summary, StatsError> {
    if values.is_empty() {
        return Err(StatsError::NoSamples);
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let min = *v.first().unwrap();
    let max = *v.last().unwrap();
    let median = median_f64_sorted(&v);

    let (mean, stddev) = if let Some((m, var)) = mean_and_variance(values) {
        (Some(m), Some(var.sqrt()))
    } else {
        (None, None)
    };

    Ok(F64Summary {
        median,
        min,
        max,
        mean,
        stddev,
    })
}

pub fn median_u64_sorted(sorted: &[u64]) -> u64 {
    debug_assert!(!sorted.is_empty());
    let n = sorted.len();
    let mid = n / 2;
    if n % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] / 2) + (sorted[mid] / 2) + ((sorted[mid - 1] % 2 + sorted[mid] % 2) / 2)
    }
}

pub fn median_f64_sorted(sorted: &[f64]) -> f64 {
    debug_assert!(!sorted.is_empty());
    let n = sorted.len();
    let mid = n / 2;
    if n % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

/// Compute the `q`-th percentile (0.0–1.0) using linear interpolation.
///
/// Returns `None` if `values` is empty.
///
/// # Examples
///
/// ```
/// use perfgate_stats::percentile;
///
/// let p50 = percentile(vec![1.0, 2.0, 3.0, 4.0, 5.0], 0.5).unwrap();
/// assert_eq!(p50, 3.0);
///
/// let p0 = percentile(vec![10.0, 20.0, 30.0], 0.0).unwrap();
/// assert_eq!(p0, 10.0);
/// ```
pub fn percentile(mut values: Vec<f64>, q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    if values.len() == 1 {
        return Some(values[0]);
    }

    let rank = q.clamp(0.0, 1.0) * (values.len() as f64 - 1.0);
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;

    if lower == upper {
        return Some(values[lower]);
    }

    let weight = rank - lower as f64;
    Some(values[lower] + (values[upper] - values[lower]) * weight)
}

/// Compute sample mean and unbiased variance (Welford's algorithm).
///
/// Returns `None` if `values` is empty or the result is non-finite.
/// Variance uses Bessel's correction (n−1 denominator).
///
/// # Examples
///
/// ```
/// use perfgate_stats::mean_and_variance;
///
/// let (mean, var) = mean_and_variance(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
/// assert!((mean - 3.0).abs() < 1e-10);
/// assert!((var - 2.5).abs() < 1e-10);
///
/// // Single element: variance is 0
/// let (mean, var) = mean_and_variance(&[42.0]).unwrap();
/// assert_eq!(mean, 42.0);
/// assert_eq!(var, 0.0);
/// ```
pub fn mean_and_variance(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }

    // Welford's online one-pass algorithm for numerical stability
    let mut n: u64 = 0;
    let mut mean = 0.0_f64;
    let mut m2 = 0.0_f64;

    for &x in values {
        n += 1;
        let delta = x - mean;
        mean += delta / n as f64;
        let delta2 = x - mean;
        m2 += delta * delta2;
    }

    let var = if n > 1 { m2 / (n as f64 - 1.0) } else { 0.0 };

    if mean.is_finite() && var.is_finite() {
        Some((mean, var.max(0.0)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarize_u64_empty_returns_error() {
        let result = summarize_u64(&[]);
        assert!(matches!(result, Err(StatsError::NoSamples)));
    }

    #[test]
    fn summarize_f64_empty_returns_error() {
        let result = summarize_f64(&[]);
        assert!(matches!(result, Err(StatsError::NoSamples)));
    }

    #[test]
    fn summarize_u64_single_element() {
        let summary = summarize_u64(&[42]).unwrap();
        assert_eq!(summary.median, 42);
        assert_eq!(summary.min, 42);
        assert_eq!(summary.max, 42);
    }

    #[test]
    fn summarize_f64_single_element() {
        let summary = summarize_f64(&[42.0]).unwrap();
        assert_eq!(summary.median, 42.0);
        assert_eq!(summary.min, 42.0);
        assert_eq!(summary.max, 42.0);
    }

    #[test]
    fn summarize_u64_two_elements() {
        let summary = summarize_u64(&[10, 20]).unwrap();
        assert_eq!(summary.median, 15);
        assert_eq!(summary.min, 10);
        assert_eq!(summary.max, 20);
    }

    #[test]
    fn summarize_f64_two_elements() {
        let summary = summarize_f64(&[10.0, 20.0]).unwrap();
        assert_eq!(summary.median, 15.0);
        assert_eq!(summary.min, 10.0);
        assert_eq!(summary.max, 20.0);
    }

    #[test]
    fn summarize_u64_odd_length() {
        let summary = summarize_u64(&[10, 30, 20]).unwrap();
        assert_eq!(summary.median, 20);
        assert_eq!(summary.min, 10);
        assert_eq!(summary.max, 30);
    }

    #[test]
    fn summarize_f64_odd_length() {
        let summary = summarize_f64(&[10.0, 30.0, 20.0]).unwrap();
        assert_eq!(summary.median, 20.0);
        assert_eq!(summary.min, 10.0);
        assert_eq!(summary.max, 30.0);
    }

    #[test]
    fn summarize_u64_even_length_median_rounds_down() {
        let summary = summarize_u64(&[10, 20, 30, 40]).unwrap();
        assert_eq!(summary.median, 25);
    }

    #[test]
    fn summarize_u64_large_values_no_overflow() {
        let values = [u64::MAX, u64::MAX - 1];
        let summary = summarize_u64(&values).unwrap();
        assert_eq!(summary.min, u64::MAX - 1);
        assert_eq!(summary.max, u64::MAX);
        assert_eq!(summary.median, u64::MAX - 1);
    }

    #[test]
    fn percentile_empty_returns_none() {
        assert!(percentile(vec![], 0.5).is_none());
    }

    #[test]
    fn percentile_single_element() {
        let p = percentile(vec![42.0], 0.5).unwrap();
        assert_eq!(p, 42.0);
    }

    #[test]
    fn percentile_zero_is_min() {
        let p = percentile(vec![1.0, 2.0, 3.0, 4.0, 5.0], 0.0).unwrap();
        assert_eq!(p, 1.0);
    }

    #[test]
    fn percentile_one_is_max() {
        let p = percentile(vec![1.0, 2.0, 3.0, 4.0, 5.0], 1.0).unwrap();
        assert_eq!(p, 5.0);
    }

    #[test]
    fn percentile_half_is_median_odd() {
        let p = percentile(vec![1.0, 2.0, 3.0, 4.0, 5.0], 0.5).unwrap();
        assert_eq!(p, 3.0);
    }

    #[test]
    fn mean_and_variance_empty_returns_none() {
        assert!(mean_and_variance(&[]).is_none());
    }

    #[test]
    fn mean_and_variance_single_element() {
        let (mean, var) = mean_and_variance(&[42.0]).unwrap();
        assert_eq!(mean, 42.0);
        assert_eq!(var, 0.0);
    }

    #[test]
    fn mean_and_variance_basic() {
        let (mean, var) = mean_and_variance(&[1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
        assert!((mean - 3.0).abs() < 1e-10);
        assert!((var - 2.5).abs() < 1e-10);
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn expected_median_u64(sorted: &[u64]) -> u64 {
        let n = sorted.len();
        let mid = n / 2;
        if n % 2 == 1 {
            sorted[mid]
        } else {
            let a = sorted[mid - 1] as u128;
            let b = sorted[mid] as u128;
            ((a + b) / 2) as u64
        }
    }

    fn finite_f64_strategy() -> impl Strategy<Value = f64> {
        -1e100f64..1e100f64
    }

    fn large_u64_strategy() -> impl Strategy<Value = u64> {
        let min_val = u64::MAX - (u64::MAX / 10);
        min_val..=u64::MAX
    }

    proptest! {
        #[test]
        fn prop_summarize_u64_ordering(values in prop::collection::vec(any::<u64>(), 1..100)) {
            let summary = summarize_u64(&values).expect("non-empty vec should succeed");
            prop_assert!(summary.min <= summary.median);
            prop_assert!(summary.median <= summary.max);
        }

        #[test]
        fn prop_summarize_u64_correctness(values in prop::collection::vec(any::<u64>(), 1..100)) {
            let summary = summarize_u64(&values).expect("non-empty vec should succeed");
            let mut sorted = values.clone();
            sorted.sort_unstable();
            prop_assert_eq!(summary.min, *sorted.first().unwrap());
            prop_assert_eq!(summary.max, *sorted.last().unwrap());
            prop_assert_eq!(summary.median, expected_median_u64(&sorted));
        }

        #[test]
        fn prop_summarize_u64_single_element(value: u64) {
            let summary = summarize_u64(&[value]).unwrap();
            prop_assert_eq!(summary.min, value);
            prop_assert_eq!(summary.max, value);
            prop_assert_eq!(summary.median, value);
        }

        #[test]
        fn prop_summarize_f64_ordering(values in prop::collection::vec(finite_f64_strategy(), 1..100)) {
            let summary = summarize_f64(&values).expect("non-empty vec should succeed");
            prop_assert!(summary.min <= summary.median);
            prop_assert!(summary.median <= summary.max);
        }

        #[test]
        fn prop_median_u64_overflow_handling(values in prop::collection::vec(large_u64_strategy(), 2..50)) {
            let summary = summarize_u64(&values).expect("non-empty vec should succeed");
            let mut sorted = values.clone();
            sorted.sort_unstable();
            let expected = expected_median_u64(&sorted);
            prop_assert_eq!(summary.median, expected);
        }

        #[test]
        fn prop_percentile_bounds(values in prop::collection::vec(finite_f64_strategy(), 1..50)) {
            let min_val = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let p0 = percentile(values.clone(), 0.0).unwrap();
            let p100 = percentile(values.clone(), 1.0).unwrap();
            let p50 = percentile(values.clone(), 0.5).unwrap();
            prop_assert!((p0 - min_val).abs() < f64::EPSILON);
            prop_assert!((p100 - max_val).abs() < f64::EPSILON);
            prop_assert!(p50 >= min_val && p50 <= max_val);
        }

        #[test]
        fn prop_mean_and_variance_correctness(values in prop::collection::vec(finite_f64_strategy(), 1..50)) {
            let result = mean_and_variance(&values);
            prop_assert!(result.is_some());
            let (mean, var) = result.unwrap();
            let expected_mean: f64 = values.iter().sum::<f64>() / values.len() as f64;
            let mean_tol = expected_mean.abs().max(1.0) * 1e-9;
            prop_assert!((mean - expected_mean).abs() < mean_tol,
                "mean diff {} exceeds tolerance {}", (mean - expected_mean).abs(), mean_tol);
            if values.len() > 1 {
                let expected_var: f64 = values.iter()
                    .map(|v| (v - expected_mean).powi(2))
                    .sum::<f64>() / (values.len() - 1) as f64;
                let var_tol = expected_var.abs().max(1.0) * 1e-6;
                prop_assert!((var - expected_var).abs() < var_tol,
                    "var diff {} exceeds tolerance {}", (var - expected_var).abs(), var_tol);
            } else {
                prop_assert_eq!(var, 0.0);
            }
        }

        #[test]
        fn prop_mean_and_variance_finite(values in prop::collection::vec(finite_f64_strategy(), 1..50)) {
            let (mean, var) = mean_and_variance(&values).unwrap();
            prop_assert!(mean.is_finite());
            prop_assert!(var.is_finite());
            prop_assert!(var >= 0.0);
        }

        #[test]
        fn prop_p95_gte_median(values in prop::collection::vec(finite_f64_strategy(), 2..100)) {
            let p50 = percentile(values.clone(), 0.5).unwrap();
            let p95 = percentile(values, 0.95).unwrap();
            prop_assert!(p95 >= p50, "p95 ({}) should be >= median ({})", p95, p50);
        }

        #[test]
        fn prop_mean_equals_sum_over_count(values in prop::collection::vec(finite_f64_strategy(), 1..50)) {
            if let Some((mean, _)) = mean_and_variance(&values) {
                let expected = values.iter().sum::<f64>() / values.len() as f64;
                if expected.is_finite() {
                    let tol = expected.abs().max(1.0) * 1e-10;
                    prop_assert!((mean - expected).abs() < tol,
                        "mean={}, expected={}, diff={}", mean, expected, (mean - expected).abs());
                }
            }
        }

        #[test]
        fn prop_summarize_u64_preserves_input(values in prop::collection::vec(any::<u64>(), 1..100)) {
            let summary = summarize_u64(&values).unwrap();
            prop_assert_eq!(summary.min, *values.iter().min().unwrap());
            prop_assert_eq!(summary.max, *values.iter().max().unwrap());
        }
    }
}
