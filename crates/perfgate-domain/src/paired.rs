//! Paired statistics computation for perfgate.

use perfgate_types::{PairedDiffSummary, PairedSample, PairedStats};
use crate::{summarize_f64, summarize_u64, DomainError};

pub fn compute_paired_stats(samples: &[PairedSample], work_units: Option<u64>) -> Result<PairedStats, DomainError> {
    let measured: Vec<&PairedSample> = samples.iter().filter(|s| !s.warmup).collect();
    if measured.is_empty() { return Err(DomainError::NoSamples); }
    
    let baseline_wall: Vec<u64> = measured.iter().map(|s| s.baseline.wall_ms).collect();
    let current_wall: Vec<u64> = measured.iter().map(|s| s.current.wall_ms).collect();
    let wall_diffs: Vec<f64> = measured.iter().map(|s| s.wall_diff_ms as f64).collect();
    
    let baseline_wall_ms = summarize_u64(&baseline_wall)?;
    let current_wall_ms = summarize_u64(&current_wall)?;
    let wall_diff_ms = summarize_paired_diffs(&wall_diffs)?;
    
    let baseline_rss: Vec<u64> = measured.iter().filter_map(|s| s.baseline.max_rss_kb).collect();
    let current_rss: Vec<u64> = measured.iter().filter_map(|s| s.current.max_rss_kb).collect();
    let rss_diffs: Vec<f64> = measured.iter().filter_map(|s| s.rss_diff_kb).map(|d| d as f64).collect();
    
    let baseline_max_rss_kb = if baseline_rss.is_empty() { None } else { Some(summarize_u64(&baseline_rss)?) };
    let current_max_rss_kb = if current_rss.is_empty() { None } else { Some(summarize_u64(&current_rss)?) };
    let rss_diff_kb = if rss_diffs.is_empty() { None } else { Some(summarize_paired_diffs(&rss_diffs)?) };
    
    let (baseline_throughput_per_s, current_throughput_per_s, throughput_diff_per_s) = match work_units {
        Some(work) => {
            let baseline_thr: Vec<f64> = measured.iter().map(|s| {
                let secs = s.baseline.wall_ms as f64 / 1000.0;
                if secs <= 0.0 { 0.0 } else { work as f64 / secs }
            }).collect();
            let current_thr: Vec<f64> = measured.iter().map(|s| {
                let secs = s.current.wall_ms as f64 / 1000.0;
                if secs <= 0.0 { 0.0 } else { work as f64 / secs }
            }).collect();
            let thr_diffs: Vec<f64> = baseline_thr.iter().zip(current_thr.iter()).map(|(b, c)| c - b).collect();
            (Some(summarize_f64(&baseline_thr)?), Some(summarize_f64(&current_thr)?), Some(summarize_paired_diffs(&thr_diffs)?))
        }
        None => (None, None, None),
    };
    
    Ok(PairedStats {
        baseline_wall_ms, current_wall_ms, wall_diff_ms,
        baseline_max_rss_kb, current_max_rss_kb, rss_diff_kb,
        baseline_throughput_per_s, current_throughput_per_s, throughput_diff_per_s,
    })
}

fn summarize_paired_diffs(diffs: &[f64]) -> Result<PairedDiffSummary, DomainError> {
    if diffs.is_empty() { return Err(DomainError::NoSamples); }
    let count = diffs.len() as u32;
    let mean = diffs.iter().sum::<f64>() / count as f64;
    let mut sorted = diffs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if count % 2 == 1 { sorted[(count / 2) as usize] } 
                 else { (sorted[(count / 2 - 1) as usize] + sorted[(count / 2) as usize]) / 2.0 };
    let min = *sorted.first().unwrap();
    let max = *sorted.last().unwrap();
    let variance = diffs.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / count as f64;
    let std_dev = variance.sqrt();
    Ok(PairedDiffSummary { mean, median, std_dev, min, max, count })
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
    let std_error = if n > 1.0 { diff.std_dev / n.sqrt() } else { 0.0 };
    let t_value = if n >= 30.0 { 1.96 } else { 2.0 };
    let ci_95_lower = diff.mean - t_value * std_error;
    let ci_95_upper = diff.mean + t_value * std_error;
    let is_significant = ci_95_lower > 0.0 || ci_95_upper < 0.0;
    let baseline_mean = stats.baseline_wall_ms.median as f64;
    let pct_change = if baseline_mean > 0.0 { diff.mean / baseline_mean } else { 0.0 };
    PairedComparison { mean_diff_ms: diff.mean, median_diff_ms: diff.median, pct_change, std_error, ci_95_lower, ci_95_upper, is_significant }
}
