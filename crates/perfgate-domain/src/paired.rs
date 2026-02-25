//! Paired statistics computation for perfgate.
//!
//! This module re-exports from `perfgate_paired` for backward compatibility.

pub use perfgate_paired::{
    PairedComparison, PairedError, compare_paired_stats, compute_paired_stats,
};

use crate::DomainError;

impl From<PairedError> for DomainError {
    fn from(err: PairedError) -> Self {
        match err {
            PairedError::NoSamples => DomainError::NoSamples,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{PairedSample, PairedSampleHalf};

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

    #[test]
    fn test_paired_error_conversion() {
        let paired_err = PairedError::NoSamples;
        let domain_err: DomainError = paired_err.into();
        assert!(matches!(domain_err, DomainError::NoSamples));
    }

    #[test]
    fn test_compute_paired_stats_empty_samples_returns_domain_error() {
        let samples: Vec<PairedSample> = vec![];
        let result = compute_paired_stats(&samples, None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let domain_err: DomainError = err.into();
        assert!(matches!(domain_err, DomainError::NoSamples));
    }

    #[test]
    fn test_re_exports_work() {
        let samples = vec![
            paired_sample(0, false, 100, 110),
            paired_sample(1, false, 100, 120),
        ];

        let stats = compute_paired_stats(&samples, None).unwrap();
        let comparison = compare_paired_stats(&stats);

        assert_eq!(stats.wall_diff_ms.mean, 15.0);
        assert_eq!(comparison.mean_diff_ms, 15.0);
    }
}
