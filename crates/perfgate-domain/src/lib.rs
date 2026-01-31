//! Domain logic for perfgate.
//!
//! This crate is intentionally I/O-free: it does math and policy.

use perfgate_types::{
    Budget, Delta, Direction, F64Summary, Metric, MetricStatus, Stats, U64Summary, Verdict,
    VerdictCounts, VerdictStatus,
};
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("no samples to summarize")]
    NoSamples,

    #[error("baseline value for {0:?} must be > 0")]
    InvalidBaseline(Metric),
}

pub fn summarize_u64(values: &[u64]) -> Result<U64Summary, DomainError> {
    if values.is_empty() {
        return Err(DomainError::NoSamples);
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let min = *v.first().unwrap();
    let max = *v.last().unwrap();
    let median = median_u64_sorted(&v);
    Ok(U64Summary { median, min, max })
}

pub fn summarize_f64(values: &[f64]) -> Result<F64Summary, DomainError> {
    if values.is_empty() {
        return Err(DomainError::NoSamples);
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let min = *v.first().unwrap();
    let max = *v.last().unwrap();
    let median = median_f64_sorted(&v);
    Ok(F64Summary { median, min, max })
}

fn median_u64_sorted(sorted: &[u64]) -> u64 {
    debug_assert!(!sorted.is_empty());
    let n = sorted.len();
    let mid = n / 2;
    if n % 2 == 1 {
        sorted[mid]
    } else {
        // average, rounding down
        (sorted[mid - 1] / 2) + (sorted[mid] / 2) + ((sorted[mid - 1] % 2 + sorted[mid] % 2) / 2)
    }
}

fn median_f64_sorted(sorted: &[f64]) -> f64 {
    debug_assert!(!sorted.is_empty());
    let n = sorted.len();
    let mid = n / 2;
    if n % 2 == 1 {
        sorted[mid]
    } else {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    }
}

/// Compute perfgate stats from samples.
///
/// Warmup samples (`sample.warmup == true`) are excluded.
pub fn compute_stats(
    samples: &[perfgate_types::Sample],
    work_units: Option<u64>,
) -> Result<Stats, DomainError> {
    let measured: Vec<&perfgate_types::Sample> = samples.iter().filter(|s| !s.warmup).collect();
    if measured.is_empty() {
        return Err(DomainError::NoSamples);
    }

    let wall: Vec<u64> = measured.iter().map(|s| s.wall_ms).collect();
    let wall_ms = summarize_u64(&wall)?;

    let rss_vals: Vec<u64> = measured.iter().filter_map(|s| s.max_rss_kb).collect();
    let max_rss_kb = if rss_vals.is_empty() {
        None
    } else {
        Some(summarize_u64(&rss_vals)?)
    };

    let throughput_per_s = match work_units {
        Some(work) => {
            let thr: Vec<f64> = measured
                .iter()
                .map(|s| {
                    let secs = (s.wall_ms as f64) / 1000.0;
                    if secs <= 0.0 {
                        0.0
                    } else {
                        (work as f64) / secs
                    }
                })
                .collect();
            Some(summarize_f64(&thr)?)
        }
        None => None,
    };

    Ok(Stats {
        wall_ms,
        max_rss_kb,
        throughput_per_s,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    pub deltas: BTreeMap<Metric, Delta>,
    pub verdict: Verdict,
}

/// Compare stats under the provided budgets.
///
/// Metrics without both baseline+current values are skipped (and therefore do not affect verdict).
pub fn compare_stats(
    baseline: &Stats,
    current: &Stats,
    budgets: &BTreeMap<Metric, Budget>,
) -> Result<Comparison, DomainError> {
    let mut deltas: BTreeMap<Metric, Delta> = BTreeMap::new();
    let mut reasons: Vec<String> = Vec::new();

    let mut counts = VerdictCounts {
        pass: 0,
        warn: 0,
        fail: 0,
    };

    for (metric, budget) in budgets {
        let b = metric_value(baseline, *metric);
        let c = metric_value(current, *metric);

        let (Some(bv), Some(cv)) = (b, c) else {
            continue;
        };

        if bv <= 0.0 {
            return Err(DomainError::InvalidBaseline(*metric));
        }

        let ratio = cv / bv;
        let pct = (cv - bv) / bv;

        let regression = match budget.direction {
            Direction::Lower => pct.max(0.0),
            Direction::Higher => (-pct).max(0.0),
        };

        let status = if regression > budget.threshold {
            MetricStatus::Fail
        } else if regression >= budget.warn_threshold {
            MetricStatus::Warn
        } else {
            MetricStatus::Pass
        };

        match status {
            MetricStatus::Pass => counts.pass += 1,
            MetricStatus::Warn => {
                counts.warn += 1;
                reasons.push(format!(
                    "{metric:?} near budget: {pct:.2}% (warn ≥ {warn:.2}%, fail > {fail:.2}%)",
                    metric = metric,
                    pct = pct * 100.0,
                    warn = budget.warn_threshold * 100.0,
                    fail = budget.threshold * 100.0
                ));
            }
            MetricStatus::Fail => {
                counts.fail += 1;
                reasons.push(format!(
                    "{metric:?} regression: {pct:.2}% (budget {fail:.2}%)",
                    metric = metric,
                    pct = pct * 100.0,
                    fail = budget.threshold * 100.0
                ));
            }
        }

        deltas.insert(
            *metric,
            Delta {
                baseline: bv,
                current: cv,
                ratio,
                pct,
                regression,
                status,
            },
        );
    }

    let status = if counts.fail > 0 {
        VerdictStatus::Fail
    } else if counts.warn > 0 {
        VerdictStatus::Warn
    } else {
        VerdictStatus::Pass
    };

    Ok(Comparison {
        deltas,
        verdict: Verdict {
            status,
            counts,
            reasons,
        },
    })
}

fn metric_value(stats: &Stats, metric: Metric) -> Option<f64> {
    match metric {
        Metric::WallMs => Some(stats.wall_ms.median as f64),
        Metric::MaxRssKb => stats.max_rss_kb.as_ref().map(|s| s.median as f64),
        Metric::ThroughputPerS => stats.throughput_per_s.as_ref().map(|s| s.median),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{
        Budget, Direction, F64Summary, Metric, MetricStatus, Sample, Stats, U64Summary,
        VerdictStatus,
    };
    use proptest::prelude::*;

    #[test]
    fn summarize_u64_median_even_rounds_down() {
        let s = summarize_u64(&[10, 20]).unwrap();
        assert_eq!(s.median, 15);
    }

    // =========================================================================
    // Property-Based Tests
    // =========================================================================

    /// **Validates: Requirements 3.1, 3.2, 3.3**
    ///
    /// Property 1: Statistics Computation Correctness
    ///
    /// For any non-empty list of u64 values, the computed summary SHALL have:
    /// - `median` equal to the middle value (or average of two middle values for even-length lists)
    /// - `min` equal to the smallest value
    /// - `max` equal to the largest value
    mod property_tests {
        use super::*;

        /// Helper function to compute the expected median for a sorted slice.
        /// For even-length lists, computes the average of the two middle values,
        /// matching the implementation's rounding behavior.
        fn expected_median(sorted: &[u64]) -> u64 {
            let n = sorted.len();
            let mid = n / 2;
            if n % 2 == 1 {
                sorted[mid]
            } else {
                // Match the implementation's rounding: avoid overflow by splitting
                let a = sorted[mid - 1];
                let b = sorted[mid];
                (a / 2) + (b / 2) + ((a % 2 + b % 2) / 2)
            }
        }

        proptest! {
            /// **Validates: Requirements 3.1, 3.2, 3.3**
            ///
            /// Property 1: Statistics Computation Correctness
            ///
            /// For any non-empty list of u64 values:
            /// - min equals the smallest value
            /// - max equals the largest value
            /// - median equals the middle value (or average of two middle for even-length)
            #[test]
            fn prop_summarize_u64_correctness(values in prop::collection::vec(any::<u64>(), 1..100)) {
                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                // Sort the values to compute expected results
                let mut sorted = values.clone();
                sorted.sort_unstable();

                // Property: min is the smallest value
                let expected_min = *sorted.first().unwrap();
                prop_assert_eq!(
                    summary.min, expected_min,
                    "min should be the smallest value"
                );

                // Property: max is the largest value
                let expected_max = *sorted.last().unwrap();
                prop_assert_eq!(
                    summary.max, expected_max,
                    "max should be the largest value"
                );

                // Property: median is correct
                let expected_med = expected_median(&sorted);
                prop_assert_eq!(
                    summary.median, expected_med,
                    "median should be the middle value (or average for even-length)"
                );
            }

            /// **Validates: Requirements 3.1, 3.2, 3.3**
            ///
            /// Property: min <= median <= max always holds
            #[test]
            fn prop_summarize_u64_ordering(values in prop::collection::vec(any::<u64>(), 1..100)) {
                let summary = summarize_u64(&values).expect("non-empty vec should succeed");

                prop_assert!(
                    summary.min <= summary.median,
                    "min ({}) should be <= median ({})",
                    summary.min, summary.median
                );
                prop_assert!(
                    summary.median <= summary.max,
                    "median ({}) should be <= max ({})",
                    summary.median, summary.max
                );
            }

            /// **Validates: Requirements 3.1, 3.2, 3.3**
            ///
            /// Property: For single-element vectors, min == median == max
            #[test]
            fn prop_summarize_u64_single_element(value: u64) {
                let summary = summarize_u64(&[value]).expect("single element should succeed");

                prop_assert_eq!(summary.min, value, "min should equal the single value");
                prop_assert_eq!(summary.max, value, "max should equal the single value");
                prop_assert_eq!(summary.median, value, "median should equal the single value");
            }
        }

        // =====================================================================
        // Property 2: Warmup Sample Exclusion
        // =====================================================================

        /// Helper to generate a non-warmup sample with arbitrary wall_ms
        fn non_warmup_sample(wall_ms: u64) -> Sample {
            Sample {
                wall_ms,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                max_rss_kb: None,
                stdout: None,
                stderr: None,
            }
        }

        /// Helper to generate a warmup sample with arbitrary wall_ms
        fn warmup_sample(wall_ms: u64) -> Sample {
            Sample {
                wall_ms,
                exit_code: 0,
                warmup: true,
                timed_out: false,
                max_rss_kb: None,
                stdout: None,
                stderr: None,
            }
        }

        proptest! {
            /// **Validates: Requirements 3.4**
            ///
            /// Property 2: Warmup Sample Exclusion
            ///
            /// For any list of samples containing both warmup and non-warmup samples,
            /// the computed statistics SHALL only reflect non-warmup samples.
            /// Adding or modifying warmup samples SHALL NOT change the computed statistics.
            #[test]
            fn prop_warmup_samples_excluded_from_stats(
                // Generate 1-20 non-warmup sample wall_ms values
                non_warmup_wall_ms in prop::collection::vec(1u64..10000, 1..20),
                // Generate 0-10 warmup sample wall_ms values (can be any values)
                warmup_wall_ms in prop::collection::vec(any::<u64>(), 0..10),
            ) {
                // Create non-warmup samples
                let non_warmup_samples: Vec<Sample> = non_warmup_wall_ms
                    .iter()
                    .map(|&ms| non_warmup_sample(ms))
                    .collect();

                // Create warmup samples
                let warmup_samples: Vec<Sample> = warmup_wall_ms
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Compute stats with only non-warmup samples
                let stats_without_warmup = compute_stats(&non_warmup_samples, None)
                    .expect("non-empty non-warmup samples should succeed");

                // Combine non-warmup and warmup samples
                let mut combined_samples = non_warmup_samples.clone();
                combined_samples.extend(warmup_samples.clone());

                // Compute stats with combined samples (warmup + non-warmup)
                let stats_with_warmup = compute_stats(&combined_samples, None)
                    .expect("combined samples with non-warmup should succeed");

                // Property: Statistics should be identical regardless of warmup samples
                prop_assert_eq!(
                    stats_without_warmup.wall_ms, stats_with_warmup.wall_ms,
                    "wall_ms stats should be identical with or without warmup samples"
                );
                prop_assert_eq!(
                    stats_without_warmup.max_rss_kb, stats_with_warmup.max_rss_kb,
                    "max_rss_kb stats should be identical with or without warmup samples"
                );
                prop_assert_eq!(
                    stats_without_warmup.throughput_per_s, stats_with_warmup.throughput_per_s,
                    "throughput_per_s stats should be identical with or without warmup samples"
                );
            }

            /// **Validates: Requirements 3.4**
            ///
            /// Property 2: Warmup Sample Exclusion (modification variant)
            ///
            /// Modifying warmup sample values SHALL NOT change the computed statistics.
            #[test]
            fn prop_modifying_warmup_samples_does_not_affect_stats(
                // Generate 1-10 non-warmup sample wall_ms values
                non_warmup_wall_ms in prop::collection::vec(1u64..10000, 1..10),
                // Generate 1-5 warmup sample wall_ms values (original)
                warmup_wall_ms_original in prop::collection::vec(any::<u64>(), 1..5),
                // Generate 1-5 warmup sample wall_ms values (modified - different values)
                warmup_wall_ms_modified in prop::collection::vec(any::<u64>(), 1..5),
            ) {
                // Create non-warmup samples
                let non_warmup_samples: Vec<Sample> = non_warmup_wall_ms
                    .iter()
                    .map(|&ms| non_warmup_sample(ms))
                    .collect();

                // Create original warmup samples
                let warmup_samples_original: Vec<Sample> = warmup_wall_ms_original
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Create modified warmup samples (different values)
                let warmup_samples_modified: Vec<Sample> = warmup_wall_ms_modified
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Combine with original warmup samples
                let mut samples_with_original_warmup = non_warmup_samples.clone();
                samples_with_original_warmup.extend(warmup_samples_original);

                // Combine with modified warmup samples
                let mut samples_with_modified_warmup = non_warmup_samples.clone();
                samples_with_modified_warmup.extend(warmup_samples_modified);

                // Compute stats with original warmup samples
                let stats_original = compute_stats(&samples_with_original_warmup, None)
                    .expect("samples with original warmup should succeed");

                // Compute stats with modified warmup samples
                let stats_modified = compute_stats(&samples_with_modified_warmup, None)
                    .expect("samples with modified warmup should succeed");

                // Property: Statistics should be identical regardless of warmup sample values
                prop_assert_eq!(
                    stats_original.wall_ms, stats_modified.wall_ms,
                    "wall_ms stats should be identical regardless of warmup sample values"
                );
            }

            /// **Validates: Requirements 3.4**
            ///
            /// Property 2: Warmup Sample Exclusion (only warmup samples error)
            ///
            /// If all samples are warmup samples, compute_stats SHALL return an error.
            #[test]
            fn prop_only_warmup_samples_returns_error(
                warmup_wall_ms in prop::collection::vec(any::<u64>(), 1..10),
            ) {
                // Create only warmup samples
                let warmup_only_samples: Vec<Sample> = warmup_wall_ms
                    .iter()
                    .map(|&ms| warmup_sample(ms))
                    .collect();

                // Compute stats should fail with NoSamples error
                let result = compute_stats(&warmup_only_samples, None);

                prop_assert!(
                    result.is_err(),
                    "compute_stats should return error when all samples are warmup"
                );

                // Verify it's specifically a NoSamples error
                match result {
                    Err(DomainError::NoSamples) => { /* expected */ }
                    Err(other) => prop_assert!(false, "expected NoSamples error, got: {:?}", other),
                    Ok(_) => prop_assert!(false, "expected error, got Ok"),
                }
            }
        }

        // =====================================================================
        // Property 4: Metric Status Determination
        // =====================================================================

        /// Helper to compute expected regression value based on direction.
        ///
        /// For Direction::Lower: regression = max(0, (current - baseline) / baseline)
        /// For Direction::Higher: regression = max(0, (baseline - current) / baseline)
        fn compute_regression(baseline: f64, current: f64, direction: Direction) -> f64 {
            let pct = (current - baseline) / baseline;
            match direction {
                Direction::Lower => pct.max(0.0),
                Direction::Higher => (-pct).max(0.0),
            }
        }

        /// Helper to compute expected status based on regression and thresholds.
        fn expected_status(regression: f64, threshold: f64, warn_threshold: f64) -> MetricStatus {
            if regression > threshold {
                MetricStatus::Fail
            } else if regression >= warn_threshold {
                MetricStatus::Warn
            } else {
                MetricStatus::Pass
            }
        }

        /// Strategy to generate valid threshold pairs where warn_threshold <= threshold.
        fn threshold_pair_strategy() -> impl Strategy<Value = (f64, f64)> {
            // Generate threshold in range (0.0, 1.0] and warn_factor in range [0.0, 1.0]
            (0.01f64..1.0, 0.0f64..=1.0).prop_map(|(threshold, warn_factor)| {
                let warn_threshold = threshold * warn_factor;
                (threshold, warn_threshold)
            })
        }

        /// Strategy to generate a valid baseline value (must be > 0).
        fn baseline_strategy() -> impl Strategy<Value = f64> {
            // Use positive values, avoiding very small values that could cause precision issues
            1.0f64..10000.0
        }

        /// Strategy to generate a current value (can be any positive value).
        fn current_strategy() -> impl Strategy<Value = f64> {
            // Use positive values
            0.1f64..20000.0
        }

        proptest! {
            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination
            ///
            /// For any baseline value, current value, threshold, warn_threshold, and direction:
            /// - If regression > threshold, status SHALL be Fail
            /// - If warn_threshold <= regression <= threshold, status SHALL be Warn
            /// - If regression < warn_threshold, status SHALL be Pass
            #[test]
            fn prop_metric_status_determination_lower_is_better(
                baseline in baseline_strategy(),
                current in current_strategy(),
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
                let direction = Direction::Lower;

                // Create stats for baseline and current
                let baseline_stats = Stats {
                    wall_ms: U64Summary {
                        median: baseline as u64,
                        min: baseline as u64,
                        max: baseline as u64,
                    },
                    max_rss_kb: None,
                    throughput_per_s: None,
                };

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: current as u64,
                        min: current as u64,
                        max: current as u64,
                    },
                    max_rss_kb: None,
                    throughput_per_s: None,
                };

                // Create budget with the generated thresholds
                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction,
                    },
                );

                // Compare stats
                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed with valid inputs");

                // Get the delta for WallMs
                let delta = comparison.deltas.get(&Metric::WallMs)
                    .expect("WallMs delta should exist");

                // Verify the status matches expected
                // Note: We use the actual median values (as u64) for comparison,
                // so we need to recompute expected based on actual values used
                let actual_baseline = baseline_stats.wall_ms.median as f64;
                let actual_current = current_stats.wall_ms.median as f64;
                let actual_regression = compute_regression(actual_baseline, actual_current, direction);
                let actual_expected = expected_status(actual_regression, threshold, warn_threshold);

                prop_assert_eq!(
                    delta.status, actual_expected,
                    "Status mismatch for Direction::Lower: baseline={}, current={}, regression={}, threshold={}, warn_threshold={}",
                    actual_baseline, actual_current, actual_regression, threshold, warn_threshold
                );
            }

            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination (Higher is Better)
            ///
            /// For Direction::Higher (e.g., throughput), regression is computed as
            /// max(0, (baseline - current) / baseline), meaning a decrease in value
            /// is a regression.
            #[test]
            fn prop_metric_status_determination_higher_is_better(
                baseline in baseline_strategy(),
                current in current_strategy(),
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
                let direction = Direction::Higher;

                // Create stats for baseline and current using throughput
                let baseline_stats = Stats {
                    wall_ms: U64Summary {
                        median: 1000,
                        min: 1000,
                        max: 1000,
                    },
                    max_rss_kb: None,
                    throughput_per_s: Some(F64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    }),
                };

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: 1000,
                        min: 1000,
                        max: 1000,
                    },
                    max_rss_kb: None,
                    throughput_per_s: Some(F64Summary {
                        median: current,
                        min: current,
                        max: current,
                    }),
                };

                // Create budget with the generated thresholds
                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::ThroughputPerS,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction,
                    },
                );

                // Compare stats
                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed with valid inputs");

                // Get the delta for ThroughputPerS
                let delta = comparison.deltas.get(&Metric::ThroughputPerS)
                    .expect("ThroughputPerS delta should exist");

                // Compute expected regression and status
                let regression = compute_regression(baseline, current, direction);
                let expected = expected_status(regression, threshold, warn_threshold);

                prop_assert_eq!(
                    delta.status, expected,
                    "Status mismatch for Direction::Higher: baseline={}, current={}, regression={}, threshold={}, warn_threshold={}",
                    baseline, current, regression, threshold, warn_threshold
                );
            }

            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination (Regression is non-negative)
            ///
            /// The regression value SHALL always be >= 0, regardless of whether
            /// performance improved or degraded.
            #[test]
            fn prop_regression_is_non_negative(
                baseline in baseline_strategy(),
                current in current_strategy(),
                (threshold, warn_threshold) in threshold_pair_strategy(),
                direction_lower in any::<bool>(),
            ) {
                let direction = if direction_lower { Direction::Lower } else { Direction::Higher };

                // Create appropriate stats based on direction
                let (baseline_stats, current_stats, metric, budgets) = if direction_lower {
                    let bs = Stats {
                        wall_ms: U64Summary {
                            median: baseline as u64,
                            min: baseline as u64,
                            max: baseline as u64,
                        },
                        max_rss_kb: None,
                        throughput_per_s: None,
                    };
                    let cs = Stats {
                        wall_ms: U64Summary {
                            median: current as u64,
                            min: current as u64,
                            max: current as u64,
                        },
                        max_rss_kb: None,
                        throughput_per_s: None,
                    };
                    let mut b = BTreeMap::new();
                    b.insert(Metric::WallMs, Budget { threshold, warn_threshold, direction });
                    (bs, cs, Metric::WallMs, b)
                } else {
                    let bs = Stats {
                        wall_ms: U64Summary { median: 1000, min: 1000, max: 1000 },
                        max_rss_kb: None,
                        throughput_per_s: Some(F64Summary {
                            median: baseline,
                            min: baseline,
                            max: baseline,
                        }),
                    };
                    let cs = Stats {
                        wall_ms: U64Summary { median: 1000, min: 1000, max: 1000 },
                        max_rss_kb: None,
                        throughput_per_s: Some(F64Summary {
                            median: current,
                            min: current,
                            max: current,
                        }),
                    };
                    let mut b = BTreeMap::new();
                    b.insert(Metric::ThroughputPerS, Budget { threshold, warn_threshold, direction });
                    (bs, cs, Metric::ThroughputPerS, b)
                };

                // Compare stats
                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed with valid inputs");

                // Get the delta
                let delta = comparison.deltas.get(&metric)
                    .expect("delta should exist");

                // Property: regression is always >= 0
                prop_assert!(
                    delta.regression >= 0.0,
                    "Regression should be non-negative, got: {} for baseline={}, current={}, direction={:?}",
                    delta.regression, baseline, current, direction
                );
            }

            /// **Validates: Requirements 5.1, 5.2, 5.3**
            ///
            /// Property 4: Metric Status Determination (Status boundaries)
            ///
            /// Verify the exact boundary conditions:
            /// - regression == threshold should be Warn (not Fail)
            /// - regression == warn_threshold should be Warn (not Pass)
            #[test]
            fn prop_status_boundary_conditions(
                baseline in 100.0f64..1000.0,
                (threshold, warn_threshold) in threshold_pair_strategy(),
            ) {
                let baseline_stats = Stats {
                    wall_ms: U64Summary { median: 1000, min: 1000, max: 1000 },
                    max_rss_kb: None,
                    throughput_per_s: Some(F64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    }),
                };

                // For Direction::Higher, regression = max(0, (baseline - current) / baseline)
                // To get regression = threshold, we need: (baseline - current) / baseline = threshold
                // So: current = baseline * (1 - threshold)
                let current_at_threshold_higher = baseline * (1.0 - threshold);

                // Only test if current would be positive
                if current_at_threshold_higher > 0.0 {
                    let current_stats = Stats {
                        wall_ms: U64Summary { median: 1000, min: 1000, max: 1000 },
                        max_rss_kb: None,
                        throughput_per_s: Some(F64Summary {
                            median: current_at_threshold_higher,
                            min: current_at_threshold_higher,
                            max: current_at_threshold_higher,
                        }),
                    };

                    let mut budgets = BTreeMap::new();
                    budgets.insert(
                        Metric::ThroughputPerS,
                        Budget {
                            threshold,
                            warn_threshold,
                            direction: Direction::Higher,
                        },
                    );

                    let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                        .expect("compare_stats should succeed");

                    let delta = comparison.deltas.get(&Metric::ThroughputPerS)
                        .expect("delta should exist");

                    // At exactly threshold, status should be Warn (not Fail)
                    // because the condition is regression > threshold for Fail
                    prop_assert!(
                        delta.status != MetricStatus::Fail || delta.regression > threshold,
                        "At regression={} (threshold={}), status should not be Fail unless regression > threshold",
                        delta.regression, threshold
                    );
                }
            }
        }

        // =====================================================================
        // Property 5: Verdict Aggregation
        // =====================================================================

        /// Strategy to generate a random MetricStatus.
        fn metric_status_strategy() -> impl Strategy<Value = MetricStatus> {
            prop_oneof![
                Just(MetricStatus::Pass),
                Just(MetricStatus::Warn),
                Just(MetricStatus::Fail),
            ]
        }

        /// Compute the expected verdict status from a set of metric statuses.
        ///
        /// - If any metric has Fail status, verdict SHALL be Fail
        /// - Else if any metric has Warn status, verdict SHALL be Warn
        /// - Else verdict SHALL be Pass
        fn expected_verdict_status(statuses: &[MetricStatus]) -> VerdictStatus {
            if statuses.contains(&MetricStatus::Fail) {
                VerdictStatus::Fail
            } else if statuses.contains(&MetricStatus::Warn) {
                VerdictStatus::Warn
            } else {
                VerdictStatus::Pass
            }
        }

        /// Helper to create Stats with a specific wall_ms median value.
        fn make_stats_with_wall_ms(median: u64) -> Stats {
            Stats {
                wall_ms: U64Summary {
                    median,
                    min: median,
                    max: median,
                },
                max_rss_kb: None,
                throughput_per_s: None,
            }
        }

        /// Helper to compute the current value needed to achieve a specific status.
        ///
        /// Given a baseline, threshold, warn_threshold, and desired status,
        /// returns a current value that will produce that status.
        fn current_for_status(
            baseline: u64,
            threshold: f64,
            warn_threshold: f64,
            status: MetricStatus,
        ) -> u64 {
            let baseline_f = baseline as f64;
            match status {
                // For Pass: regression < warn_threshold
                // regression = (current - baseline) / baseline
                // So current = baseline * (1 + regression)
                // Use regression = 0 (no change) for Pass
                MetricStatus::Pass => baseline,

                // For Warn: warn_threshold <= regression <= threshold
                // Use midpoint between warn_threshold and threshold
                MetricStatus::Warn => {
                    let regression = (warn_threshold + threshold) / 2.0;
                    (baseline_f * (1.0 + regression)).ceil() as u64
                }

                // For Fail: regression > threshold
                // Use threshold + 0.1 to ensure we exceed it
                MetricStatus::Fail => {
                    let regression = threshold + 0.1;
                    (baseline_f * (1.0 + regression)).ceil() as u64
                }
            }
        }

        proptest! {
            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation
            ///
            /// For any set of metric statuses:
            /// - If any metric has Fail status, verdict SHALL be Fail
            /// - Else if any metric has Warn status, verdict SHALL be Warn
            /// - Else verdict SHALL be Pass
            #[test]
            fn prop_verdict_aggregation_single_metric(
                status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                let baseline_stats = make_stats_with_wall_ms(baseline);
                let current_value = current_for_status(baseline, threshold, warn_threshold, status);
                let current_stats = make_stats_with_wall_ms(current_value);

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verify the verdict matches the expected aggregation
                let expected = expected_verdict_status(&[status]);
                prop_assert_eq!(
                    comparison.verdict.status, expected,
                    "Verdict should be {:?} when single metric status is {:?}",
                    expected, status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Multiple Metrics)
            ///
            /// Test with multiple metrics to verify aggregation across all metrics.
            #[test]
            fn prop_verdict_aggregation_multiple_metrics(
                wall_ms_status in metric_status_strategy(),
                max_rss_status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with both wall_ms and max_rss_kb
                let baseline_stats = Stats {
                    wall_ms: U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    }),
                    throughput_per_s: None,
                };

                // Compute current values to achieve desired statuses
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, wall_ms_status);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, max_rss_status);

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: wall_ms_current,
                        min: wall_ms_current,
                        max: wall_ms_current,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: max_rss_current,
                        min: max_rss_current,
                        max: max_rss_current,
                    }),
                    throughput_per_s: None,
                };

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                budgets.insert(
                    Metric::MaxRssKb,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verify the verdict matches the expected aggregation
                let expected = expected_verdict_status(&[wall_ms_status, max_rss_status]);
                prop_assert_eq!(
                    comparison.verdict.status, expected,
                    "Verdict should be {:?} when metric statuses are [{:?}, {:?}]",
                    expected, wall_ms_status, max_rss_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Three Metrics)
            ///
            /// Test with all three metric types to verify comprehensive aggregation.
            #[test]
            fn prop_verdict_aggregation_three_metrics(
                wall_ms_status in metric_status_strategy(),
                max_rss_status in metric_status_strategy(),
                throughput_status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let baseline_throughput = 100.0f64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with all three metrics
                let baseline_stats = Stats {
                    wall_ms: U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    }),
                    throughput_per_s: Some(F64Summary {
                        median: baseline_throughput,
                        min: baseline_throughput,
                        max: baseline_throughput,
                    }),
                };

                // Compute current values to achieve desired statuses
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, wall_ms_status);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, max_rss_status);

                // For throughput (higher is better), we need to invert the logic
                // Pass: current >= baseline (no regression)
                // Warn: current = baseline * (1 - midpoint of warn/threshold)
                // Fail: current = baseline * (1 - (threshold + 0.1))
                let throughput_current = match throughput_status {
                    MetricStatus::Pass => baseline_throughput,
                    MetricStatus::Warn => {
                        let regression = (warn_threshold + threshold) / 2.0;
                        baseline_throughput * (1.0 - regression)
                    }
                    MetricStatus::Fail => {
                        let regression = threshold + 0.1;
                        baseline_throughput * (1.0 - regression)
                    }
                };

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: wall_ms_current,
                        min: wall_ms_current,
                        max: wall_ms_current,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: max_rss_current,
                        min: max_rss_current,
                        max: max_rss_current,
                    }),
                    throughput_per_s: Some(F64Summary {
                        median: throughput_current,
                        min: throughput_current,
                        max: throughput_current,
                    }),
                };

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                budgets.insert(
                    Metric::MaxRssKb,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                budgets.insert(
                    Metric::ThroughputPerS,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Higher,
                    },
                );

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verify the verdict matches the expected aggregation
                let expected = expected_verdict_status(&[wall_ms_status, max_rss_status, throughput_status]);
                prop_assert_eq!(
                    comparison.verdict.status, expected,
                    "Verdict should be {:?} when metric statuses are [{:?}, {:?}, {:?}]",
                    expected, wall_ms_status, max_rss_status, throughput_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Fail dominates)
            ///
            /// If any metric has Fail status, the verdict SHALL be Fail,
            /// regardless of other metric statuses.
            #[test]
            fn prop_verdict_fail_dominates(
                other_status in metric_status_strategy(),
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with both wall_ms and max_rss_kb
                let baseline_stats = Stats {
                    wall_ms: U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    }),
                    throughput_per_s: None,
                };

                // wall_ms will be Fail, max_rss will be the random status
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, MetricStatus::Fail);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, other_status);

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: wall_ms_current,
                        min: wall_ms_current,
                        max: wall_ms_current,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: max_rss_current,
                        min: max_rss_current,
                        max: max_rss_current,
                    }),
                    throughput_per_s: None,
                };

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                budgets.insert(
                    Metric::MaxRssKb,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verdict should always be Fail when any metric is Fail
                prop_assert_eq!(
                    comparison.verdict.status, VerdictStatus::Fail,
                    "Verdict should be Fail when any metric has Fail status, regardless of other_status={:?}",
                    other_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (Warn without Fail)
            ///
            /// If no metric has Fail status but at least one has Warn status,
            /// the verdict SHALL be Warn.
            #[test]
            fn prop_verdict_warn_without_fail(
                // Generate only Pass or Warn statuses (no Fail)
                other_status in prop_oneof![Just(MetricStatus::Pass), Just(MetricStatus::Warn)],
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // Create baseline stats with both wall_ms and max_rss_kb
                let baseline_stats = Stats {
                    wall_ms: U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    }),
                    throughput_per_s: None,
                };

                // wall_ms will be Warn, max_rss will be Pass or Warn
                let wall_ms_current = current_for_status(baseline, threshold, warn_threshold, MetricStatus::Warn);
                let max_rss_current = current_for_status(baseline, threshold, warn_threshold, other_status);

                let current_stats = Stats {
                    wall_ms: U64Summary {
                        median: wall_ms_current,
                        min: wall_ms_current,
                        max: wall_ms_current,
                    },
                    max_rss_kb: Some(U64Summary {
                        median: max_rss_current,
                        min: max_rss_current,
                        max: max_rss_current,
                    }),
                    throughput_per_s: None,
                };

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                budgets.insert(
                    Metric::MaxRssKb,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verdict should be Warn when at least one metric is Warn and none are Fail
                prop_assert_eq!(
                    comparison.verdict.status, VerdictStatus::Warn,
                    "Verdict should be Warn when at least one metric is Warn and none are Fail, other_status={:?}",
                    other_status
                );
            }

            /// **Validates: Requirements 5.4, 5.5, 5.6**
            ///
            /// Property 5: Verdict Aggregation (All Pass)
            ///
            /// If all metrics have Pass status, the verdict SHALL be Pass.
            #[test]
            fn prop_verdict_all_pass(
                // Generate 1-3 metrics, all with Pass status
                num_metrics in 1usize..=3,
            ) {
                // Use fixed baseline and thresholds
                let baseline = 1000u64;
                let baseline_throughput = 100.0f64;
                let threshold = 0.20;
                let warn_threshold = 0.10;

                // All metrics will be Pass (current == baseline, no regression)
                let baseline_stats = Stats {
                    wall_ms: U64Summary {
                        median: baseline,
                        min: baseline,
                        max: baseline,
                    },
                    max_rss_kb: if num_metrics >= 2 {
                        Some(U64Summary {
                            median: baseline,
                            min: baseline,
                            max: baseline,
                        })
                    } else {
                        None
                    },
                    throughput_per_s: if num_metrics >= 3 {
                        Some(F64Summary {
                            median: baseline_throughput,
                            min: baseline_throughput,
                            max: baseline_throughput,
                        })
                    } else {
                        None
                    },
                };

                // Current stats are same as baseline (Pass status)
                let current_stats = baseline_stats.clone();

                let mut budgets = BTreeMap::new();
                budgets.insert(
                    Metric::WallMs,
                    Budget {
                        threshold,
                        warn_threshold,
                        direction: Direction::Lower,
                    },
                );
                if num_metrics >= 2 {
                    budgets.insert(
                        Metric::MaxRssKb,
                        Budget {
                            threshold,
                            warn_threshold,
                            direction: Direction::Lower,
                        },
                    );
                }
                if num_metrics >= 3 {
                    budgets.insert(
                        Metric::ThroughputPerS,
                        Budget {
                            threshold,
                            warn_threshold,
                            direction: Direction::Higher,
                        },
                    );
                }

                let comparison = compare_stats(&baseline_stats, &current_stats, &budgets)
                    .expect("compare_stats should succeed");

                // Verdict should be Pass when all metrics are Pass
                prop_assert_eq!(
                    comparison.verdict.status, VerdictStatus::Pass,
                    "Verdict should be Pass when all {} metrics have Pass status",
                    num_metrics
                );

                // Also verify the counts are correct
                prop_assert_eq!(
                    comparison.verdict.counts.pass, num_metrics as u32,
                    "Pass count should equal number of metrics"
                );
                prop_assert_eq!(
                    comparison.verdict.counts.warn, 0,
                    "Warn count should be 0"
                );
                prop_assert_eq!(
                    comparison.verdict.counts.fail, 0,
                    "Fail count should be 0"
                );
            }
        }
    }

    #[test]
    fn compute_stats_excludes_warmup() {
        let samples = vec![
            Sample {
                wall_ms: 100,
                exit_code: 0,
                warmup: true,
                timed_out: false,
                max_rss_kb: None,
                stdout: None,
                stderr: None,
            },
            Sample {
                wall_ms: 200,
                exit_code: 0,
                warmup: false,
                timed_out: false,
                max_rss_kb: None,
                stdout: None,
                stderr: None,
            },
        ];

        let stats = compute_stats(&samples, None).unwrap();
        assert_eq!(
            stats.wall_ms,
            U64Summary {
                median: 200,
                min: 200,
                max: 200
            }
        );
    }

    #[test]
    fn compare_lower_is_worse_regression_is_positive_pct() {
        let baseline = Stats {
            wall_ms: U64Summary {
                median: 1000,
                min: 1000,
                max: 1000,
            },
            max_rss_kb: None,
            throughput_per_s: None,
        };
        let current = Stats {
            wall_ms: U64Summary {
                median: 1100,
                min: 1100,
                max: 1100,
            },
            max_rss_kb: None,
            throughput_per_s: None,
        };

        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::WallMs,
            Budget {
                threshold: 0.20,
                warn_threshold: 0.18,
                direction: Direction::Lower,
            },
        );

        let c = compare_stats(&baseline, &current, &budgets).unwrap();
        let d = c.deltas.get(&Metric::WallMs).unwrap();
        assert!(d.pct > 0.0);
        assert_eq!(d.status, MetricStatus::Pass);
    }

    #[test]
    fn compare_higher_is_better_regression_is_negative_pct() {
        let baseline = Stats {
            wall_ms: U64Summary {
                median: 1000,
                min: 1000,
                max: 1000,
            },
            max_rss_kb: None,
            throughput_per_s: Some(F64Summary {
                median: 100.0,
                min: 100.0,
                max: 100.0,
            }),
        };
        let current = Stats {
            wall_ms: U64Summary {
                median: 1000,
                min: 1000,
                max: 1000,
            },
            max_rss_kb: None,
            throughput_per_s: Some(F64Summary {
                median: 92.0,
                min: 92.0,
                max: 92.0,
            }),
        };

        let mut budgets = BTreeMap::new();
        budgets.insert(
            Metric::ThroughputPerS,
            Budget {
                threshold: 0.15,
                warn_threshold: 0.135,
                direction: Direction::Higher,
            },
        );

        let c = compare_stats(&baseline, &current, &budgets).unwrap();
        let d = c.deltas.get(&Metric::ThroughputPerS).unwrap();
        assert!(d.pct < 0.0);
        assert_eq!(d.status, MetricStatus::Pass);
    }
}
