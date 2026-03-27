//! Paired benchmarking statistics for A/B comparison.
//!
//! This crate provides statistical functions for analyzing paired benchmark data,
//! where each measurement consists of a baseline and current observation from the
//! same experimental unit (e.g., same input, same machine configuration).
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Overview
//!
//! The crate provides:
//! - [`compute_paired_stats`] — Compute summary statistics from paired samples
//! - [`compare_paired_stats`] — Compare paired statistics with confidence intervals
//! - [`PairedComparison`] — Result struct with significance testing
//! - [`summarize_paired_diffs`] — Summarize the distribution of differences
//!
//! # Statistical Methodology
//!
//! ## Paired t-test
//!
//! The comparison uses a paired t-test approach:
//! - For n >= 30 samples: uses t-value of 1.96 (normal approximation)
//! - For n < 30 samples: uses t-value of 2.0 (conservative small-sample estimate)
//!
//! ## Confidence Intervals
//!
//! 95% confidence intervals are computed as:
//! ```text
//! CI = mean ± t_value × (std_dev / sqrt(n))
//! ```
//!
//! A result is considered statistically significant if the confidence interval
//! does not span zero (i.e., `ci_lower > 0` or `ci_upper < 0`).
//!
//! # Example
//!
//! ```
//! use perfgate_paired::{compute_paired_stats, compare_paired_stats, PairedError};
//! use perfgate_types::{PairedSample, PairedSampleHalf};
//!
//! fn make_half(wall_ms: u64) -> PairedSampleHalf {
//!     PairedSampleHalf {
//!         wall_ms,
//!         exit_code: 0,
//!         timed_out: false,
//!         max_rss_kb: None,
//!         stdout: None,
//!         stderr: None,
//!     }
//! }
//!
//! fn make_sample(idx: u32, baseline_ms: u64, current_ms: u64) -> PairedSample {
//!     PairedSample {
//!         pair_index: idx,
//!         warmup: false,
//!         baseline: make_half(baseline_ms),
//!         current: make_half(current_ms),
//!         wall_diff_ms: current_ms as i64 - baseline_ms as i64,
//!         rss_diff_kb: None,
//!     }
//! }
//!
//! let samples = vec![
//!     make_sample(0, 100, 95),   // 5ms improvement
//!     make_sample(1, 105, 100),  // 5ms improvement
//!     make_sample(2, 110, 103),  // 7ms improvement
//! ];
//!
//! let stats = compute_paired_stats(&samples, None, None)?;
//! let comparison = compare_paired_stats(&stats);
//!
//! println!("Mean diff: {:.2}ms", comparison.mean_diff_ms);
//! println!("% change: {:.2}%", comparison.pct_change * 100.0);
//! println!("Significant: {}", comparison.is_significant);
//! # Ok::<(), PairedError>(())
//! ```

use perfgate_stats::{summarize_f64, summarize_u64};
use perfgate_types::{
    PairedDiffSummary, PairedSample, PairedStats, Significance, SignificancePolicy,
};

pub use perfgate_error::PairedError;

/// Compute summary statistics from paired benchmark samples.
///
/// Filters out warmup samples, then computes wall-time, RSS, and
/// throughput summaries for both baseline and current runs, as well
/// as the distribution of their paired differences.
pub fn compute_paired_stats(
    samples: &[PairedSample],
    work_units: Option<u64>,
    significance_policy: Option<&SignificancePolicy>,
) -> Result<PairedStats, PairedError> {
    let measured: Vec<&PairedSample> = samples.iter().filter(|s| !s.warmup).collect();
    if measured.is_empty() {
        return Err(PairedError::NoSamples);
    }

    let baseline_wall: Vec<u64> = measured.iter().map(|s| s.baseline.wall_ms).collect();
    let current_wall: Vec<u64> = measured.iter().map(|s| s.current.wall_ms).collect();
    let wall_diffs: Vec<f64> = measured.iter().map(|s| s.wall_diff_ms as f64).collect();

    let baseline_wall_ms = summarize_u64(&baseline_wall).map_err(|_| PairedError::NoSamples)?;
    let current_wall_ms = summarize_u64(&current_wall).map_err(|_| PairedError::NoSamples)?;
    let wall_diff_ms = summarize_paired_diffs(&wall_diffs, significance_policy)?;

    let baseline_rss: Vec<u64> = measured
        .iter()
        .filter_map(|s| s.baseline.max_rss_kb)
        .collect();
    let current_rss: Vec<u64> = measured
        .iter()
        .filter_map(|s| s.current.max_rss_kb)
        .collect();
    let rss_diffs: Vec<f64> = measured
        .iter()
        .filter_map(|s| s.rss_diff_kb)
        .map(|d| d as f64)
        .collect();

    let baseline_max_rss_kb = if baseline_rss.is_empty() {
        None
    } else {
        Some(summarize_u64(&baseline_rss).map_err(|_| PairedError::NoSamples)?)
    };
    let current_max_rss_kb = if current_rss.is_empty() {
        None
    } else {
        Some(summarize_u64(&current_rss).map_err(|_| PairedError::NoSamples)?)
    };
    let rss_diff_kb = if rss_diffs.is_empty() {
        None
    } else {
        Some(summarize_paired_diffs(&rss_diffs, significance_policy)?)
    };

    let (baseline_throughput_per_s, current_throughput_per_s, throughput_diff_per_s) =
        match work_units {
            Some(work) => {
                let baseline_thr: Vec<f64> = measured
                    .iter()
                    .map(|s| {
                        let secs = s.baseline.wall_ms as f64 / 1000.0;
                        if secs <= 0.0 { 0.0 } else { work as f64 / secs }
                    })
                    .collect();
                let current_thr: Vec<f64> = measured
                    .iter()
                    .map(|s| {
                        let secs = s.current.wall_ms as f64 / 1000.0;
                        if secs <= 0.0 { 0.0 } else { work as f64 / secs }
                    })
                    .collect();
                let thr_diffs: Vec<f64> = baseline_thr
                    .iter()
                    .zip(current_thr.iter())
                    .map(|(b, c)| c - b)
                    .collect();
                (
                    Some(summarize_f64(&baseline_thr).map_err(|_| PairedError::NoSamples)?),
                    Some(summarize_f64(&current_thr).map_err(|_| PairedError::NoSamples)?),
                    Some(summarize_paired_diffs(&thr_diffs, significance_policy)?),
                )
            }
            None => (None, None, None),
        };

    Ok(PairedStats {
        baseline_wall_ms,
        current_wall_ms,
        wall_diff_ms,
        baseline_max_rss_kb,
        current_max_rss_kb,
        rss_diff_kb,
        baseline_throughput_per_s,
        current_throughput_per_s,
        throughput_diff_per_s,
    })
}

/// Summarize the distribution of paired differences.
pub fn summarize_paired_diffs(
    diffs: &[f64],
    policy: Option<&SignificancePolicy>,
) -> Result<PairedDiffSummary, PairedError> {
    if diffs.is_empty() {
        return Err(PairedError::NoSamples);
    }
    let count = diffs.len() as u32;
    let mean = diffs.iter().sum::<f64>() / count as f64;
    let mut sorted = diffs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if count % 2 == 1 {
        sorted[(count / 2) as usize]
    } else {
        (sorted[(count / 2 - 1) as usize] + sorted[(count / 2) as usize]) / 2.0
    };
    let min = *sorted.first().unwrap();
    let max = *sorted.last().unwrap();
    let variance = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / count as f64;
    let std_dev = variance.sqrt();

    let significance = policy.map(|p| {
        let n = count as f64;
        let std_error = if n > 1.0 { std_dev / n.sqrt() } else { 0.0 };
        let alpha = p.alpha.unwrap_or(0.05);
        let min_samples = p.min_samples.unwrap_or(3);

        let t_value = if n >= 30.0 { 1.96 } else { 2.0 };
        let ci_lower = mean - t_value * std_error;
        let ci_upper = mean + t_value * std_error;

        let significant = n >= min_samples as f64 && (ci_lower > 0.0 || ci_upper < 0.0);

        Significance {
            test: perfgate_types::SignificanceTest::WelchT,
            significant,
            alpha,
            p_value: None, // Paired t-test p-value could be added here
            ci_lower: Some(ci_lower),
            ci_upper: Some(ci_upper),
            baseline_samples: count,
            current_samples: count,
        }
    });

    Ok(PairedDiffSummary {
        mean,
        median,
        std_dev,
        min,
        max,
        count,
        significance,
    })
}

/// Result of comparing paired statistics, including significance testing.
///
/// # Examples
///
/// ```
/// use perfgate_paired::{compare_paired_stats, PairedComparison};
/// use perfgate_types::{PairedStats, PairedDiffSummary, U64Summary};
///
/// let stats = PairedStats {
///     baseline_wall_ms: U64Summary::new(100, 100, 100 ),
///     current_wall_ms: U64Summary::new(120, 120, 120 ),
///     wall_diff_ms: PairedDiffSummary {
///         mean: 20.0, median: 20.0, std_dev: 3.0,
///         min: 17.0, max: 23.0, count: 10,
///         significance: None,
///     },
///     baseline_max_rss_kb: None,
///     current_max_rss_kb: None,
///     rss_diff_kb: None,
///     baseline_throughput_per_s: None,
///     current_throughput_per_s: None,
///     throughput_diff_per_s: None,
/// };
///
/// let cmp: PairedComparison = compare_paired_stats(&stats);
/// assert!(cmp.is_significant);
/// assert_eq!(cmp.mean_diff_ms, 20.0);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct PairedComparison {
    pub mean_diff_ms: f64,
    pub median_diff_ms: f64,
    pub pct_change: f64,
    pub std_error: f64,
    pub ci_95_lower: f64,
    pub ci_95_upper: f64,
    pub is_significant: bool,
}

/// Compare paired statistics and compute a confidence interval.
///
/// Uses a paired t-test approach: t = 1.96 for n ≥ 30, t = 2.0 otherwise.
/// A result is significant when the 95 % CI does not span zero.
///
/// # Examples
///
/// ```
/// use perfgate_paired::compare_paired_stats;
/// use perfgate_types::{PairedStats, PairedDiffSummary, U64Summary};
///
/// let stats = PairedStats {
///     baseline_wall_ms: U64Summary::new(100, 90, 110 ),
///     current_wall_ms: U64Summary::new(110, 100, 120 ),
///     wall_diff_ms: PairedDiffSummary {
///         mean: 10.0, median: 10.0, std_dev: 2.0,
///         min: 8.0, max: 12.0, count: 5,
///         significance: None,
///     },
///     baseline_max_rss_kb: None,
///     current_max_rss_kb: None,
///     rss_diff_kb: None,
///     baseline_throughput_per_s: None,
///     current_throughput_per_s: None,
///     throughput_diff_per_s: None,
/// };
///
/// let cmp = compare_paired_stats(&stats);
/// assert!(cmp.is_significant);
/// assert_eq!(cmp.mean_diff_ms, 10.0);
/// assert!(cmp.pct_change > 0.0);
/// ```
pub fn compare_paired_stats(stats: &PairedStats) -> PairedComparison {
    let diff = &stats.wall_diff_ms;
    let n = diff.count as f64;
    let std_error = if n > 1.0 {
        diff.std_dev / n.sqrt()
    } else {
        0.0
    };
    let t_value = if n >= 30.0 { 1.96 } else { 2.0 };
    let ci_95_lower = diff.mean - t_value * std_error;
    let ci_95_upper = diff.mean + t_value * std_error;
    let is_significant = ci_95_lower > 0.0 || ci_95_upper < 0.0;
    let baseline_mean = stats.baseline_wall_ms.median as f64;
    let pct_change = if baseline_mean > 0.0 {
        diff.mean / baseline_mean
    } else {
        0.0
    };
    PairedComparison {
        mean_diff_ms: diff.mean,
        median_diff_ms: diff.median,
        pct_change,
        std_error,
        ci_95_lower,
        ci_95_upper,
        is_significant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{PairedSampleHalf, U64Summary};

    fn sample_half(wall_ms: u64) -> PairedSampleHalf {
        PairedSampleHalf {
            wall_ms,
            exit_code: 0,
            timed_out: false,
            max_rss_kb: None,
            stdout: None,
            stderr: None,
        }
    }

    fn sample_half_with_rss(wall_ms: u64, max_rss_kb: u64) -> PairedSampleHalf {
        PairedSampleHalf {
            wall_ms,
            exit_code: 0,
            timed_out: false,
            max_rss_kb: Some(max_rss_kb),
            stdout: None,
            stderr: None,
        }
    }

    fn paired_sample(
        pair_index: u32,
        warmup: bool,
        baseline_wall_ms: u64,
        current_wall_ms: u64,
    ) -> PairedSample {
        PairedSample {
            pair_index,
            warmup,
            baseline: sample_half(baseline_wall_ms),
            current: sample_half(current_wall_ms),
            wall_diff_ms: current_wall_ms as i64 - baseline_wall_ms as i64,
            rss_diff_kb: None,
        }
    }

    fn paired_sample_with_rss(
        pair_index: u32,
        warmup: bool,
        baseline_wall_ms: u64,
        current_wall_ms: u64,
        baseline_rss: u64,
        current_rss: u64,
    ) -> PairedSample {
        PairedSample {
            pair_index,
            warmup,
            baseline: sample_half_with_rss(baseline_wall_ms, baseline_rss),
            current: sample_half_with_rss(current_wall_ms, current_rss),
            wall_diff_ms: current_wall_ms as i64 - baseline_wall_ms as i64,
            rss_diff_kb: Some(current_rss as i64 - baseline_rss as i64),
        }
    }

    #[test]
    fn test_compute_paired_stats_basic() {
        let samples = vec![
            paired_sample(0, false, 100, 90),
            paired_sample(1, false, 110, 100),
            paired_sample(2, false, 120, 110),
        ];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        assert_eq!(stats.baseline_wall_ms.median, 110);
        assert_eq!(stats.baseline_wall_ms.min, 100);
        assert_eq!(stats.baseline_wall_ms.max, 120);

        assert_eq!(stats.current_wall_ms.median, 100);
        assert_eq!(stats.current_wall_ms.min, 90);
        assert_eq!(stats.current_wall_ms.max, 110);

        assert_eq!(stats.wall_diff_ms.mean, -10.0);
        assert_eq!(stats.wall_diff_ms.median, -10.0);
        assert_eq!(stats.wall_diff_ms.std_dev, 0.0);
        assert_eq!(stats.wall_diff_ms.min, -10.0);
        assert_eq!(stats.wall_diff_ms.max, -10.0);
        assert_eq!(stats.wall_diff_ms.count, 3);
    }

    #[test]
    fn test_compute_paired_stats_with_variance() {
        let samples = vec![
            paired_sample(0, false, 100, 110),
            paired_sample(1, false, 100, 120),
            paired_sample(2, false, 100, 130),
        ];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        assert_eq!(stats.wall_diff_ms.mean, 20.0);
        assert_eq!(stats.wall_diff_ms.median, 20.0);
        assert_eq!(stats.wall_diff_ms.min, 10.0);
        assert_eq!(stats.wall_diff_ms.max, 30.0);
        assert_eq!(stats.wall_diff_ms.count, 3);

        let expected_std_dev = (200.0_f64 / 3.0).sqrt();
        assert!(
            (stats.wall_diff_ms.std_dev - expected_std_dev).abs() < 0.001,
            "std_dev should be ~8.165, got {}",
            stats.wall_diff_ms.std_dev
        );
    }

    #[test]
    fn test_compute_paired_stats_filters_warmup() {
        let samples = vec![
            paired_sample(0, true, 1000, 2000),
            paired_sample(1, true, 1000, 2000),
            paired_sample(2, false, 100, 110),
            paired_sample(3, false, 100, 120),
        ];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        assert_eq!(stats.wall_diff_ms.count, 2);
        assert_eq!(stats.baseline_wall_ms.median, 100);
        assert_eq!(stats.current_wall_ms.median, 115);
    }

    #[test]
    fn test_compute_paired_stats_empty_after_warmup_filter() {
        let samples = vec![
            paired_sample(0, true, 100, 110),
            paired_sample(1, true, 100, 120),
        ];

        let result = compute_paired_stats(&samples, None, None);
        assert!(result.is_err(), "should error with no measured samples");
        assert!(matches!(result.unwrap_err(), PairedError::NoSamples));
    }

    #[test]
    fn test_compute_paired_stats_empty_samples() {
        let samples: Vec<PairedSample> = vec![];

        let result = compute_paired_stats(&samples, None, None);
        assert!(result.is_err(), "should error with empty samples");
        assert!(matches!(result.unwrap_err(), PairedError::NoSamples));
    }

    #[test]
    fn test_compute_paired_stats_single_sample() {
        let samples = vec![paired_sample(0, false, 100, 150)];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        assert_eq!(stats.baseline_wall_ms.median, 100);
        assert_eq!(stats.baseline_wall_ms.min, 100);
        assert_eq!(stats.baseline_wall_ms.max, 100);

        assert_eq!(stats.current_wall_ms.median, 150);

        assert_eq!(stats.wall_diff_ms.mean, 50.0);
        assert_eq!(stats.wall_diff_ms.median, 50.0);
        assert_eq!(stats.wall_diff_ms.std_dev, 0.0);
        assert_eq!(stats.wall_diff_ms.count, 1);
    }

    #[test]
    fn test_compute_paired_stats_with_rss() {
        let samples = vec![
            paired_sample_with_rss(0, false, 100, 110, 1000, 1100),
            paired_sample_with_rss(1, false, 100, 120, 1000, 1200),
            paired_sample_with_rss(2, false, 100, 130, 1000, 1300),
        ];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        let baseline_rss = stats.baseline_max_rss_kb.expect("should have baseline RSS");
        assert_eq!(baseline_rss.median, 1000);

        let current_rss = stats.current_max_rss_kb.expect("should have current RSS");
        assert_eq!(current_rss.median, 1200);

        let rss_diff = stats.rss_diff_kb.expect("should have RSS diff");
        assert_eq!(rss_diff.mean, 200.0);
        assert_eq!(rss_diff.count, 3);
    }

    #[test]
    fn test_compute_paired_stats_with_work_units() {
        let samples = vec![
            paired_sample(0, false, 1000, 500),
            paired_sample(1, false, 1000, 500),
        ];

        let stats = compute_paired_stats(&samples, Some(100), None).expect("should compute stats");

        let baseline_thr = stats
            .baseline_throughput_per_s
            .expect("should have baseline throughput");
        assert_eq!(baseline_thr.median, 100.0);

        let current_thr = stats
            .current_throughput_per_s
            .expect("should have current throughput");
        assert_eq!(current_thr.median, 200.0);

        let thr_diff = stats
            .throughput_diff_per_s
            .expect("should have throughput diff");
        assert_eq!(thr_diff.mean, 100.0);
    }

    #[test]
    fn test_compute_paired_stats_no_throughput_without_work_units() {
        let samples = vec![paired_sample(0, false, 100, 110)];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        assert!(stats.baseline_throughput_per_s.is_none());
        assert!(stats.current_throughput_per_s.is_none());
        assert!(stats.throughput_diff_per_s.is_none());
    }

    #[test]
    fn test_compute_paired_stats_negative_diffs() {
        let samples = vec![
            paired_sample(0, false, 200, 100),
            paired_sample(1, false, 200, 100),
        ];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        assert_eq!(stats.wall_diff_ms.mean, -100.0);
        assert_eq!(stats.wall_diff_ms.median, -100.0);
    }

    #[test]
    fn test_compute_paired_stats_even_count_median() {
        let samples = vec![
            paired_sample(0, false, 100, 110),
            paired_sample(1, false, 100, 120),
            paired_sample(2, false, 100, 130),
            paired_sample(3, false, 100, 140),
        ];

        let stats = compute_paired_stats(&samples, None, None).expect("should compute stats");

        assert_eq!(stats.wall_diff_ms.median, 25.0);
        assert_eq!(stats.wall_diff_ms.mean, 25.0);
    }

    #[test]
    fn test_compare_paired_stats_basic() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary::new(100, 90, 110),
            current_wall_ms: U64Summary::new(110, 100, 120),
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 5.0,
                min: 5.0,
                max: 15.0,
                count: 10,
                significance: None,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        assert_eq!(comparison.mean_diff_ms, 10.0);
        assert_eq!(comparison.median_diff_ms, 10.0);
        assert_eq!(comparison.pct_change, 0.1);

        let expected_std_error = 5.0 / (10.0_f64).sqrt();
        assert!(
            (comparison.std_error - expected_std_error).abs() < 0.01,
            "std_error should be ~1.58, got {}",
            comparison.std_error
        );
    }

    #[test]
    fn test_compare_paired_stats_ci_calculation() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary::new(100, 100, 100),
            current_wall_ms: U64Summary::new(110, 110, 110),
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 2.0,
                min: 8.0,
                max: 12.0,
                count: 5,
                significance: None,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        let expected_std_error = 2.0 / (5.0_f64).sqrt();
        let expected_ci_lower = 10.0 - 2.0 * expected_std_error;
        let expected_ci_upper = 10.0 + 2.0 * expected_std_error;

        assert!(
            (comparison.ci_95_lower - expected_ci_lower).abs() < 0.01,
            "ci_95_lower should be ~{}, got {}",
            expected_ci_lower,
            comparison.ci_95_lower
        );
        assert!(
            (comparison.ci_95_upper - expected_ci_upper).abs() < 0.01,
            "ci_95_upper should be ~{}, got {}",
            expected_ci_upper,
            comparison.ci_95_upper
        );

        assert!(
            comparison.is_significant,
            "result should be significant when CI doesn't span zero"
        );
    }

    #[test]
    fn test_compare_paired_stats_large_sample_t_value() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary::new(100, 100, 100),
            current_wall_ms: U64Summary::new(110, 110, 110),
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 5.0,
                min: 0.0,
                max: 20.0,
                count: 30,
                significance: None,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        let expected_std_error = 5.0 / (30.0_f64).sqrt();
        let expected_ci_lower = 10.0 - 1.96 * expected_std_error;

        assert!(
            (comparison.ci_95_lower - expected_ci_lower).abs() < 0.01,
            "ci_95_lower with n>=30 should use t_value=1.96"
        );
    }

    #[test]
    fn test_compare_paired_stats_not_significant() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary::new(100, 100, 100),
            current_wall_ms: U64Summary::new(101, 101, 101),
            wall_diff_ms: PairedDiffSummary {
                mean: 1.0,
                median: 1.0,
                std_dev: 10.0,
                min: -15.0,
                max: 15.0,
                count: 5,
                significance: None,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        assert!(
            !comparison.is_significant,
            "result should not be significant when CI spans zero: [{}, {}]",
            comparison.ci_95_lower, comparison.ci_95_upper
        );
        assert!(
            comparison.ci_95_lower < 0.0 && comparison.ci_95_upper > 0.0,
            "CI should span zero"
        );
    }

    #[test]
    fn test_compare_paired_stats_single_sample() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary::new(100, 100, 100),
            current_wall_ms: U64Summary::new(110, 110, 110),
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 0.0,
                min: 10.0,
                max: 10.0,
                count: 1,
                significance: None,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        assert_eq!(comparison.std_error, 0.0);
        assert_eq!(comparison.ci_95_lower, 10.0);
        assert_eq!(comparison.ci_95_upper, 10.0);
    }

    #[test]
    fn test_compare_paired_stats_zero_baseline() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary::new(0, 0, 0),
            current_wall_ms: U64Summary::new(10, 10, 10),
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 0.0,
                min: 10.0,
                max: 10.0,
                count: 1,
                significance: None,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        assert_eq!(
            comparison.pct_change, 0.0,
            "pct_change should be 0 when baseline is 0"
        );
    }

    #[test]
    fn test_compare_paired_stats_negative_improvement() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary::new(100, 100, 100),
            current_wall_ms: U64Summary::new(80, 80, 80),
            wall_diff_ms: PairedDiffSummary {
                mean: -20.0,
                median: -20.0,
                std_dev: 2.0,
                min: -22.0,
                max: -18.0,
                count: 5,
                significance: None,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        assert_eq!(comparison.mean_diff_ms, -20.0);
        assert_eq!(comparison.pct_change, -0.2);
        assert!(
            comparison.is_significant,
            "significant improvement should be detected"
        );
        assert!(
            comparison.ci_95_upper < 0.0,
            "CI upper bound should be negative for improvement"
        );
    }

    #[test]
    fn test_summarize_paired_diffs_empty() {
        let result = summarize_paired_diffs(&[], None);
        assert!(matches!(result, Err(PairedError::NoSamples)));
    }

    #[test]
    fn test_summarize_paired_diffs_single() {
        let summary = summarize_paired_diffs(&[5.0], None).unwrap();
        assert_eq!(summary.mean, 5.0);
        assert_eq!(summary.median, 5.0);
        assert_eq!(summary.std_dev, 0.0);
        assert_eq!(summary.min, 5.0);
        assert_eq!(summary.max, 5.0);
        assert_eq!(summary.count, 1);
    }

    #[test]
    fn test_summarize_paired_diffs_zero_variance() {
        let summary = summarize_paired_diffs(&[10.0, 10.0, 10.0, 10.0], None).unwrap();
        assert_eq!(summary.mean, 10.0);
        assert_eq!(summary.std_dev, 0.0);
        assert_eq!(summary.count, 4);
    }

    #[test]
    fn test_summarize_paired_diffs_large_sample() {
        let diffs: Vec<f64> = (0..1000).map(|i| i as f64).collect();
        let summary = summarize_paired_diffs(&diffs, None).unwrap();

        assert_eq!(summary.count, 1000);
        assert_eq!(summary.min, 0.0);
        assert_eq!(summary.max, 999.0);

        let expected_mean = (0.0 + 999.0) / 2.0;
        assert!((summary.mean - expected_mean).abs() < 0.1);
    }

    mod edge_cases {
        use super::*;

        #[test]
        fn test_ci_bounds_with_zero_std_dev() {
            let stats = PairedStats {
                baseline_wall_ms: U64Summary::new(100, 100, 100),
                current_wall_ms: U64Summary::new(110, 110, 110),
                wall_diff_ms: PairedDiffSummary {
                    mean: 10.0,
                    median: 10.0,
                    std_dev: 0.0,
                    min: 10.0,
                    max: 10.0,
                    count: 10,
                    significance: None,
                },
                baseline_max_rss_kb: None,
                current_max_rss_kb: None,
                rss_diff_kb: None,
                baseline_throughput_per_s: None,
                current_throughput_per_s: None,
                throughput_diff_per_s: None,
            };

            let comparison = compare_paired_stats(&stats);
            assert_eq!(comparison.std_error, 0.0);
            assert_eq!(comparison.ci_95_lower, 10.0);
            assert_eq!(comparison.ci_95_upper, 10.0);
            assert!(comparison.is_significant);
        }

        #[test]
        fn test_large_positive_diff() {
            let stats = PairedStats {
                baseline_wall_ms: U64Summary::new(100, 100, 100),
                current_wall_ms: U64Summary::new(100000, 100000, 100000),
                wall_diff_ms: PairedDiffSummary {
                    mean: 99900.0,
                    median: 99900.0,
                    std_dev: 100.0,
                    min: 99800.0,
                    max: 100000.0,
                    count: 50,
                    significance: None,
                },
                baseline_max_rss_kb: None,
                current_max_rss_kb: None,
                rss_diff_kb: None,
                baseline_throughput_per_s: None,
                current_throughput_per_s: None,
                throughput_diff_per_s: None,
            };

            let comparison = compare_paired_stats(&stats);
            assert_eq!(comparison.mean_diff_ms, 99900.0);
            assert!((comparison.pct_change - 999.0).abs() < 0.01);
            assert!(comparison.is_significant);
        }

        #[test]
        fn test_very_small_diffs() {
            let stats = PairedStats {
                baseline_wall_ms: U64Summary::new(100000, 100000, 100000),
                current_wall_ms: U64Summary::new(100001, 100001, 100001),
                wall_diff_ms: PairedDiffSummary {
                    mean: 1.0,
                    median: 1.0,
                    std_dev: 0.5,
                    min: 0.0,
                    max: 2.0,
                    count: 30,
                    significance: None,
                },
                baseline_max_rss_kb: None,
                current_max_rss_kb: None,
                rss_diff_kb: None,
                baseline_throughput_per_s: None,
                current_throughput_per_s: None,
                throughput_diff_per_s: None,
            };

            let comparison = compare_paired_stats(&stats);
            assert!((comparison.pct_change - 0.00001).abs() < 0.000001);
        }
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use perfgate_types::{PairedSample, PairedSampleHalf};
    use proptest::prelude::*;

    fn finite_f64_strategy() -> impl Strategy<Value = f64> {
        -1e100f64..1e100f64
    }

    fn make_paired_samples(baseline: &[u64], current: &[u64]) -> Vec<PairedSample> {
        baseline
            .iter()
            .zip(current.iter())
            .enumerate()
            .map(|(i, (&b, &c))| PairedSample {
                pair_index: i as u32,
                warmup: false,
                baseline: PairedSampleHalf {
                    wall_ms: b,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: None,
                    stdout: None,
                    stderr: None,
                },
                current: PairedSampleHalf {
                    wall_ms: c,
                    exit_code: 0,
                    timed_out: false,
                    max_rss_kb: None,
                    stdout: None,
                    stderr: None,
                },
                wall_diff_ms: c as i64 - b as i64,
                rss_diff_kb: None,
            })
            .collect()
    }

    proptest! {
        #[test]
        fn prop_summarize_paired_diffs_count_matches(diffs in prop::collection::vec(finite_f64_strategy(), 1..100)) {
            let summary = summarize_paired_diffs(&diffs, None).unwrap();
            prop_assert_eq!(summary.count, diffs.len() as u32);
        }

        #[test]
        fn prop_summarize_paired_diffs_mean_correct(diffs in prop::collection::vec(finite_f64_strategy(), 1..100)) {
            let summary = summarize_paired_diffs(&diffs, None).unwrap();
            let expected_mean: f64 = diffs.iter().sum::<f64>() / diffs.len() as f64;
            prop_assert!((summary.mean - expected_mean).abs() < 1e-10 || expected_mean.abs() < 1e-10);
        }

        #[test]
        fn prop_summarize_paired_diffs_min_max_bounds(diffs in prop::collection::vec(finite_f64_strategy(), 1..100)) {
            let summary = summarize_paired_diffs(&diffs, None).unwrap();
            let expected_min = diffs.iter().cloned().fold(f64::INFINITY, f64::min);
            let expected_max = diffs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

            if expected_min.is_finite() {
                prop_assert!((summary.min - expected_min).abs() < 1e-10);
            }
            if expected_max.is_finite() {
                prop_assert!((summary.max - expected_max).abs() < 1e-10);
            }
        }

        #[test]
        fn prop_summarize_paired_diffs_ordering(diffs in prop::collection::vec(finite_f64_strategy(), 1..100)) {
            let summary = summarize_paired_diffs(&diffs, None).unwrap();
            if summary.min.is_finite() && summary.median.is_finite() && summary.max.is_finite() {
                prop_assert!(summary.min <= summary.median);
                prop_assert!(summary.median <= summary.max);
            }
        }

        #[test]
        fn prop_std_dev_non_negative(diffs in prop::collection::vec(finite_f64_strategy(), 1..100)) {
            let summary = summarize_paired_diffs(&diffs, None).unwrap();
            prop_assert!(summary.std_dev >= 0.0 || !summary.std_dev.is_finite());
        }

        #[test]
        fn prop_ci_contains_mean(
            mean in -1000.0f64..1000.0,
            std_dev in 0.1f64..100.0,
            count in 2u32..100
        ) {
            let stats = PairedStats {
                baseline_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                current_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                wall_diff_ms: PairedDiffSummary {
                    mean,
                    median: mean,
                    std_dev,
                    min: mean - std_dev,
                    max: mean + std_dev,
                    count,
                    significance: None,
                    },
                baseline_max_rss_kb: None,
                current_max_rss_kb: None,
                rss_diff_kb: None,
                baseline_throughput_per_s: None,
                current_throughput_per_s: None,
                throughput_diff_per_s: None,
            };

            let comparison = compare_paired_stats(&stats);

            prop_assert!(comparison.ci_95_lower <= mean);
            prop_assert!(comparison.ci_95_upper >= mean);
        }

        #[test]
        fn prop_ci_width_decreases_with_sample_size(
            mean in 0.0f64..100.0,
            std_dev in 1.0f64..10.0,
        ) {
            let make_comparison = |count: u32| {
                let stats = PairedStats {
                    baseline_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                    current_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                    wall_diff_ms: PairedDiffSummary {
                        mean,
                        median: mean,
                        std_dev,
                        min: mean - std_dev,
                        max: mean + std_dev,
                        count,
                        significance: None,
                        },
                    baseline_max_rss_kb: None,
                    current_max_rss_kb: None,
                    rss_diff_kb: None,
                    baseline_throughput_per_s: None,
                    current_throughput_per_s: None,
                    throughput_diff_per_s: None,
                };
                compare_paired_stats(&stats)
            };

            let small = make_comparison(10);
            let large = make_comparison(100);

            let width_small = small.ci_95_upper - small.ci_95_lower;
            let width_large = large.ci_95_upper - large.ci_95_lower;

            prop_assert!(width_large < width_small, "CI should narrow with more samples");
        }

        #[test]
        fn prop_zero_variance_zero_std_error(
            mean in -1000.0f64..1000.0,
            count in 2u32..100
        ) {
            let stats = PairedStats {
                baseline_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                current_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                wall_diff_ms: PairedDiffSummary {
                    mean,
                    median: mean,
                    std_dev: 0.0,
                    min: mean,
                    max: mean,
                    count,
                    significance: None,
                    },
                baseline_max_rss_kb: None,
                current_max_rss_kb: None,
                rss_diff_kb: None,
                baseline_throughput_per_s: None,
                current_throughput_per_s: None,
                throughput_diff_per_s: None,
            };

            let comparison = compare_paired_stats(&stats);

            prop_assert_eq!(comparison.std_error, 0.0);
            prop_assert_eq!(comparison.ci_95_lower, mean);
            prop_assert_eq!(comparison.ci_95_upper, mean);
        }

        #[test]
        fn prop_significance_deterministic(
            mean in -100.0f64..100.0,
            std_dev in 0.0f64..50.0,
            count in 1u32..50
        ) {
            let stats = PairedStats {
                baseline_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                current_wall_ms: perfgate_types::U64Summary::new(100, 100, 100 ),
                wall_diff_ms: PairedDiffSummary {
                    mean,
                    median: mean,
                    std_dev,
                    min: mean - std_dev,
                    max: mean + std_dev,
                    count,
                    significance: None,
                    },
                baseline_max_rss_kb: None,
                current_max_rss_kb: None,
                rss_diff_kb: None,
                baseline_throughput_per_s: None,
                current_throughput_per_s: None,
                throughput_diff_per_s: None,
            };

            let comparison = compare_paired_stats(&stats);

            let is_significant = comparison.ci_95_lower > 0.0 || comparison.ci_95_upper < 0.0;
            prop_assert_eq!(comparison.is_significant, is_significant);
        }

        #[test]
        fn prop_paired_stats_deterministic(
            baseline in prop::collection::vec(1u64..10000u64, 5..50),
            current in prop::collection::vec(1u64..10000u64, 5..50),
        ) {
            let len = baseline.len().min(current.len());
            let samples = make_paired_samples(&baseline[..len], &current[..len]);
            let r1 = compute_paired_stats(&samples, None, None);
            let r2 = compute_paired_stats(&samples, None, None);
            match (r1, r2) {
                (Ok(s1), Ok(s2)) => {
                    prop_assert_eq!(s1.wall_diff_ms.mean, s2.wall_diff_ms.mean);
                    prop_assert_eq!(s1.wall_diff_ms.median, s2.wall_diff_ms.median);
                    prop_assert_eq!(s1.wall_diff_ms.std_dev, s2.wall_diff_ms.std_dev);
                    prop_assert_eq!(s1.wall_diff_ms.count, s2.wall_diff_ms.count);
                }
                (Err(_), Err(_)) => {}
                _ => prop_assert!(false, "both calls must produce the same result"),
            }
        }

        #[test]
        fn prop_ci_contains_mean_diff_from_samples(
            baseline in prop::collection::vec(1u64..10000u64, 5..50),
            current in prop::collection::vec(1u64..10000u64, 5..50),
        ) {
            let len = baseline.len().min(current.len());
            let samples = make_paired_samples(&baseline[..len], &current[..len]);
            if let Ok(stats) = compute_paired_stats(&samples, None, None) {
                let cmp = compare_paired_stats(&stats);
                prop_assert!(
                    cmp.ci_95_lower <= cmp.mean_diff_ms,
                    "CI lower {} must be <= mean {}",
                    cmp.ci_95_lower, cmp.mean_diff_ms
                );
                prop_assert!(
                    cmp.ci_95_upper >= cmp.mean_diff_ms,
                    "CI upper {} must be >= mean {}",
                    cmp.ci_95_upper, cmp.mean_diff_ms
                );
            }
        }

        #[test]
        fn prop_reversing_negates_mean_diff(
            baseline in prop::collection::vec(1u64..10000u64, 5..50),
            current in prop::collection::vec(1u64..10000u64, 5..50),
        ) {
            let len = baseline.len().min(current.len());
            let fwd = make_paired_samples(&baseline[..len], &current[..len]);
            let rev = make_paired_samples(&current[..len], &baseline[..len]);
            if let (Ok(fwd_stats), Ok(rev_stats)) =
                (compute_paired_stats(&fwd, None, None), compute_paired_stats(&rev, None, None))
            {
                let fwd_cmp = compare_paired_stats(&fwd_stats);
                let rev_cmp = compare_paired_stats(&rev_stats);
                prop_assert!(
                    (fwd_cmp.mean_diff_ms + rev_cmp.mean_diff_ms).abs() < 1e-10,
                    "reversing must negate mean diff: {} vs {}",
                    fwd_cmp.mean_diff_ms, rev_cmp.mean_diff_ms
                );
            }
        }

        /// Full pipeline determinism: compute + compare yields identical results.
        #[test]
        fn prop_full_pipeline_determinism(
            baseline in prop::collection::vec(1u64..10000u64, 5..50),
            current in prop::collection::vec(1u64..10000u64, 5..50),
        ) {
            let len = baseline.len().min(current.len());
            let samples = make_paired_samples(&baseline[..len], &current[..len]);
            let stats1 = compute_paired_stats(&samples, None, None).unwrap();
            let stats2 = compute_paired_stats(&samples, None, None).unwrap();
            let cmp1 = compare_paired_stats(&stats1);
            let cmp2 = compare_paired_stats(&stats2);
            prop_assert_eq!(cmp1, cmp2, "identical inputs must produce identical comparisons");
        }

        /// Sample count in output matches number of non-warmup input samples.
        #[test]
        fn prop_sample_count_preserved(
            baseline in prop::collection::vec(1u64..10000u64, 2..50),
            current in prop::collection::vec(1u64..10000u64, 2..50),
        ) {
            let len = baseline.len().min(current.len());
            let samples = make_paired_samples(&baseline[..len], &current[..len]);
            let non_warmup = samples.iter().filter(|s| !s.warmup).count() as u32;
            let stats = compute_paired_stats(&samples, None, None).unwrap();
            prop_assert_eq!(
                stats.wall_diff_ms.count, non_warmup,
                "output count {} must equal non-warmup input count {}",
                stats.wall_diff_ms.count, non_warmup
            );
        }

        /// Swapping baseline/current flips the sign of pct_change.
        #[test]
        fn prop_diff_symmetry_direction_flips(
            baseline in prop::collection::vec(1u64..10000u64, 5..50),
            current in prop::collection::vec(1u64..10000u64, 5..50),
        ) {
            let len = baseline.len().min(current.len());
            let fwd = make_paired_samples(&baseline[..len], &current[..len]);
            let rev = make_paired_samples(&current[..len], &baseline[..len]);
            if let (Ok(fwd_stats), Ok(rev_stats)) =
                (compute_paired_stats(&fwd, None, None), compute_paired_stats(&rev, None, None))
            {
                let fwd_cmp = compare_paired_stats(&fwd_stats);
                let rev_cmp = compare_paired_stats(&rev_stats);
                // If forward shows regression (positive diff), reverse must show improvement.
                if fwd_cmp.mean_diff_ms > 0.0 {
                    prop_assert!(
                        rev_cmp.mean_diff_ms < 0.0,
                        "if forward is regression ({:.2}), reverse must be improvement ({:.2})",
                        fwd_cmp.mean_diff_ms, rev_cmp.mean_diff_ms
                    );
                } else if fwd_cmp.mean_diff_ms < 0.0 {
                    prop_assert!(
                        rev_cmp.mean_diff_ms > 0.0,
                        "if forward is improvement ({:.2}), reverse must be regression ({:.2})",
                        fwd_cmp.mean_diff_ms, rev_cmp.mean_diff_ms
                    );
                }
                // Median must also flip.
                prop_assert!(
                    (fwd_cmp.median_diff_ms + rev_cmp.median_diff_ms).abs() < 1e-10,
                    "median diff must negate: {:.2} vs {:.2}",
                    fwd_cmp.median_diff_ms, rev_cmp.median_diff_ms
                );
            }
        }

        /// Mean difference is bounded by min and max of individual pair diffs.
        #[test]
        fn prop_mean_bounded_by_min_max(
            baseline in prop::collection::vec(1u64..10000u64, 2..50),
            current in prop::collection::vec(1u64..10000u64, 2..50),
        ) {
            let len = baseline.len().min(current.len());
            let samples = make_paired_samples(&baseline[..len], &current[..len]);
            if let Ok(stats) = compute_paired_stats(&samples, None, None) {
                let diff = &stats.wall_diff_ms;
                prop_assert!(
                    diff.mean >= diff.min,
                    "mean {:.2} must be >= min {:.2}",
                    diff.mean, diff.min
                );
                prop_assert!(
                    diff.mean <= diff.max,
                    "mean {:.2} must be <= max {:.2}",
                    diff.mean, diff.max
                );
            }
        }
    }
}
