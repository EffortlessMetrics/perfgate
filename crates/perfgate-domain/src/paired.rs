//! Paired statistics computation for perfgate.

use crate::{DomainError, summarize_f64, summarize_u64};
use perfgate_types::{PairedDiffSummary, PairedSample, PairedStats};

pub fn compute_paired_stats(
    samples: &[PairedSample],
    work_units: Option<u64>,
) -> Result<PairedStats, DomainError> {
    let measured: Vec<&PairedSample> = samples.iter().filter(|s| !s.warmup).collect();
    if measured.is_empty() {
        return Err(DomainError::NoSamples);
    }

    let baseline_wall: Vec<u64> = measured.iter().map(|s| s.baseline.wall_ms).collect();
    let current_wall: Vec<u64> = measured.iter().map(|s| s.current.wall_ms).collect();
    let wall_diffs: Vec<f64> = measured.iter().map(|s| s.wall_diff_ms as f64).collect();

    let baseline_wall_ms = summarize_u64(&baseline_wall)?;
    let current_wall_ms = summarize_u64(&current_wall)?;
    let wall_diff_ms = summarize_paired_diffs(&wall_diffs)?;

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
        Some(summarize_u64(&baseline_rss)?)
    };
    let current_max_rss_kb = if current_rss.is_empty() {
        None
    } else {
        Some(summarize_u64(&current_rss)?)
    };
    let rss_diff_kb = if rss_diffs.is_empty() {
        None
    } else {
        Some(summarize_paired_diffs(&rss_diffs)?)
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
                    Some(summarize_f64(&baseline_thr)?),
                    Some(summarize_f64(&current_thr)?),
                    Some(summarize_paired_diffs(&thr_diffs)?),
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

fn summarize_paired_diffs(diffs: &[f64]) -> Result<PairedDiffSummary, DomainError> {
    if diffs.is_empty() {
        return Err(DomainError::NoSamples);
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
    Ok(PairedDiffSummary {
        mean,
        median,
        std_dev,
        min,
        max,
        count,
    })
}

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

    /// Helper to create a sample half with minimal required data.
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

    /// Helper to create a sample half with RSS data.
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

    /// Helper to create a paired sample.
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

    /// Helper to create a paired sample with RSS data.
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

    // ======================================================================
    // compute_paired_stats tests
    // ======================================================================

    #[test]
    fn test_compute_paired_stats_basic() {
        // baseline: 100, 110, 120 -> median=110, min=100, max=120
        // current: 90, 100, 110 -> median=100, min=90, max=110
        // diffs: -10, -10, -10 -> mean=-10, median=-10, std_dev=0
        let samples = vec![
            paired_sample(0, false, 100, 90),
            paired_sample(1, false, 110, 100),
            paired_sample(2, false, 120, 110),
        ];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

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
        // diffs: 10, 20, 30 -> mean=20, median=20
        // variance = ((10-20)^2 + (20-20)^2 + (30-20)^2) / 3 = (100 + 0 + 100) / 3 = 200/3
        // std_dev = sqrt(200/3) ~ 8.165
        let samples = vec![
            paired_sample(0, false, 100, 110), // diff = 10
            paired_sample(1, false, 100, 120), // diff = 20
            paired_sample(2, false, 100, 130), // diff = 30
        ];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

        assert_eq!(stats.wall_diff_ms.mean, 20.0);
        assert_eq!(stats.wall_diff_ms.median, 20.0);
        assert_eq!(stats.wall_diff_ms.min, 10.0);
        assert_eq!(stats.wall_diff_ms.max, 30.0);
        assert_eq!(stats.wall_diff_ms.count, 3);

        // Check std_dev is approximately correct (sqrt(200/3) ~ 8.165)
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
            paired_sample(0, true, 1000, 2000), // warmup, should be excluded
            paired_sample(1, true, 1000, 2000), // warmup, should be excluded
            paired_sample(2, false, 100, 110),  // measured
            paired_sample(3, false, 100, 120),  // measured
        ];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

        // Only measured samples (indices 2, 3) should be included
        assert_eq!(stats.wall_diff_ms.count, 2);
        assert_eq!(stats.baseline_wall_ms.median, 100);
        assert_eq!(stats.current_wall_ms.median, 115); // (110 + 120) / 2 = 115
    }

    #[test]
    fn test_compute_paired_stats_empty_after_warmup_filter() {
        // All samples are warmup
        let samples = vec![
            paired_sample(0, true, 100, 110),
            paired_sample(1, true, 100, 120),
        ];

        let result = compute_paired_stats(&samples, None);
        assert!(result.is_err(), "should error with no measured samples");
        assert!(matches!(result.unwrap_err(), DomainError::NoSamples));
    }

    #[test]
    fn test_compute_paired_stats_empty_samples() {
        let samples: Vec<PairedSample> = vec![];

        let result = compute_paired_stats(&samples, None);
        assert!(result.is_err(), "should error with empty samples");
        assert!(matches!(result.unwrap_err(), DomainError::NoSamples));
    }

    #[test]
    fn test_compute_paired_stats_single_sample() {
        let samples = vec![paired_sample(0, false, 100, 150)];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

        // Single sample: median = value, min = max = value
        assert_eq!(stats.baseline_wall_ms.median, 100);
        assert_eq!(stats.baseline_wall_ms.min, 100);
        assert_eq!(stats.baseline_wall_ms.max, 100);

        assert_eq!(stats.current_wall_ms.median, 150);

        assert_eq!(stats.wall_diff_ms.mean, 50.0);
        assert_eq!(stats.wall_diff_ms.median, 50.0);
        assert_eq!(stats.wall_diff_ms.std_dev, 0.0); // no variance with 1 sample
        assert_eq!(stats.wall_diff_ms.count, 1);
    }

    #[test]
    fn test_compute_paired_stats_with_rss() {
        let samples = vec![
            paired_sample_with_rss(0, false, 100, 110, 1000, 1100),
            paired_sample_with_rss(1, false, 100, 120, 1000, 1200),
            paired_sample_with_rss(2, false, 100, 130, 1000, 1300),
        ];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

        // Verify RSS stats are present
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
        // baseline: 1000ms = 1s, work=100 -> throughput = 100/s
        // current: 500ms = 0.5s, work=100 -> throughput = 200/s
        let samples = vec![
            paired_sample(0, false, 1000, 500),
            paired_sample(1, false, 1000, 500),
        ];

        let stats = compute_paired_stats(&samples, Some(100)).expect("should compute stats");

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
        assert_eq!(thr_diff.mean, 100.0); // current - baseline = 200 - 100
    }

    #[test]
    fn test_compute_paired_stats_no_throughput_without_work_units() {
        let samples = vec![paired_sample(0, false, 100, 110)];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

        assert!(stats.baseline_throughput_per_s.is_none());
        assert!(stats.current_throughput_per_s.is_none());
        assert!(stats.throughput_diff_per_s.is_none());
    }

    #[test]
    fn test_compute_paired_stats_negative_diffs() {
        // Current is faster than baseline
        let samples = vec![
            paired_sample(0, false, 200, 100), // diff = -100
            paired_sample(1, false, 200, 100), // diff = -100
        ];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

        assert_eq!(stats.wall_diff_ms.mean, -100.0);
        assert_eq!(stats.wall_diff_ms.median, -100.0);
    }

    #[test]
    fn test_compute_paired_stats_even_count_median() {
        // Even number of samples: median is average of two middle values
        // diffs: 10, 20, 30, 40 -> median = (20 + 30) / 2 = 25
        let samples = vec![
            paired_sample(0, false, 100, 110), // diff = 10
            paired_sample(1, false, 100, 120), // diff = 20
            paired_sample(2, false, 100, 130), // diff = 30
            paired_sample(3, false, 100, 140), // diff = 40
        ];

        let stats = compute_paired_stats(&samples, None).expect("should compute stats");

        assert_eq!(stats.wall_diff_ms.median, 25.0);
        assert_eq!(stats.wall_diff_ms.mean, 25.0);
    }

    // ======================================================================
    // compare_paired_stats tests
    // ======================================================================

    #[test]
    fn test_compare_paired_stats_basic() {
        let stats = PairedStats {
            baseline_wall_ms: U64Summary {
                median: 100,
                min: 90,
                max: 110,
            },
            current_wall_ms: U64Summary {
                median: 110,
                min: 100,
                max: 120,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 5.0,
                min: 5.0,
                max: 15.0,
                count: 10,
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
        assert_eq!(comparison.pct_change, 0.1); // 10 / 100 = 10%

        // std_error = std_dev / sqrt(n) = 5 / sqrt(10) ~ 1.58
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
            baseline_wall_ms: U64Summary {
                median: 100,
                min: 100,
                max: 100,
            },
            current_wall_ms: U64Summary {
                median: 110,
                min: 110,
                max: 110,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 2.0,
                min: 8.0,
                max: 12.0,
                count: 5, // < 30, so t_value = 2.0
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        // std_error = 2.0 / sqrt(5) ~ 0.894
        // t_value = 2.0 (since n < 30)
        // ci_lower = 10.0 - 2.0 * 0.894 ~ 8.21
        // ci_upper = 10.0 + 2.0 * 0.894 ~ 11.79
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

        // CI doesn't span zero, so it's significant
        assert!(
            comparison.is_significant,
            "result should be significant when CI doesn't span zero"
        );
    }

    #[test]
    fn test_compare_paired_stats_large_sample_t_value() {
        // n >= 30 uses t_value = 1.96
        let stats = PairedStats {
            baseline_wall_ms: U64Summary {
                median: 100,
                min: 100,
                max: 100,
            },
            current_wall_ms: U64Summary {
                median: 110,
                min: 110,
                max: 110,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 5.0,
                min: 0.0,
                max: 20.0,
                count: 30, // >= 30, so t_value = 1.96
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        // std_error = 5.0 / sqrt(30) ~ 0.913
        // t_value = 1.96
        // ci_lower = 10.0 - 1.96 * 0.913 ~ 8.21
        let expected_std_error = 5.0 / (30.0_f64).sqrt();
        let expected_ci_lower = 10.0 - 1.96 * expected_std_error;

        assert!(
            (comparison.ci_95_lower - expected_ci_lower).abs() < 0.01,
            "ci_95_lower with n>=30 should use t_value=1.96"
        );
    }

    #[test]
    fn test_compare_paired_stats_not_significant() {
        // CI spans zero = not significant
        let stats = PairedStats {
            baseline_wall_ms: U64Summary {
                median: 100,
                min: 100,
                max: 100,
            },
            current_wall_ms: U64Summary {
                median: 101,
                min: 101,
                max: 101,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: 1.0, // small mean
                median: 1.0,
                std_dev: 10.0, // large std_dev
                min: -15.0,
                max: 15.0,
                count: 5,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        // With high variance and small mean, CI will span zero
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
        // Single sample: std_error = 0 (no variance estimate possible)
        let stats = PairedStats {
            baseline_wall_ms: U64Summary {
                median: 100,
                min: 100,
                max: 100,
            },
            current_wall_ms: U64Summary {
                median: 110,
                min: 110,
                max: 110,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 0.0, // no variance with 1 sample
                min: 10.0,
                max: 10.0,
                count: 1,
            },
            baseline_max_rss_kb: None,
            current_max_rss_kb: None,
            rss_diff_kb: None,
            baseline_throughput_per_s: None,
            current_throughput_per_s: None,
            throughput_diff_per_s: None,
        };

        let comparison = compare_paired_stats(&stats);

        // With n=1, std_error should be 0
        assert_eq!(comparison.std_error, 0.0);
        // CI collapses to the mean
        assert_eq!(comparison.ci_95_lower, 10.0);
        assert_eq!(comparison.ci_95_upper, 10.0);
    }

    #[test]
    fn test_compare_paired_stats_zero_baseline() {
        // If baseline is 0, pct_change should be 0 (avoid division by zero)
        let stats = PairedStats {
            baseline_wall_ms: U64Summary {
                median: 0,
                min: 0,
                max: 0,
            },
            current_wall_ms: U64Summary {
                median: 10,
                min: 10,
                max: 10,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: 10.0,
                median: 10.0,
                std_dev: 0.0,
                min: 10.0,
                max: 10.0,
                count: 1,
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
        // Current is faster (negative diff)
        let stats = PairedStats {
            baseline_wall_ms: U64Summary {
                median: 100,
                min: 100,
                max: 100,
            },
            current_wall_ms: U64Summary {
                median: 80,
                min: 80,
                max: 80,
            },
            wall_diff_ms: PairedDiffSummary {
                mean: -20.0,
                median: -20.0,
                std_dev: 2.0,
                min: -22.0,
                max: -18.0,
                count: 5,
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
        assert_eq!(comparison.pct_change, -0.2); // -20 / 100 = -20%
        assert!(
            comparison.is_significant,
            "significant improvement should be detected"
        );
        assert!(
            comparison.ci_95_upper < 0.0,
            "CI upper bound should be negative for improvement"
        );
    }
}
