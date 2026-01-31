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
    use perfgate_types::{Sample, U64Summary};
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
