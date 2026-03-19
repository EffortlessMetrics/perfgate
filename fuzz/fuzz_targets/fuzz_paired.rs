//! Fuzz target for paired benchmarking statistics.
//!
//! This target verifies that `compute_paired_stats` and `compare_paired_stats`
//! never panic on arbitrary paired sample vectors, regardless of sample count,
//! wall times, or optional fields.

#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use perfgate_types::{PairedSample, PairedSampleHalf};

#[derive(Debug, Arbitrary)]
struct PairedInput {
    samples: Vec<FuzzPairedSample>,
    work_units: Option<u64>,
}

#[derive(Debug, Arbitrary)]
struct FuzzPairedSample {
    pair_index: u32,
    warmup: bool,
    baseline_wall_ms: u64,
    current_wall_ms: u64,
    baseline_rss: Option<u64>,
    current_rss: Option<u64>,
}

impl FuzzPairedSample {
    fn to_paired_sample(&self) -> PairedSample {
        let wall_diff_ms = self.current_wall_ms as i64 - self.baseline_wall_ms as i64;
        let rss_diff_kb = match (self.baseline_rss, self.current_rss) {
            (Some(b), Some(c)) => Some(c as i64 - b as i64),
            _ => None,
        };
        PairedSample {
            pair_index: self.pair_index,
            warmup: self.warmup,
            baseline: PairedSampleHalf {
                wall_ms: self.baseline_wall_ms,
                exit_code: 0,
                timed_out: false,
                max_rss_kb: self.baseline_rss,
                stdout: None,
                stderr: None,
            },
            current: PairedSampleHalf {
                wall_ms: self.current_wall_ms,
                exit_code: 0,
                timed_out: false,
                max_rss_kb: self.current_rss,
                stdout: None,
                stderr: None,
            },
            wall_diff_ms,
            rss_diff_kb,
        }
    }
}

fuzz_target!(|input: PairedInput| {
    let samples: Vec<PairedSample> = input
        .samples
        .iter()
        .take(100) // cap to avoid excessive memory
        .map(|s| s.to_paired_sample())
        .collect();

    if let Ok(stats) = perfgate_paired::compute_paired_stats(&samples, input.work_units) {
        // Also fuzz the comparison path
        let comparison = perfgate_paired::compare_paired_stats(&stats);

        // Invariant: CI lower <= CI upper
        assert!(comparison.ci_95_lower <= comparison.ci_95_upper);
    }
});








