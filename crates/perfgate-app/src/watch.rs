//! Watch use case — re-run benchmarks on file changes with live terminal output.
//!
//! This module provides the core logic for the `perfgate watch` command.
//! It runs benchmarks, compares against a baseline, and tracks trend history
//! for display in a live terminal UI.
//!
//! The filesystem-watching layer lives in the CLI crate; this module handles
//! the benchmark execution cycle and result formatting.

use crate::{CheckRequest, CheckUseCase, Clock, format_metric, format_value};
use perfgate_adapters::{HostProbe, ProcessRunner};
use perfgate_types::{
    ConfigFile, HostMismatchPolicy, Metric, MetricStatus, RunReceipt, ToolInfo, VerdictStatus,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

// Re-export CheckOutcome for external use
pub use crate::CheckOutcome;

/// Request for a single watch iteration.
#[derive(Debug, Clone)]
pub struct WatchRunRequest {
    /// The loaded configuration file.
    pub config: ConfigFile,

    /// Name of the bench to run.
    pub bench_name: String,

    /// Output directory for artifacts.
    pub out_dir: PathBuf,

    /// Optional baseline receipt (already loaded).
    pub baseline: Option<RunReceipt>,

    /// Path to the baseline file.
    pub baseline_path: Option<PathBuf>,

    /// Tool info for receipts.
    pub tool: ToolInfo,

    /// Environment variables for the benchmark.
    pub env: Vec<(String, String)>,

    /// Max bytes captured from stdout/stderr per run.
    pub output_cap_bytes: usize,

    /// Policy for handling host mismatches.
    pub host_mismatch_policy: HostMismatchPolicy,
}

/// Result of a single watch iteration.
#[derive(Debug, Clone)]
pub struct WatchRunResult {
    /// The check outcome from this iteration.
    pub outcome: CheckOutcome,

    /// How long the benchmark took to run.
    pub elapsed: std::time::Duration,
}

/// Trend direction for a metric across watch iterations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    /// Performance is improving (getting faster/smaller).
    Improving,
    /// Performance is degrading (getting slower/larger).
    Degrading,
    /// Performance is stable (within noise).
    Stable,
}

/// A snapshot of metric trends accumulated over watch iterations.
#[derive(Debug, Clone)]
pub struct MetricTrend {
    /// Recent delta percentages for this metric.
    pub history: Vec<f64>,
    /// The current trend direction.
    pub direction: TrendDirection,
}

/// Accumulated state across watch iterations.
#[derive(Debug, Clone)]
pub struct WatchState {
    /// Number of completed iterations.
    pub iteration_count: u32,
    /// Metric trend data.
    pub trends: BTreeMap<Metric, MetricTrend>,
    /// Last run result (if any).
    pub last_result: Option<WatchRunResult>,
    /// When the last run completed.
    pub last_run_time: Option<Instant>,
    /// Total number of passes.
    pub pass_count: u32,
    /// Total number of warnings.
    pub warn_count: u32,
    /// Total number of failures.
    pub fail_count: u32,
}

impl WatchState {
    /// Create a new empty watch state.
    pub fn new() -> Self {
        Self {
            iteration_count: 0,
            trends: BTreeMap::new(),
            last_result: None,
            last_run_time: None,
            pass_count: 0,
            warn_count: 0,
            fail_count: 0,
        }
    }

    /// Update the state with a new watch run result.
    pub fn update(&mut self, result: WatchRunResult) {
        self.iteration_count += 1;
        self.last_run_time = Some(Instant::now());

        // Update verdict counts
        if let Some(compare) = &result.outcome.compare_receipt {
            match compare.verdict.status {
                VerdictStatus::Pass | VerdictStatus::Skip => self.pass_count += 1,
                VerdictStatus::Warn => self.warn_count += 1,
                VerdictStatus::Fail => self.fail_count += 1,
            }

            // Update metric trends
            for (metric, delta) in &compare.deltas {
                let trend = self.trends.entry(*metric).or_insert_with(|| MetricTrend {
                    history: Vec::new(),
                    direction: TrendDirection::Stable,
                });
                trend.history.push(delta.pct);
                // Keep at most 20 entries in trend history
                if trend.history.len() > MAX_TREND_HISTORY {
                    trend.history.remove(0);
                }
                trend.direction = compute_trend_direction(&trend.history);
            }
        } else {
            // No baseline means pass
            self.pass_count += 1;
        }

        self.last_result = Some(result);
    }
}

impl Default for WatchState {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum number of trend history entries to keep per metric.
const MAX_TREND_HISTORY: usize = 20;

/// Threshold below which a trend is considered stable (1% change).
const STABLE_THRESHOLD: f64 = 0.01;

/// Compute trend direction from a history of delta percentages.
///
/// Uses a simple moving average of the last 3 entries to determine direction.
pub fn compute_trend_direction(history: &[f64]) -> TrendDirection {
    if history.len() < 2 {
        return TrendDirection::Stable;
    }

    let window = if history.len() >= 3 {
        &history[history.len() - 3..]
    } else {
        history
    };

    let avg: f64 = window.iter().sum::<f64>() / window.len() as f64;

    if avg.abs() < STABLE_THRESHOLD {
        TrendDirection::Stable
    } else if avg > 0.0 {
        // Positive pct means current > baseline, which is a regression for "lower is better"
        // metrics. But since pct is already direction-aware from the comparison logic,
        // positive = worse, negative = better.
        TrendDirection::Degrading
    } else {
        TrendDirection::Improving
    }
}

/// Execute a single watch iteration: run the benchmark and compare against baseline.
pub fn execute_watch_run<R: ProcessRunner + Clone, H: HostProbe + Clone, C: Clock + Clone>(
    runner: R,
    host_probe: H,
    clock: C,
    request: &WatchRunRequest,
) -> anyhow::Result<WatchRunResult> {
    let start = Instant::now();

    let usecase = CheckUseCase::new(runner, host_probe, clock);
    let outcome = usecase.execute(CheckRequest {
        config: request.config.clone(),
        bench_name: request.bench_name.clone(),
        out_dir: request.out_dir.clone(),
        baseline: request.baseline.clone(),
        baseline_path: request.baseline_path.clone(),
        require_baseline: false,
        fail_on_warn: false,
        noise_threshold: None,
        noise_policy: None,
        tool: request.tool.clone(),
        env: request.env.clone(),
        output_cap_bytes: request.output_cap_bytes,
        allow_nonzero: false,
        host_mismatch_policy: request.host_mismatch_policy,
        significance_alpha: None,
        significance_min_samples: 8,
        require_significance: false,
    })?;

    let elapsed = start.elapsed();
    Ok(WatchRunResult { outcome, elapsed })
}

/// Format the trend direction as a terminal-friendly string.
pub fn trend_arrow(direction: TrendDirection) -> &'static str {
    match direction {
        TrendDirection::Improving => ">> improving",
        TrendDirection::Degrading => ">> degrading",
        TrendDirection::Stable => ">> stable",
    }
}

/// Format the verdict status as a terminal-friendly string.
pub fn verdict_display(status: VerdictStatus) -> &'static str {
    match status {
        VerdictStatus::Pass => "PASS",
        VerdictStatus::Warn => "WARN",
        VerdictStatus::Fail => "FAIL",
        VerdictStatus::Skip => "SKIP",
    }
}

/// Render the watch state as a plain-text terminal display.
///
/// Returns a vector of lines to print. The caller is responsible for
/// clearing the screen and printing these lines.
pub fn render_watch_display(state: &WatchState, bench_name: &str, status: &str) -> Vec<String> {
    let mut lines = Vec::new();

    lines.push(format!(
        "perfgate watch | bench: {} | status: {}",
        bench_name, status
    ));
    lines.push(format!(
        "iterations: {} | pass: {} | warn: {} | fail: {}",
        state.iteration_count, state.pass_count, state.warn_count, state.fail_count
    ));

    if let Some(last_run_time) = state.last_run_time {
        let ago = last_run_time.elapsed();
        lines.push(format!("last run: {}s ago", ago.as_secs()));
    }

    lines.push(String::new());

    if let Some(result) = &state.last_result {
        if let Some(compare) = &result.outcome.compare_receipt {
            lines.push(format!(
                "verdict: {} (ran in {:.1}s)",
                verdict_display(compare.verdict.status),
                result.elapsed.as_secs_f64()
            ));
            lines.push(String::new());

            // Table header
            lines.push(format!(
                "{:<20} {:>12} {:>12} {:>10} {:>8}  {}",
                "Metric", "Baseline", "Current", "Delta", "Status", "Trend"
            ));
            lines.push("-".repeat(80));

            for (metric, delta) in &compare.deltas {
                let status_str = match delta.status {
                    MetricStatus::Pass => "pass",
                    MetricStatus::Warn => "WARN",
                    MetricStatus::Fail => "FAIL",
                    MetricStatus::Skip => "skip",
                };

                let trend_str = state
                    .trends
                    .get(metric)
                    .map(|t| trend_arrow(t.direction))
                    .unwrap_or("");

                lines.push(format!(
                    "{:<20} {:>12} {:>12} {:>9}% {:>8}  {}",
                    format_metric(*metric),
                    format_value(*metric, delta.baseline),
                    format_value(*metric, delta.current),
                    format!("{:+.1}", delta.pct * 100.0),
                    status_str,
                    trend_str,
                ));
            }

            if !compare.verdict.reasons.is_empty() {
                lines.push(String::new());
                for reason in &compare.verdict.reasons {
                    lines.push(format!("  {}", reason));
                }
            }
        } else {
            lines.push(format!(
                "no baseline (ran in {:.1}s)",
                result.elapsed.as_secs_f64()
            ));

            // Show raw run stats
            let receipt = &result.outcome.run_receipt;
            lines.push(String::new());
            lines.push(format!("{:<20} {:>12}", "Metric", "Value"));
            lines.push("-".repeat(35));

            lines.push(format!(
                "{:<20} {:>12}",
                "wall_ms",
                format!("{}", receipt.stats.wall_ms.median)
            ));
            if let Some(cpu) = &receipt.stats.cpu_ms {
                lines.push(format!(
                    "{:<20} {:>12}",
                    "cpu_ms",
                    format!("{}", cpu.median)
                ));
            }
            if let Some(rss) = &receipt.stats.max_rss_kb {
                lines.push(format!(
                    "{:<20} {:>12}",
                    "max_rss_kb",
                    format!("{}", rss.median)
                ));
            }
        }

        // Show warnings from the outcome
        if !result.outcome.warnings.is_empty() {
            lines.push(String::new());
            for w in &result.outcome.warnings {
                lines.push(format!("warning: {}", w));
            }
        }
    } else {
        lines.push("waiting for first run...".to_string());
    }

    lines.push(String::new());
    lines.push("press Ctrl+C to stop".to_string());

    lines
}

/// Debounce helper: tracks incoming events and determines when to trigger.
#[derive(Debug)]
pub struct Debouncer {
    /// Debounce interval in milliseconds.
    debounce_ms: u64,
    /// When the last event was received.
    last_event: Option<Instant>,
    /// Whether an event is pending.
    pending: bool,
}

impl Debouncer {
    /// Create a new debouncer with the given interval in milliseconds.
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            last_event: None,
            pending: false,
        }
    }

    /// Record an incoming event.
    pub fn event(&mut self) {
        self.last_event = Some(Instant::now());
        self.pending = true;
    }

    /// Check if the debounce interval has elapsed since the last event.
    /// If so, consume the pending flag and return true.
    pub fn should_trigger(&mut self) -> bool {
        if !self.pending {
            return false;
        }
        if let Some(last) = self.last_event
            && last.elapsed().as_millis() >= self.debounce_ms as u128
        {
            self.pending = false;
            return true;
        }
        false
    }

    /// Return the remaining time until debounce triggers, or None if not pending.
    pub fn remaining_ms(&self) -> Option<u64> {
        if !self.pending {
            return None;
        }
        if let Some(last) = self.last_event {
            let elapsed = last.elapsed().as_millis() as u64;
            if elapsed >= self.debounce_ms {
                Some(0)
            } else {
                Some(self.debounce_ms - elapsed)
            }
        } else {
            None
        }
    }

    /// Returns true if there is a pending event that hasn't triggered yet.
    pub fn is_pending(&self) -> bool {
        self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn debouncer_new_is_not_pending() {
        let mut d = Debouncer::new(500);
        assert!(!d.is_pending());
        assert!(!d.should_trigger());
    }

    #[test]
    fn debouncer_event_sets_pending() {
        let mut d = Debouncer::new(500);
        d.event();
        assert!(d.is_pending());
    }

    #[test]
    fn debouncer_does_not_trigger_immediately() {
        let mut d = Debouncer::new(100);
        d.event();
        // Should not trigger immediately
        assert!(!d.should_trigger());
        assert!(d.is_pending());
    }

    #[test]
    fn debouncer_triggers_after_interval() {
        let mut d = Debouncer::new(50);
        d.event();
        thread::sleep(Duration::from_millis(60));
        assert!(d.should_trigger());
        // After triggering, pending should be false
        assert!(!d.is_pending());
    }

    #[test]
    fn debouncer_resets_on_new_event() {
        let mut d = Debouncer::new(80);
        d.event();
        thread::sleep(Duration::from_millis(40));
        // New event resets the timer
        d.event();
        // Should not trigger yet (only 0ms since last event)
        assert!(!d.should_trigger());
        thread::sleep(Duration::from_millis(90));
        assert!(d.should_trigger());
    }

    #[test]
    fn debouncer_remaining_ms_when_not_pending() {
        let d = Debouncer::new(500);
        assert_eq!(d.remaining_ms(), None);
    }

    #[test]
    fn debouncer_remaining_ms_when_pending() {
        let mut d = Debouncer::new(200);
        d.event();
        let remaining = d.remaining_ms().unwrap();
        // Should be close to 200ms (allow some tolerance)
        assert!(remaining <= 200);
        assert!(remaining > 150);
    }

    #[test]
    fn debouncer_remaining_ms_after_elapsed() {
        let mut d = Debouncer::new(30);
        d.event();
        thread::sleep(Duration::from_millis(40));
        assert_eq!(d.remaining_ms(), Some(0));
    }

    #[test]
    fn trend_direction_stable_for_empty() {
        assert_eq!(compute_trend_direction(&[]), TrendDirection::Stable);
    }

    #[test]
    fn trend_direction_stable_for_single() {
        assert_eq!(compute_trend_direction(&[0.05]), TrendDirection::Stable);
    }

    #[test]
    fn trend_direction_degrading_for_positive() {
        assert_eq!(
            compute_trend_direction(&[0.05, 0.06, 0.07]),
            TrendDirection::Degrading
        );
    }

    #[test]
    fn trend_direction_improving_for_negative() {
        assert_eq!(
            compute_trend_direction(&[-0.05, -0.06, -0.07]),
            TrendDirection::Improving
        );
    }

    #[test]
    fn trend_direction_stable_for_small_values() {
        assert_eq!(
            compute_trend_direction(&[0.001, -0.002, 0.003]),
            TrendDirection::Stable
        );
    }

    #[test]
    fn trend_uses_last_three_entries() {
        // History has old degrading values but recent improving values
        let history = vec![0.10, 0.15, 0.20, -0.05, -0.06, -0.07];
        assert_eq!(compute_trend_direction(&history), TrendDirection::Improving);
    }

    #[test]
    fn watch_state_default_is_empty() {
        let state = WatchState::default();
        assert_eq!(state.iteration_count, 0);
        assert_eq!(state.pass_count, 0);
        assert_eq!(state.warn_count, 0);
        assert_eq!(state.fail_count, 0);
        assert!(state.last_result.is_none());
        assert!(state.last_run_time.is_none());
        assert!(state.trends.is_empty());
    }

    #[test]
    fn render_watch_display_waiting() {
        let state = WatchState::new();
        let lines = render_watch_display(&state, "my-bench", "idle");
        assert!(lines.iter().any(|l| l.contains("my-bench")));
        assert!(lines.iter().any(|l| l.contains("idle")));
        assert!(lines.iter().any(|l| l.contains("waiting for first run")));
        assert!(lines.iter().any(|l| l.contains("Ctrl+C")));
    }

    #[test]
    fn trend_arrow_formatting() {
        assert_eq!(trend_arrow(TrendDirection::Improving), ">> improving");
        assert_eq!(trend_arrow(TrendDirection::Degrading), ">> degrading");
        assert_eq!(trend_arrow(TrendDirection::Stable), ">> stable");
    }

    #[test]
    fn verdict_display_formatting() {
        assert_eq!(verdict_display(VerdictStatus::Pass), "PASS");
        assert_eq!(verdict_display(VerdictStatus::Warn), "WARN");
        assert_eq!(verdict_display(VerdictStatus::Fail), "FAIL");
        assert_eq!(verdict_display(VerdictStatus::Skip), "SKIP");
    }
}
