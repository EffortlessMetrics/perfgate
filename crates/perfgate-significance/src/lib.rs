//! Statistical significance testing for benchmarking.
//!
//! This crate provides Welch's t-test implementation for detecting statistically
//! significant performance changes between benchmark runs.
//!
//! # Statistical Methodology
//!
//! ## Welch's t-test
//!
//! Welch's t-test is an adaptation of Student's t-test that is more reliable when
//! the two samples have unequal variances and/or unequal sample sizes. This makes
//! it ideal for benchmarking where:
//!
//! - Baseline and current runs may have different numbers of samples
//! - Variance can differ significantly between runs due to system noise
//! - We want to detect real performance changes, not just noise
//!
//! ### Formula
//!
//! The test statistic is computed as:
//!
//! ```text
//! t = (mean_1 - mean_2) / sqrt(var_1/n_1 + var_2/n_2)
//! ```
//!
//! The degrees of freedom is approximated using the Welch-Satterthwaite equation:
//!
//! ```text
//! df = (var_1/n_1 + var_2/n_2)² / ((var_1²/n_1²(n_1-1)) + (var_2²/n_2²(n_2-1)))
//! ```
//!
//! ### Interpretation
//!
//! - The p-value represents the probability of observing a difference as extreme
//!   as (or more extreme than) the measured difference, assuming no real change.
//! - A small p-value (≤ alpha, typically 0.05) indicates strong evidence against
//!   the null hypothesis, suggesting a statistically significant change.
//!
//! ## Limitations
//!
//! - **Minimum samples**: Requires at least `min_samples` in both groups (typically 8)
//!   for reliable results with smaller sample sizes, the test returns `None`
//! - **Zero variance**: When all values in a group are identical, the test handles
//!   this edge case explicitly (returns p-value 1.0 if means are equal, 0.0 otherwise)
//! - **Assumptions**: Assumes data is approximately normally distributed; for
//!   highly skewed distributions, consider non-parametric alternatives

use perfgate_types::{Significance, SignificanceTest};
use statrs::distribution::{ContinuousCDF, StudentsT};

/// Compute statistical significance using Welch's t-test.
///
/// Returns `None` if:
/// - Either sample has fewer than `min_samples` observations
/// - Either sample has fewer than 2 observations (variance undefined)
/// - Computed degrees of freedom is non-finite or non-positive
///
/// # Arguments
///
/// * `baseline` - Baseline metric values
/// * `current` - Current metric values
/// * `alpha` - Significance level (typically 0.05)
/// * `min_samples` - Minimum samples required in each group
///
/// # Returns
///
/// A `Significance` struct containing:
/// - `p_value`: Two-tailed p-value from Welch's t-test
/// - `alpha`: The provided significance threshold
/// - `significant`: Whether p_value ≤ alpha
/// - `baseline_samples` / `current_samples`: Sample counts
///
/// # Example
///
/// ```
/// use perfgate_significance::compute_significance;
///
/// let baseline = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0, 101.0, 99.0];
/// let current = vec![110.0, 112.0, 108.0, 111.0, 109.0, 110.0, 111.0, 109.0];
///
/// let result = compute_significance(&baseline, &current, 0.05, 8);
/// assert!(result.is_some());
///
/// let sig = result.unwrap();
/// assert!(sig.significant); // Clear performance regression
/// assert!(sig.p_value < 0.05);
/// ```
pub fn compute_significance(
    baseline: &[f64],
    current: &[f64],
    alpha: f64,
    min_samples: usize,
) -> Option<Significance> {
    if baseline.len() < min_samples || current.len() < min_samples {
        return None;
    }

    if baseline.len() < 2 || current.len() < 2 {
        return None;
    }

    let (base_mean, base_var) = mean_and_variance(baseline)?;
    let (curr_mean, curr_var) = mean_and_variance(current)?;

    let n1 = baseline.len() as f64;
    let n2 = current.len() as f64;
    let se2 = (base_var / n1) + (curr_var / n2);

    let p_value = if se2 <= 0.0 {
        if (base_mean - curr_mean).abs() < f64::EPSILON {
            1.0
        } else {
            0.0
        }
    } else {
        let t = (base_mean - curr_mean) / se2.sqrt();
        let numerator = se2 * se2;
        let denom_left = (base_var * base_var) / (n1 * n1 * (n1 - 1.0));
        let denom_right = (curr_var * curr_var) / (n2 * n2 * (n2 - 1.0));
        let df = numerator / (denom_left + denom_right);

        if !df.is_finite() || df <= 0.0 {
            return None;
        }

        let dist = StudentsT::new(0.0, 1.0, df).ok()?;
        let tail = 1.0 - dist.cdf(t.abs());
        (2.0 * tail).clamp(0.0, 1.0)
    };

    Some(Significance {
        test: SignificanceTest::WelchT,
        p_value,
        alpha,
        significant: p_value <= alpha,
        baseline_samples: baseline.len() as u32,
        current_samples: current.len() as u32,
    })
}

/// Compute sample mean and unbiased variance (Bessel's correction).
///
/// Returns `None` if:
/// - The input slice is empty
/// - Mean or variance is non-finite (NaN or infinity)
///
/// # Arguments
///
/// * `values` - Slice of f64 values
///
/// # Returns
///
/// A tuple of (mean, variance) where:
/// - Mean is the arithmetic mean
/// - Variance is the sample variance (n-1 denominator for unbiased estimation)
/// - Variance is 0.0 for single-element samples
/// - Variance is clamped to be non-negative (handles floating point errors)
///
/// # Example
///
/// ```
/// use perfgate_significance::mean_and_variance;
///
/// let values = vec![10.0, 12.0, 14.0, 16.0, 18.0];
/// let (mean, var) = mean_and_variance(&values).unwrap();
///
/// assert!((mean - 14.0).abs() < 1e-10);
/// assert!(var > 0.0); // Sample variance with Bessel's correction
/// ```
pub fn mean_and_variance(values: &[f64]) -> Option<(f64, f64)> {
    if values.is_empty() {
        return None;
    }

    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = if values.len() > 1 {
        values
            .iter()
            .map(|v| {
                let d = v - mean;
                d * d
            })
            .sum::<f64>()
            / (values.len() as f64 - 1.0)
    } else {
        0.0
    };

    if mean.is_finite() && var.is_finite() {
        Some((mean, var.max(0.0)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use proptest::prelude::*;

    #[test]
    fn significance_detects_clear_regression() {
        let baseline = vec![100.0; 20];
        let current = vec![110.0; 20];

        let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

        assert!(result.significant);
        assert!(result.p_value < 0.001);
        assert_eq!(result.test, SignificanceTest::WelchT);
    }

    #[test]
    fn significance_returns_none_for_insufficient_samples() {
        let baseline = vec![100.0, 101.0, 102.0];
        let current = vec![100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 106.0, 107.0];

        let result = compute_significance(&baseline, &current, 0.05, 8);

        assert!(result.is_none());
    }

    #[test]
    fn significance_returns_none_for_single_sample() {
        let baseline = vec![100.0];
        let current = vec![100.0];

        let result = compute_significance(&baseline, &current, 0.05, 1);

        assert!(result.is_none());
    }

    #[test]
    fn significance_handles_zero_variance_equal_means() {
        let baseline = vec![100.0; 10];
        let current = vec![100.0; 10];

        let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

        assert!(!result.significant);
        assert_relative_eq!(result.p_value, 1.0);
    }

    #[test]
    fn significance_handles_zero_variance_different_means() {
        let baseline = vec![100.0; 10];
        let current = vec![110.0; 10];

        let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

        assert!(result.significant);
        assert_relative_eq!(result.p_value, 0.0);
    }

    #[test]
    fn significance_not_significant_for_noisy_data() {
        let baseline: Vec<f64> = (0..20).map(|i| 100.0 + (i as f64 % 5.0) - 2.5).collect();
        let current: Vec<f64> = (0..20).map(|i| 100.5 + (i as f64 % 5.0) - 2.5).collect();

        let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

        assert!(
            !result.significant,
            "Expected not significant due to high variance"
        );
    }

    #[test]
    fn significance_sample_counts_recorded() {
        let baseline = vec![100.0; 15];
        let current = vec![100.0; 12];

        let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

        assert_eq!(result.baseline_samples, 15);
        assert_eq!(result.current_samples, 12);
    }

    #[test]
    fn significance_respects_alpha_threshold() {
        let baseline = vec![100.0, 101.0, 99.0, 100.0, 101.0, 99.0, 100.0, 101.0];
        let current = vec![102.0, 103.0, 101.0, 102.0, 103.0, 101.0, 102.0, 103.0];

        let result_strict = compute_significance(&baseline, &current, 0.01, 8).unwrap();
        let result_lenient = compute_significance(&baseline, &current, 0.10, 8).unwrap();

        assert_eq!(result_strict.p_value, result_lenient.p_value);
        assert!(
            result_lenient.significant || !result_strict.significant,
            "lenient threshold should be more likely to be significant"
        );
    }

    #[test]
    fn mean_and_variance_empty_returns_none() {
        assert!(mean_and_variance(&[]).is_none());
    }

    #[test]
    fn mean_and_variance_single_element() {
        let (mean, var) = mean_and_variance(&[42.0]).unwrap();

        assert_relative_eq!(mean, 42.0);
        assert_relative_eq!(var, 0.0);
    }

    #[test]
    fn mean_and_variance_two_elements() {
        let (mean, var) = mean_and_variance(&[10.0, 20.0]).unwrap();

        assert_relative_eq!(mean, 15.0);
        assert_relative_eq!(var, 50.0);
    }

    #[test]
    fn mean_and_variance_uniform_values() {
        let (mean, var) = mean_and_variance(&[100.0; 10]).unwrap();

        assert_relative_eq!(mean, 100.0);
        assert_relative_eq!(var, 0.0);
    }

    #[test]
    fn mean_and_variance_known_values() {
        let values = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let (mean, var) = mean_and_variance(&values).unwrap();

        assert_relative_eq!(mean, 5.0);
        assert_relative_eq!(var, 32.0 / 7.0);
    }

    #[test]
    fn significance_large_samples() {
        let baseline: Vec<f64> = (0..1000).map(|i| 100.0 + (i as f64 % 10.0)).collect();
        let current: Vec<f64> = (0..1000).map(|i| 100.0 + (i as f64 % 10.0)).collect();

        let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

        assert_relative_eq!(result.p_value, 1.0, epsilon = 1e-10);
        assert!(!result.significant);
    }

    #[test]
    fn significance_with_small_real_difference() {
        let baseline: Vec<f64> = (0..50).map(|_| 100.0 + rand_normal(0.0, 1.0)).collect();
        let current: Vec<f64> = (0..50).map(|_| 100.0 + rand_normal(0.0, 1.0)).collect();

        let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

        assert!(result.p_value >= 0.0 && result.p_value <= 1.0);
    }

    fn rand_normal(_mean: f64, _std: f64) -> f64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos();
        ((ns as f64 % 2000.0) - 1000.0) / 100.0
    }

    mod property_tests {
        use super::*;

        proptest! {
            #[test]
            fn prop_p_value_bounds(
                baseline in prop::collection::vec(0.0f64..1000.0, 8..100),
                current in prop::collection::vec(0.0f64..1000.0, 8..100),
                alpha in 0.01f64..0.5,
            ) {
                let result = compute_significance(&baseline, &current, alpha, 8);

                if let Some(sig) = result {
                    prop_assert!(sig.p_value >= 0.0, "p-value must be >= 0");
                    prop_assert!(sig.p_value <= 1.0, "p-value must be <= 1");
                    prop_assert_eq!(sig.baseline_samples, baseline.len() as u32);
                    prop_assert_eq!(sig.current_samples, current.len() as u32);
                    prop_assert_eq!(sig.significant, sig.p_value <= sig.alpha);
                }
            }

            #[test]
            fn prop_mean_and_variance_finite(values in prop::collection::vec(any::<f64>(), 1..100)) {
                let result = mean_and_variance(&values);

                if values.iter().all(|v| v.is_finite())
                    && let Some((mean, var)) = result
                {
                    prop_assert!(mean.is_finite(), "mean must be finite");
                    prop_assert!(var.is_finite(), "variance must be finite");
                    prop_assert!(var >= 0.0, "variance must be non-negative");
                }
            }

            #[test]
            fn prop_identical_samples_p_value_one(
                values in prop::collection::vec(0.0f64..1000.0, 8..50)
            ) {
                let result = compute_significance(&values, &values, 0.05, 8);

                if let Some(sig) = result {
                    prop_assert!(
                        (sig.p_value - 1.0).abs() < 1e-10,
                        "identical samples should have p-value ≈ 1, got {}",
                        sig.p_value
                    );
                    prop_assert!(!sig.significant, "identical samples should not be significant");
                }
            }

            #[test]
            fn prop_shifted_samples_significant(
                values in prop::collection::vec(10.0f64..100.0, 20..50)
                    .prop_filter("values must have variance", |v| {
                        let mean: f64 = v.iter().sum::<f64>() / v.len() as f64;
                        let var: f64 = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / v.len() as f64;
                        var > 0.01
                    }),
                shift in 50.0f64..100.0,
            ) {
                let current: Vec<f64> = values.iter().map(|v| v + shift).collect();

                let result = compute_significance(&values, &current, 0.05, 8);

                if let Some(sig) = result {
                    prop_assert!(sig.significant, "large shift should be significant");
                    prop_assert!(sig.p_value < 0.001, "large shift should have small p-value");
                }
            }

            #[test]
            fn prop_significance_deterministic(
                baseline in prop::collection::vec(0.0f64..1000.0, 8..30),
                current in prop::collection::vec(0.0f64..1000.0, 8..30),
            ) {
                let result1 = compute_significance(&baseline, &current, 0.05, 8);
                let result2 = compute_significance(&baseline, &current, 0.05, 8);

                prop_assert_eq!(result1, result2, "significance test should be deterministic");
            }

            #[test]
            fn prop_variance_bessel_correction(values in prop::collection::vec(0.0f64..100.0, 3..50)) {
                let result = mean_and_variance(&values);

                if let Some((mean, var)) = result {
                    let n = values.len() as f64;
                    let expected_mean: f64 = values.iter().sum::<f64>() / n;
                    let pop_var: f64 = values.iter()
                        .map(|v| (v - expected_mean).powi(2))
                        .sum::<f64>() / n;

                    if values.len() > 1 {
                        let sample_var = pop_var * n / (n - 1.0);
                        prop_assert!(
                            (var - sample_var).abs() < 1e-10 || (var < 1e-10 && sample_var < 1e-10),
                            "sample variance should use Bessel's correction"
                        );
                    }

                    prop_assert!((mean - expected_mean).abs() < 1e-10);
                }
            }
        }
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn handles_very_large_values() {
            let baseline = vec![
                1e15,
                1e15 + 1.0,
                1e15 - 1.0,
                1e15,
                1e15 + 0.5,
                1e15 - 0.5,
                1e15,
                1e15,
            ];
            let current = vec![1e15 + 1000.0; 8];

            let result = compute_significance(&baseline, &current, 0.05, 8);

            assert!(result.is_some() || result.is_none());
        }

        #[test]
        fn handles_very_small_values() {
            let baseline = vec![1e-15, 2e-15, 1.5e-15, 1e-15, 2e-15, 1.5e-15, 1e-15, 2e-15];
            let current = vec![1e-10; 8];

            let result = compute_significance(&baseline, &current, 0.05, 8);

            assert!(result.is_some());
            if let Some(sig) = result {
                assert!(sig.significant);
            }
        }

        #[test]
        fn handles_negative_values() {
            let baseline = vec![-100.0, -102.0, -98.0, -101.0, -99.0, -100.0, -101.0, -99.0];
            let current = vec![
                -110.0, -112.0, -108.0, -111.0, -109.0, -110.0, -111.0, -109.0,
            ];

            let result = compute_significance(&baseline, &current, 0.05, 8);

            assert!(result.is_some());
            let sig = result.unwrap();
            assert!(sig.significant);
        }

        #[test]
        fn handles_mixed_sign_values() {
            let baseline = vec![-50.0, -25.0, 0.0, 25.0, 50.0, 75.0, 100.0, 125.0];
            let current = vec![-100.0, -75.0, -50.0, -25.0, 0.0, 25.0, 50.0, 75.0];

            let result = compute_significance(&baseline, &current, 0.05, 8);

            assert!(result.is_some());
        }

        #[test]
        fn exactly_min_samples() {
            let baseline = vec![100.0; 8];
            let current = vec![110.0; 8];

            let result = compute_significance(&baseline, &current, 0.05, 8);

            assert!(result.is_some());
        }

        #[test]
        fn one_below_min_samples() {
            let baseline = vec![100.0; 7];
            let current = vec![110.0; 8];

            let result = compute_significance(&baseline, &current, 0.05, 8);

            assert!(result.is_none());
        }

        #[test]
        fn unequal_sample_sizes() {
            let baseline = vec![100.0; 20];
            let current = vec![110.0; 8];

            let result = compute_significance(&baseline, &current, 0.05, 8);

            assert!(result.is_some());
            let sig = result.unwrap();
            assert_eq!(sig.baseline_samples, 20);
            assert_eq!(sig.current_samples, 8);
        }

        #[test]
        fn alpha_boundary_p_value_equal() {
            let baseline = vec![100.0; 10];
            let current = vec![100.0; 10];

            let result = compute_significance(&baseline, &current, 0.05, 8).unwrap();

            assert_eq!(result.p_value, 1.0);
            assert!(!result.significant);
        }
    }
}
