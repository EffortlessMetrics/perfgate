//! Direction-aware metric movement semantics.
//!
//! Raw percentage sign is display data. Product judgment must flow through
//! metric direction so lower-is-better and higher-is-better metrics agree on
//! what counts as improvement or regression.

use perfgate_types::{Delta, Direction, Metric};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricMovement {
    Improved,
    Regressed,
    Unchanged,
    Unknown,
}

#[must_use = "movement classification should drive user-facing judgment"]
pub fn movement_for_delta(metric: Metric, delta: &Delta) -> MetricMovement {
    movement_for_pct(metric, delta.pct)
}

#[must_use = "movement classification should drive user-facing judgment"]
pub fn movement_for_pct(metric: Metric, pct: f64) -> MetricMovement {
    if !pct.is_finite() {
        return MetricMovement::Unknown;
    }
    if pct == 0.0 {
        return MetricMovement::Unchanged;
    }

    match metric.default_direction() {
        Direction::Lower => {
            if pct < 0.0 {
                MetricMovement::Improved
            } else {
                MetricMovement::Regressed
            }
        }
        Direction::Higher => {
            if pct > 0.0 {
                MetricMovement::Improved
            } else {
                MetricMovement::Regressed
            }
        }
    }
}

#[must_use = "call site should branch on whether the metric improved"]
pub fn is_improvement(metric: Metric, delta: &Delta) -> bool {
    matches!(movement_for_delta(metric, delta), MetricMovement::Improved)
}

#[must_use = "call site should branch on whether the metric regressed"]
pub fn is_regression(metric: Metric, delta: &Delta) -> bool {
    matches!(movement_for_delta(metric, delta), MetricMovement::Regressed)
}

#[must_use = "tradeoff requirements should compare normalized improvement ratios"]
pub fn improvement_ratio(metric: Metric, delta: &Delta) -> Option<f64> {
    match metric.default_direction() {
        Direction::Higher => Some(delta.ratio),
        Direction::Lower => Some(if delta.current <= 0.0 {
            f64::INFINITY
        } else {
            delta.baseline / delta.current
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{MetricStatistic, MetricStatus};

    fn delta(baseline: f64, current: f64) -> Delta {
        Delta {
            baseline,
            current,
            ratio: current / baseline,
            pct: (current - baseline) / baseline,
            regression: 0.0,
            cv: None,
            noise_threshold: None,
            statistic: MetricStatistic::Median,
            significance: None,
            status: MetricStatus::Pass,
        }
    }

    #[test]
    fn movement_treats_lower_is_better_decrease_as_improvement() {
        let observed = movement_for_delta(Metric::WallMs, &delta(100.0, 80.0));

        assert_eq!(observed, MetricMovement::Improved);
    }

    #[test]
    fn movement_treats_lower_is_better_increase_as_regression() {
        let observed = movement_for_delta(Metric::WallMs, &delta(100.0, 120.0));

        assert_eq!(observed, MetricMovement::Regressed);
    }

    #[test]
    fn movement_treats_higher_is_better_increase_as_improvement() {
        let observed = movement_for_delta(Metric::ThroughputPerS, &delta(100.0, 125.0));

        assert_eq!(observed, MetricMovement::Improved);
    }

    #[test]
    fn movement_treats_higher_is_better_decrease_as_regression() {
        let observed = movement_for_delta(Metric::ThroughputPerS, &delta(100.0, 80.0));

        assert_eq!(observed, MetricMovement::Regressed);
    }

    #[test]
    fn movement_treats_zero_change_as_unchanged() {
        let observed = movement_for_delta(Metric::WallMs, &delta(100.0, 100.0));

        assert_eq!(observed, MetricMovement::Unchanged);
    }

    #[test]
    fn movement_treats_non_finite_pct_as_unknown() {
        let mut delta = delta(100.0, 80.0);
        delta.pct = f64::NAN;

        assert_eq!(
            movement_for_delta(Metric::WallMs, &delta),
            MetricMovement::Unknown
        );
    }

    #[test]
    fn improvement_ratio_uses_lower_is_better_direction() {
        let observed = improvement_ratio(Metric::WallMs, &delta(100.0, 80.0));

        assert_eq!(observed, Some(1.25));
    }

    #[test]
    fn improvement_ratio_uses_higher_is_better_direction() {
        let observed = improvement_ratio(Metric::ThroughputPerS, &delta(100.0, 125.0));

        assert_eq!(observed, Some(1.25));
    }
}
