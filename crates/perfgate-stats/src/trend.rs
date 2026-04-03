//! Trend analysis for predicting budget breaches.
//!
//! Provides simple linear regression over a metric history to detect drift
//! and predict when a budget threshold will be exceeded.

use serde::{Deserialize, Serialize};

/// Classification of metric drift direction and severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftClass {
    /// Metric is stable (slope near zero or improving).
    Stable,
    /// Metric is improving (moving away from threshold).
    Improving,
    /// Metric is degrading (moving toward threshold but not imminent).
    Degrading,
    /// Metric will breach threshold within the critical window.
    Critical,
}

impl DriftClass {
    /// Returns the string representation.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Improving => "improving",
            Self::Degrading => "degrading",
            Self::Critical => "critical",
        }
    }
}

impl std::fmt::Display for DriftClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Result of trend analysis for a single metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendAnalysis {
    /// Metric name (e.g., "wall_ms").
    pub metric: String,
    /// Slope per run (change in metric value per run index).
    pub slope_per_run: f64,
    /// Intercept of the linear regression line.
    pub intercept: f64,
    /// R-squared value (0.0-1.0) indicating fit quality.
    pub r_squared: f64,
    /// Drift classification.
    pub drift: DriftClass,
    /// Estimated runs until budget threshold is exceeded.
    /// `None` if the metric is not trending toward the threshold.
    pub runs_to_breach: Option<u32>,
    /// Current headroom as a percentage (how far current value is from threshold).
    /// Positive means below threshold, negative means already exceeded.
    pub current_headroom_pct: f64,
    /// Number of data points used.
    pub sample_count: usize,
}

/// Parameters controlling drift classification thresholds.
#[derive(Debug, Clone)]
pub struct TrendConfig {
    /// Number of runs within which a breach is considered "critical".
    pub critical_window: u32,
    /// Minimum R-squared to trust the trend direction.
    pub min_r_squared: f64,
    /// Minimum absolute slope (relative to current value) to count as non-stable.
    /// Slopes smaller than `stable_threshold * current_value` are considered stable.
    pub stable_threshold: f64,
}

impl Default for TrendConfig {
    fn default() -> Self {
        Self {
            critical_window: 10,
            min_r_squared: 0.3,
            stable_threshold: 0.001,
        }
    }
}

/// Fit a simple linear regression: y = slope * x + intercept.
///
/// Takes `(x, y)` pairs and returns `(slope, intercept, r_squared)`.
///
/// Returns `None` if fewer than 2 points are provided or if the regression
/// is degenerate (all x values equal).
///
/// # Examples
///
/// ```
/// use perfgate_stats::trend::linear_regression;
///
/// let points = vec![(0.0, 1.0), (1.0, 2.0), (2.0, 3.0)];
/// let (slope, intercept, r2) = linear_regression(&points).unwrap();
/// assert!((slope - 1.0).abs() < 1e-10);
/// assert!((intercept - 1.0).abs() < 1e-10);
/// assert!((r2 - 1.0).abs() < 1e-10);
/// ```
#[must_use = "pure computation; call site should use the returned regression coefficients"]
pub fn linear_regression(points: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    let n = points.len();
    if n < 2 {
        return None;
    }

    let n_f = n as f64;

    // First pass: accumulate all sums needed for slope/intercept.
    let (sum_x, sum_y, sum_xy, sum_x2) = points.iter().fold(
        (0.0f64, 0.0f64, 0.0f64, 0.0f64),
        |(sx, sy, sxy, sx2), &(x, y)| (sx + x, sy + y, sxy + x * y, sx2 + x * x),
    );

    let denom = n_f * sum_x2 - sum_x * sum_x;
    if denom.abs() < f64::EPSILON {
        // All x values are equal; cannot fit a line.
        return None;
    }

    let slope = (n_f * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n_f;

    // Second pass: R-squared (coefficient of determination).
    let mean_y = sum_y / n_f;
    let (ss_tot, ss_res) = points.iter().fold((0.0f64, 0.0f64), |(tot, res), &(x, y)| {
        let predicted = slope * x + intercept;
        (tot + (y - mean_y).powi(2), res + (y - predicted).powi(2))
    });

    let r_squared = if ss_tot.abs() < f64::EPSILON {
        // All y values are equal: perfect fit if ss_res is also 0.
        if ss_res.abs() < f64::EPSILON {
            1.0
        } else {
            0.0
        }
    } else {
        (1.0 - ss_res / ss_tot).clamp(0.0, 1.0)
    };

    if slope.is_finite() && intercept.is_finite() && r_squared.is_finite() {
        Some((slope, intercept, r_squared))
    } else {
        None
    }
}

/// Predict the run index at which the regression line crosses `threshold`.
///
/// Returns `None` if the slope is zero, the line does not approach the threshold,
/// or the crossing is in the past.
///
/// `direction_lower_is_better` indicates whether lower metric values are desirable.
/// - `true` (e.g., wall_ms): breach when value rises above threshold.
/// - `false` (e.g., throughput_per_s): breach when value drops below threshold.
#[must_use = "pure computation; call site should use the returned breach prediction"]
pub fn predict_breach_run(
    slope: f64,
    intercept: f64,
    current_run: f64,
    threshold: f64,
    direction_lower_is_better: bool,
) -> Option<f64> {
    if slope.abs() < f64::EPSILON {
        return None;
    }

    // y = slope * x + intercept = threshold
    // x = (threshold - intercept) / slope
    let breach_run = (threshold - intercept) / slope;

    // Only return if breach is in the future
    if breach_run <= current_run {
        return None;
    }

    // Check that the slope is actually moving toward the threshold
    let current_value = slope * current_run + intercept;
    if direction_lower_is_better {
        // For "lower is better", degrading means value is increasing toward threshold
        if current_value >= threshold {
            return None; // Already past threshold
        }
        if slope <= 0.0 {
            return None; // Moving away from threshold
        }
    } else {
        // For "higher is better", degrading means value is decreasing toward threshold
        if current_value <= threshold {
            return None; // Already past threshold
        }
        if slope >= 0.0 {
            return None; // Moving away from threshold
        }
    }

    Some(breach_run)
}

/// Classify the drift of a metric given regression parameters.
///
/// `current_value` is the metric value at the latest run.
/// `threshold` is the budget fail threshold (absolute value the metric must not exceed).
/// `direction_lower_is_better` specifies the metric direction.
#[must_use = "pure computation; call site should use the returned DriftClass"]
pub fn classify_drift(
    slope: f64,
    r_squared: f64,
    current_value: f64,
    _threshold: f64,
    direction_lower_is_better: bool,
    config: &TrendConfig,
    runs_to_breach: Option<u32>,
) -> DriftClass {
    // If R-squared is too low, the trend is unreliable
    if r_squared < config.min_r_squared {
        return DriftClass::Stable;
    }

    // Check if slope is negligible relative to current value
    let reference = if current_value.abs() > f64::EPSILON {
        current_value.abs()
    } else {
        1.0
    };
    if (slope / reference).abs() < config.stable_threshold {
        return DriftClass::Stable;
    }

    // Determine if the metric is moving toward or away from the threshold
    let moving_toward_threshold = if direction_lower_is_better {
        slope > 0.0 // Value increasing toward threshold
    } else {
        slope < 0.0 // Value decreasing toward threshold
    };

    if !moving_toward_threshold {
        return DriftClass::Improving;
    }

    // The metric is degrading; check if breach is imminent
    if let Some(runs) = runs_to_breach
        && runs <= config.critical_window
    {
        return DriftClass::Critical;
    }

    DriftClass::Degrading
}

/// Compute headroom as a percentage: how far the current value is from the threshold.
///
/// For "lower is better" metrics: headroom = (threshold - current) / threshold * 100.
/// For "higher is better" metrics: headroom = (current - threshold) / threshold * 100.
///
/// Positive headroom means the metric is within budget. Negative means it has exceeded.
#[must_use = "pure computation; call site should use the returned headroom percentage"]
pub fn compute_headroom_pct(
    current_value: f64,
    threshold: f64,
    direction_lower_is_better: bool,
) -> f64 {
    if threshold.abs() < f64::EPSILON {
        return 0.0;
    }
    if direction_lower_is_better {
        (threshold - current_value) / threshold * 100.0
    } else {
        (current_value - threshold) / threshold * 100.0
    }
}

/// Perform a full trend analysis on a sequence of metric values.
///
/// `values` is a series of metric values in chronological order (index = run number).
/// `metric_name` is the metric identifier (e.g., "wall_ms").
/// `threshold` is the absolute budget threshold value.
/// `direction_lower_is_better` specifies the metric direction.
/// `config` controls classification thresholds.
///
/// Returns `None` if fewer than 2 data points are provided or regression fails.
///
/// # Examples
///
/// ```
/// use perfgate_stats::trend::{TrendConfig, analyze_trend, DriftClass};
///
/// let values = vec![100.0, 102.0, 104.0, 106.0, 108.0];
/// let result = analyze_trend(&values, "wall_ms", 150.0, true, &TrendConfig::default()).unwrap();
/// assert_eq!(result.drift, DriftClass::Degrading);
/// assert!(result.runs_to_breach.is_some());
/// ```
#[must_use = "pure computation; call site should use the returned TrendAnalysis"]
pub fn analyze_trend(
    values: &[f64],
    metric_name: &str,
    threshold: f64,
    direction_lower_is_better: bool,
    config: &TrendConfig,
) -> Option<TrendAnalysis> {
    if values.len() < 2 {
        return None;
    }

    let points: Vec<(f64, f64)> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    let (slope, intercept, r_squared) = linear_regression(&points)?;

    let current_run = (values.len() - 1) as f64;
    let current_value = slope * current_run + intercept;

    let headroom_pct = compute_headroom_pct(current_value, threshold, direction_lower_is_better);

    let breach_run = predict_breach_run(
        slope,
        intercept,
        current_run,
        threshold,
        direction_lower_is_better,
    );

    let runs_to_breach = breach_run.map(|br| {
        let remaining = br - current_run;
        remaining.ceil().max(1.0) as u32
    });

    let drift = classify_drift(
        slope,
        r_squared,
        current_value,
        threshold,
        direction_lower_is_better,
        config,
        runs_to_breach,
    );

    Some(TrendAnalysis {
        metric: metric_name.to_string(),
        slope_per_run: slope,
        intercept,
        r_squared,
        drift,
        runs_to_breach,
        current_headroom_pct: headroom_pct,
        sample_count: values.len(),
    })
}

/// Render a mini ASCII spark chart for a series of values.
///
/// Returns a string like `_-^-_--^^` representing the relative magnitude
/// of each value within the series.
///
/// # Examples
///
/// ```
/// use perfgate_stats::trend::spark_chart;
///
/// let chart = spark_chart(&[1.0, 2.0, 3.0, 4.0, 5.0]);
/// assert_eq!(chart.len(), 5);
/// ```
#[must_use = "pure computation; call site should use the returned spark chart"]
pub fn spark_chart(values: &[f64]) -> String {
    if values.is_empty() {
        return String::new();
    }

    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max - min;

    if range < f64::EPSILON {
        return "_".repeat(values.len());
    }

    // Use 8 levels of spark characters
    let sparks = ['_', '.', '-', '~', '=', '+', '^', '#'];

    values
        .iter()
        .map(|&v| {
            let normalized = (v - min) / range;
            let idx = (normalized * (sparks.len() - 1) as f64).round() as usize;
            sparks[idx.min(sparks.len() - 1)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_regression_perfect_fit() {
        let points = vec![(0.0, 1.0), (1.0, 3.0), (2.0, 5.0), (3.0, 7.0)];
        let (slope, intercept, r2) = linear_regression(&points).unwrap();
        assert!((slope - 2.0).abs() < 1e-10);
        assert!((intercept - 1.0).abs() < 1e-10);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn linear_regression_flat_line() {
        let points = vec![(0.0, 5.0), (1.0, 5.0), (2.0, 5.0)];
        let (slope, intercept, r2) = linear_regression(&points).unwrap();
        assert!(slope.abs() < 1e-10);
        assert!((intercept - 5.0).abs() < 1e-10);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn linear_regression_two_points() {
        let points = vec![(0.0, 10.0), (1.0, 20.0)];
        let (slope, intercept, r2) = linear_regression(&points).unwrap();
        assert!((slope - 10.0).abs() < 1e-10);
        assert!((intercept - 10.0).abs() < 1e-10);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn linear_regression_single_point_returns_none() {
        assert!(linear_regression(&[(0.0, 5.0)]).is_none());
    }

    #[test]
    fn linear_regression_empty_returns_none() {
        assert!(linear_regression(&[]).is_none());
    }

    #[test]
    fn linear_regression_same_x_returns_none() {
        let points = vec![(1.0, 2.0), (1.0, 4.0), (1.0, 6.0)];
        assert!(linear_regression(&points).is_none());
    }

    #[test]
    fn linear_regression_noisy_data() {
        // y ~= 2*x + 1 with some noise
        let points = vec![(0.0, 1.2), (1.0, 2.8), (2.0, 5.1), (3.0, 7.3), (4.0, 8.9)];
        let (slope, _intercept, r2) = linear_regression(&points).unwrap();
        // Slope should be approximately 2.0
        assert!((slope - 2.0).abs() < 0.5);
        // R-squared should be high for this nearly-linear data
        assert!(r2 > 0.95);
    }

    #[test]
    fn predict_breach_lower_is_better_increasing() {
        // Value increasing toward threshold of 150
        let breach = predict_breach_run(2.0, 100.0, 4.0, 150.0, true);
        assert!(breach.is_some());
        let br = breach.unwrap();
        // 2.0 * x + 100 = 150 => x = 25
        assert!((br - 25.0).abs() < 1e-10);
    }

    #[test]
    fn predict_breach_lower_is_better_decreasing() {
        // Value decreasing away from threshold (improving)
        let breach = predict_breach_run(-2.0, 100.0, 4.0, 150.0, true);
        assert!(breach.is_none());
    }

    #[test]
    fn predict_breach_already_past() {
        // Current value already past threshold
        let breach = predict_breach_run(2.0, 160.0, 4.0, 150.0, true);
        assert!(breach.is_none());
    }

    #[test]
    fn predict_breach_higher_is_better_decreasing() {
        // Throughput dropping toward threshold of 50
        let breach = predict_breach_run(-3.0, 200.0, 10.0, 50.0, false);
        assert!(breach.is_some());
        // -3 * x + 200 = 50 => x = 50
        let br = breach.unwrap();
        assert!((br - 50.0).abs() < 1e-10);
    }

    #[test]
    fn predict_breach_zero_slope() {
        assert!(predict_breach_run(0.0, 100.0, 4.0, 150.0, true).is_none());
    }

    #[test]
    fn classify_drift_stable_low_r2() {
        let drift = classify_drift(1.0, 0.1, 100.0, 150.0, true, &TrendConfig::default(), None);
        assert_eq!(drift, DriftClass::Stable);
    }

    #[test]
    fn classify_drift_stable_small_slope() {
        let drift = classify_drift(
            0.0001,
            0.9,
            100.0,
            150.0,
            true,
            &TrendConfig::default(),
            None,
        );
        assert_eq!(drift, DriftClass::Stable);
    }

    #[test]
    fn classify_drift_improving() {
        // Lower is better, slope negative => improving
        let drift = classify_drift(-2.0, 0.9, 100.0, 150.0, true, &TrendConfig::default(), None);
        assert_eq!(drift, DriftClass::Improving);
    }

    #[test]
    fn classify_drift_degrading() {
        // Lower is better, slope positive, will breach in 30 runs
        let drift = classify_drift(
            2.0,
            0.9,
            100.0,
            150.0,
            true,
            &TrendConfig::default(),
            Some(30),
        );
        assert_eq!(drift, DriftClass::Degrading);
    }

    #[test]
    fn classify_drift_critical() {
        // Lower is better, slope positive, will breach in 5 runs (within default window of 10)
        let drift = classify_drift(
            2.0,
            0.9,
            100.0,
            150.0,
            true,
            &TrendConfig::default(),
            Some(5),
        );
        assert_eq!(drift, DriftClass::Critical);
    }

    #[test]
    fn classify_drift_critical_boundary() {
        // Exactly at critical window boundary
        let drift = classify_drift(
            2.0,
            0.9,
            100.0,
            150.0,
            true,
            &TrendConfig::default(),
            Some(10),
        );
        assert_eq!(drift, DriftClass::Critical);
    }

    #[test]
    fn classify_drift_just_outside_critical() {
        let drift = classify_drift(
            2.0,
            0.9,
            100.0,
            150.0,
            true,
            &TrendConfig::default(),
            Some(11),
        );
        assert_eq!(drift, DriftClass::Degrading);
    }

    #[test]
    fn headroom_pct_within_budget() {
        let h = compute_headroom_pct(100.0, 120.0, true);
        // (120 - 100) / 120 * 100 = 16.67%
        assert!((h - 16.666666666666668).abs() < 1e-10);
    }

    #[test]
    fn headroom_pct_exceeded_budget() {
        let h = compute_headroom_pct(130.0, 120.0, true);
        // (120 - 130) / 120 * 100 = -8.33%
        assert!(h < 0.0);
    }

    #[test]
    fn headroom_pct_higher_is_better() {
        let h = compute_headroom_pct(200.0, 100.0, false);
        // (200 - 100) / 100 * 100 = 100%
        assert!((h - 100.0).abs() < 1e-10);
    }

    #[test]
    fn headroom_pct_zero_threshold() {
        assert_eq!(compute_headroom_pct(100.0, 0.0, true), 0.0);
    }

    #[test]
    fn analyze_trend_degrading() {
        let values = vec![100.0, 102.0, 104.0, 106.0, 108.0];
        let result = analyze_trend(&values, "wall_ms", 120.0, true, &TrendConfig::default());
        let analysis = result.unwrap();
        assert_eq!(analysis.metric, "wall_ms");
        assert!((analysis.slope_per_run - 2.0).abs() < 1e-10);
        assert!(analysis.r_squared > 0.99);
        assert!(matches!(
            analysis.drift,
            DriftClass::Degrading | DriftClass::Critical
        ));
        assert!(analysis.runs_to_breach.is_some());
        assert!(analysis.current_headroom_pct > 0.0);
    }

    #[test]
    fn analyze_trend_improving() {
        let values = vec![115.0, 112.0, 109.0, 106.0, 103.0];
        let result = analyze_trend(&values, "wall_ms", 120.0, true, &TrendConfig::default());
        let analysis = result.unwrap();
        assert_eq!(analysis.drift, DriftClass::Improving);
        assert!(analysis.runs_to_breach.is_none());
    }

    #[test]
    fn analyze_trend_critical() {
        // At this rate will breach 120 within a few runs
        let values = vec![100.0, 105.0, 110.0, 115.0];
        let result = analyze_trend(&values, "wall_ms", 120.0, true, &TrendConfig::default());
        let analysis = result.unwrap();
        assert_eq!(analysis.drift, DriftClass::Critical);
        assert!(analysis.runs_to_breach.unwrap() <= 10);
    }

    #[test]
    fn analyze_trend_single_point() {
        assert!(analyze_trend(&[100.0], "wall_ms", 120.0, true, &TrendConfig::default()).is_none());
    }

    #[test]
    fn analyze_trend_empty() {
        assert!(analyze_trend(&[], "wall_ms", 120.0, true, &TrendConfig::default()).is_none());
    }

    #[test]
    fn analyze_trend_flat() {
        let values = vec![100.0, 100.0, 100.0, 100.0, 100.0];
        let result = analyze_trend(&values, "wall_ms", 120.0, true, &TrendConfig::default());
        let analysis = result.unwrap();
        assert_eq!(analysis.drift, DriftClass::Stable);
    }

    #[test]
    fn analyze_trend_higher_is_better() {
        // Throughput declining
        let values = vec![200.0, 195.0, 190.0, 185.0, 180.0];
        let result = analyze_trend(
            &values,
            "throughput_per_s",
            100.0,
            false,
            &TrendConfig::default(),
        );
        let analysis = result.unwrap();
        assert_eq!(analysis.drift, DriftClass::Degrading);
        assert!(analysis.runs_to_breach.is_some());
    }

    #[test]
    fn spark_chart_basic() {
        let chart = spark_chart(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(chart.len(), 5);
        assert_eq!(chart.chars().next(), Some('_'));
        assert_eq!(chart.chars().last(), Some('#'));
    }

    #[test]
    fn spark_chart_flat() {
        let chart = spark_chart(&[5.0, 5.0, 5.0]);
        assert_eq!(chart, "___");
    }

    #[test]
    fn spark_chart_empty() {
        assert_eq!(spark_chart(&[]), "");
    }

    #[test]
    fn spark_chart_single() {
        let chart = spark_chart(&[42.0]);
        assert_eq!(chart, "_");
    }

    #[test]
    fn analyze_trend_sample_count() {
        let values = vec![10.0, 20.0, 30.0];
        let analysis =
            analyze_trend(&values, "wall_ms", 100.0, true, &TrendConfig::default()).unwrap();
        assert_eq!(analysis.sample_count, 3);
    }

    #[test]
    fn runs_to_breach_rounds_up() {
        // slope=2, intercept=100, current_run=4, threshold=110
        // breach at x = (110-100)/2 = 5.0, remaining = 5.0 - 4.0 = 1.0 => 1 run
        let values = vec![100.0, 102.0, 104.0, 106.0, 108.0];
        let result = analyze_trend(&values, "wall_ms", 110.0, true, &TrendConfig::default());
        let analysis = result.unwrap();
        assert_eq!(analysis.runs_to_breach, Some(1));
    }
}
