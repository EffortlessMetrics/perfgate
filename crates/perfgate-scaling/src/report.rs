//! Scaling report types for serialization and output.

use crate::models::ComplexityClass;
use serde::{Deserialize, Serialize};

/// A single measurement: input size paired with observed time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeMeasurement {
    /// The input size parameter (e.g., number of elements).
    pub input_size: u64,
    /// The observed execution time in milliseconds (median of repeats).
    pub time_ms: f64,
}

/// Result of fitting complexity models to measured data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingResult {
    /// The best-fitting complexity class.
    pub best_fit: ComplexityClass,
    /// R-squared goodness of fit for the best model (0.0 to 1.0).
    pub r_squared: f64,
    /// Fitted coefficients for the best model: [a, b] in y = a*g(n) + b.
    pub coefficients: Vec<f64>,
    /// Whether the R-squared meets the threshold.
    pub above_threshold: bool,
    /// All model fits sorted by R-squared descending.
    pub all_fits: Vec<(ComplexityClass, f64)>,
}

/// Full scaling report including benchmark metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingReport {
    /// Schema identifier for versioning.
    pub schema: String,
    /// Benchmark name.
    pub name: String,
    /// Command template (with {n} placeholder).
    pub command: String,
    /// Input sizes used.
    pub sizes: Vec<u64>,
    /// Number of repetitions per size.
    pub repeat: u32,
    /// Expected complexity (if specified).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<ComplexityClass>,
    /// All measurements.
    pub measurements: Vec<SizeMeasurement>,
    /// Classification result.
    pub result: ScalingResult,
    /// Whether the scaling validation passed.
    pub pass: bool,
    /// Human-readable verdict.
    pub verdict: String,
}

/// Schema identifier for scaling reports.
pub const SCALING_SCHEMA_V1: &str = "perfgate.scaling.v1";

impl ScalingReport {
    /// Create a new scaling report.
    pub fn new(
        name: String,
        command: String,
        sizes: Vec<u64>,
        repeat: u32,
        expected: Option<ComplexityClass>,
        measurements: Vec<SizeMeasurement>,
        result: ScalingResult,
    ) -> Self {
        let (pass, verdict) = Self::compute_verdict(&result, expected);

        Self {
            schema: SCALING_SCHEMA_V1.to_string(),
            name,
            command,
            sizes,
            repeat,
            expected,
            measurements,
            result,
            pass,
            verdict,
        }
    }

    fn compute_verdict(
        result: &ScalingResult,
        expected: Option<ComplexityClass>,
    ) -> (bool, String) {
        if !result.above_threshold {
            return (
                false,
                format!(
                    "No model fits well (best: {} with R^2={:.3})",
                    result.best_fit, result.r_squared
                ),
            );
        }

        match expected {
            Some(expected_class) => {
                if crate::is_complexity_degraded(expected_class, result.best_fit) {
                    (
                        false,
                        format!(
                            "Complexity degradation: expected {}, detected {} (R^2={:.3})",
                            expected_class, result.best_fit, result.r_squared
                        ),
                    )
                } else {
                    (
                        true,
                        format!(
                            "Scaling validated: detected {} (expected {}, R^2={:.3})",
                            result.best_fit, expected_class, result.r_squared
                        ),
                    )
                }
            }
            None => (
                true,
                format!(
                    "Detected complexity: {} (R^2={:.3})",
                    result.best_fit, result.r_squared
                ),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_measurement_serde_round_trip() {
        let m = SizeMeasurement {
            input_size: 1000,
            time_ms: 42.5,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: SizeMeasurement = serde_json::from_str(&json).unwrap();
        assert_eq!(back.input_size, m.input_size);
        assert!((back.time_ms - m.time_ms).abs() < f64::EPSILON);
    }

    #[test]
    fn scaling_result_serde_round_trip() {
        let result = ScalingResult {
            best_fit: ComplexityClass::ON,
            r_squared: 0.99,
            coefficients: vec![0.1, 0.5],
            above_threshold: true,
            all_fits: vec![(ComplexityClass::ON, 0.99), (ComplexityClass::ONLogN, 0.95)],
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: ScalingResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.best_fit, ComplexityClass::ON);
        assert!((back.r_squared - 0.99).abs() < f64::EPSILON);
    }

    #[test]
    fn scaling_report_pass_without_expected() {
        let result = ScalingResult {
            best_fit: ComplexityClass::ON,
            r_squared: 0.99,
            coefficients: vec![0.1, 0.5],
            above_threshold: true,
            all_fits: vec![(ComplexityClass::ON, 0.99)],
        };
        let report = ScalingReport::new(
            "test".into(),
            "echo {n}".into(),
            vec![100, 200, 400],
            5,
            None,
            vec![],
            result,
        );
        assert!(report.pass);
        assert!(report.verdict.contains("Detected complexity: O(n)"));
    }

    #[test]
    fn scaling_report_pass_with_matching_expected() {
        let result = ScalingResult {
            best_fit: ComplexityClass::ON,
            r_squared: 0.99,
            coefficients: vec![0.1, 0.5],
            above_threshold: true,
            all_fits: vec![(ComplexityClass::ON, 0.99)],
        };
        let report = ScalingReport::new(
            "test".into(),
            "echo {n}".into(),
            vec![100, 200, 400],
            5,
            Some(ComplexityClass::ON),
            vec![],
            result,
        );
        assert!(report.pass);
        assert!(report.verdict.contains("Scaling validated"));
    }

    #[test]
    fn scaling_report_fail_with_degradation() {
        let result = ScalingResult {
            best_fit: ComplexityClass::ON2,
            r_squared: 0.99,
            coefficients: vec![0.001, 0.5],
            above_threshold: true,
            all_fits: vec![(ComplexityClass::ON2, 0.99)],
        };
        let report = ScalingReport::new(
            "test".into(),
            "echo {n}".into(),
            vec![100, 200, 400],
            5,
            Some(ComplexityClass::ON),
            vec![],
            result,
        );
        assert!(!report.pass);
        assert!(report.verdict.contains("Complexity degradation"));
    }

    #[test]
    fn scaling_report_fail_low_r_squared() {
        let result = ScalingResult {
            best_fit: ComplexityClass::ON,
            r_squared: 0.5,
            coefficients: vec![0.1, 0.5],
            above_threshold: false,
            all_fits: vec![(ComplexityClass::ON, 0.5)],
        };
        let report = ScalingReport::new(
            "test".into(),
            "echo {n}".into(),
            vec![100, 200, 400],
            5,
            Some(ComplexityClass::ON),
            vec![],
            result,
        );
        assert!(!report.pass);
        assert!(report.verdict.contains("No model fits well"));
    }

    #[test]
    fn scaling_report_schema() {
        let result = ScalingResult {
            best_fit: ComplexityClass::O1,
            r_squared: 1.0,
            coefficients: vec![5.0],
            above_threshold: true,
            all_fits: vec![(ComplexityClass::O1, 1.0)],
        };
        let report = ScalingReport::new(
            "test".into(),
            "echo {n}".into(),
            vec![100],
            1,
            None,
            vec![],
            result,
        );
        assert_eq!(report.schema, SCALING_SCHEMA_V1);
    }

    #[test]
    fn scaling_report_pass_better_than_expected() {
        // Detected O(1) but expected O(n) -- still passes (better than expected)
        let result = ScalingResult {
            best_fit: ComplexityClass::O1,
            r_squared: 0.99,
            coefficients: vec![5.0],
            above_threshold: true,
            all_fits: vec![(ComplexityClass::O1, 0.99)],
        };
        let report = ScalingReport::new(
            "test".into(),
            "echo {n}".into(),
            vec![100, 200, 400],
            5,
            Some(ComplexityClass::ON),
            vec![],
            result,
        );
        assert!(report.pass);
    }
}
