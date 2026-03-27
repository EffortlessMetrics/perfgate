//! Computational complexity validation and curve fitting.
//!
//! This crate provides tools to validate that benchmarks conform to expected
//! algorithmic complexity classes (O(1), O(n), O(n log n), O(n^2), etc.)
//! by running benchmarks at multiple input sizes and fitting mathematical
//! models to the observed data.
//!
//! # Overview
//!
//! The crate provides:
//! - Complexity class definitions with expected function shapes
//! - Least-squares curve fitting for each model
//! - R-squared goodness-of-fit scoring
//! - Best-fit model selection
//! - Complexity degradation detection
//! - ASCII chart rendering
//!
//! # Design
//!
//! This crate is intentionally I/O-free: it does math and model fitting only.
//! Process execution and file I/O are handled by the CLI and app layers.

mod chart;
mod fit;
mod models;
mod report;

pub use chart::render_ascii_chart;
pub use fit::{fit_all_models, fit_model, r_squared};
pub use models::ComplexityClass;
pub use report::{ScalingReport, ScalingResult, SizeMeasurement};

/// Minimum R-squared value for a model to be considered a valid fit.
pub const DEFAULT_R_SQUARED_THRESHOLD: f64 = 0.90;

/// Determine the best-fitting complexity model for the given measurements.
///
/// Returns a `ScalingResult` with the best fit, R-squared, coefficients,
/// and all model fits sorted by goodness of fit.
///
/// # Arguments
///
/// * `measurements` - Input size and measured time pairs
/// * `r_squared_threshold` - Minimum R-squared for a valid fit (default: 0.90)
///
/// # Examples
///
/// ```
/// use perfgate_scaling::{SizeMeasurement, classify_complexity};
///
/// let measurements = vec![
///     SizeMeasurement { input_size: 100, time_ms: 10.0 },
///     SizeMeasurement { input_size: 200, time_ms: 20.0 },
///     SizeMeasurement { input_size: 400, time_ms: 40.0 },
///     SizeMeasurement { input_size: 800, time_ms: 80.0 },
/// ];
///
/// let result = classify_complexity(&measurements, None);
/// assert!(result.is_ok());
/// ```
pub fn classify_complexity(
    measurements: &[SizeMeasurement],
    r_squared_threshold: Option<f64>,
) -> Result<ScalingResult, ScalingError> {
    if measurements.len() < 3 {
        return Err(ScalingError::InsufficientData {
            needed: 3,
            got: measurements.len(),
        });
    }

    // Validate all measurements have positive input sizes and finite times
    for m in measurements {
        if m.input_size == 0 {
            return Err(ScalingError::InvalidInputSize);
        }
        if !m.time_ms.is_finite() || m.time_ms < 0.0 {
            return Err(ScalingError::InvalidMeasurement);
        }
    }

    let threshold = r_squared_threshold.unwrap_or(DEFAULT_R_SQUARED_THRESHOLD);
    let all_fits = fit_all_models(measurements);

    // Sort by R-squared descending (best fit first)
    let mut sorted_fits = all_fits.clone();
    sorted_fits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (best_class, best_r_squared, best_coefficients) = sorted_fits
        .first()
        .cloned()
        .ok_or(ScalingError::NoValidFit)?;

    let all_fits_summary: Vec<(ComplexityClass, f64)> =
        sorted_fits.iter().map(|(c, r2, _)| (*c, *r2)).collect();

    Ok(ScalingResult {
        best_fit: best_class,
        r_squared: best_r_squared,
        coefficients: best_coefficients,
        above_threshold: best_r_squared >= threshold,
        all_fits: all_fits_summary,
    })
}

/// Check if actual complexity is worse than expected.
///
/// Returns `true` if the detected complexity is worse (higher order)
/// than the expected complexity.
pub fn is_complexity_degraded(expected: ComplexityClass, actual: ComplexityClass) -> bool {
    actual.order() > expected.order()
}

/// Parse a complexity class from a string like "O(n)", "O(n^2)", etc.
///
/// # Examples
///
/// ```
/// use perfgate_scaling::{parse_complexity, ComplexityClass};
///
/// assert_eq!(parse_complexity("O(1)").unwrap(), ComplexityClass::O1);
/// assert_eq!(parse_complexity("O(n)").unwrap(), ComplexityClass::ON);
/// assert_eq!(parse_complexity("O(n^2)").unwrap(), ComplexityClass::ON2);
/// ```
pub fn parse_complexity(s: &str) -> Result<ComplexityClass, ScalingError> {
    let normalized = s.trim().to_lowercase().replace(' ', "").replace("o(", "O(");

    match normalized.as_str() {
        "O(1)" | "o1" | "constant" => Ok(ComplexityClass::O1),
        "O(logn)" | "O(log(n))" | "ologn" | "logarithmic" => Ok(ComplexityClass::OLogN),
        "O(n)" | "on" | "linear" => Ok(ComplexityClass::ON),
        "O(nlogn)" | "O(nlog(n))" | "onlogn" | "linearithmic" => Ok(ComplexityClass::ONLogN),
        "O(n^2)" | "O(n2)" | "on2" | "quadratic" => Ok(ComplexityClass::ON2),
        "O(n^3)" | "O(n3)" | "on3" | "cubic" => Ok(ComplexityClass::ON3),
        "O(2^n)" | "O(2n)" | "oexp" | "exponential" => Ok(ComplexityClass::OExp),
        _ => Err(ScalingError::UnknownComplexity(s.to_string())),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScalingError {
    #[error("insufficient data points for curve fitting: need at least {needed}, got {got}")]
    InsufficientData { needed: usize, got: usize },

    #[error("input size must be positive (non-zero)")]
    InvalidInputSize,

    #[error("measurement must be finite and non-negative")]
    InvalidMeasurement,

    #[error("no valid complexity model fit the data")]
    NoValidFit,

    #[error("unknown complexity class: {0}")]
    UnknownComplexity(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_data() -> Vec<SizeMeasurement> {
        vec![
            SizeMeasurement {
                input_size: 100,
                time_ms: 10.0,
            },
            SizeMeasurement {
                input_size: 200,
                time_ms: 20.0,
            },
            SizeMeasurement {
                input_size: 400,
                time_ms: 40.0,
            },
            SizeMeasurement {
                input_size: 800,
                time_ms: 80.0,
            },
            SizeMeasurement {
                input_size: 1600,
                time_ms: 160.0,
            },
        ]
    }

    fn quadratic_data() -> Vec<SizeMeasurement> {
        // f(n) = 0.001 * n^2
        vec![
            SizeMeasurement {
                input_size: 10,
                time_ms: 0.1,
            },
            SizeMeasurement {
                input_size: 20,
                time_ms: 0.4,
            },
            SizeMeasurement {
                input_size: 40,
                time_ms: 1.6,
            },
            SizeMeasurement {
                input_size: 80,
                time_ms: 6.4,
            },
            SizeMeasurement {
                input_size: 160,
                time_ms: 25.6,
            },
        ]
    }

    fn constant_data() -> Vec<SizeMeasurement> {
        vec![
            SizeMeasurement {
                input_size: 100,
                time_ms: 5.0,
            },
            SizeMeasurement {
                input_size: 1000,
                time_ms: 5.0,
            },
            SizeMeasurement {
                input_size: 10000,
                time_ms: 5.0,
            },
            SizeMeasurement {
                input_size: 100000,
                time_ms: 5.0,
            },
        ]
    }

    fn nlogn_data() -> Vec<SizeMeasurement> {
        // f(n) = 0.01 * n * log2(n)
        vec![
            SizeMeasurement {
                input_size: 100,
                time_ms: 0.01 * 100.0 * (100.0_f64).ln(),
            },
            SizeMeasurement {
                input_size: 1000,
                time_ms: 0.01 * 1000.0 * (1000.0_f64).ln(),
            },
            SizeMeasurement {
                input_size: 10000,
                time_ms: 0.01 * 10000.0 * (10000.0_f64).ln(),
            },
            SizeMeasurement {
                input_size: 100000,
                time_ms: 0.01 * 100000.0 * (100000.0_f64).ln(),
            },
        ]
    }

    #[test]
    fn classify_linear_data() {
        let result = classify_complexity(&linear_data(), None).unwrap();
        assert_eq!(result.best_fit, ComplexityClass::ON);
        assert!(result.r_squared > 0.99);
        assert!(result.above_threshold);
    }

    #[test]
    fn classify_quadratic_data() {
        let result = classify_complexity(&quadratic_data(), None).unwrap();
        assert_eq!(result.best_fit, ComplexityClass::ON2);
        assert!(result.r_squared > 0.99);
    }

    #[test]
    fn classify_constant_data() {
        let result = classify_complexity(&constant_data(), None).unwrap();
        assert_eq!(result.best_fit, ComplexityClass::O1);
    }

    #[test]
    fn classify_nlogn_data() {
        let result = classify_complexity(&nlogn_data(), None).unwrap();
        assert_eq!(result.best_fit, ComplexityClass::ONLogN);
        assert!(result.r_squared > 0.99);
    }

    #[test]
    fn insufficient_data_returns_error() {
        let data = vec![
            SizeMeasurement {
                input_size: 100,
                time_ms: 10.0,
            },
            SizeMeasurement {
                input_size: 200,
                time_ms: 20.0,
            },
        ];
        let result = classify_complexity(&data, None);
        assert!(matches!(
            result,
            Err(ScalingError::InsufficientData { needed: 3, got: 2 })
        ));
    }

    #[test]
    fn zero_input_size_returns_error() {
        let data = vec![
            SizeMeasurement {
                input_size: 0,
                time_ms: 10.0,
            },
            SizeMeasurement {
                input_size: 200,
                time_ms: 20.0,
            },
            SizeMeasurement {
                input_size: 300,
                time_ms: 30.0,
            },
        ];
        let result = classify_complexity(&data, None);
        assert!(matches!(result, Err(ScalingError::InvalidInputSize)));
    }

    #[test]
    fn negative_time_returns_error() {
        let data = vec![
            SizeMeasurement {
                input_size: 100,
                time_ms: -1.0,
            },
            SizeMeasurement {
                input_size: 200,
                time_ms: 20.0,
            },
            SizeMeasurement {
                input_size: 300,
                time_ms: 30.0,
            },
        ];
        let result = classify_complexity(&data, None);
        assert!(matches!(result, Err(ScalingError::InvalidMeasurement)));
    }

    #[test]
    fn nan_time_returns_error() {
        let data = vec![
            SizeMeasurement {
                input_size: 100,
                time_ms: f64::NAN,
            },
            SizeMeasurement {
                input_size: 200,
                time_ms: 20.0,
            },
            SizeMeasurement {
                input_size: 300,
                time_ms: 30.0,
            },
        ];
        let result = classify_complexity(&data, None);
        assert!(matches!(result, Err(ScalingError::InvalidMeasurement)));
    }

    #[test]
    fn parse_complexity_variants() {
        assert_eq!(parse_complexity("O(1)").unwrap(), ComplexityClass::O1);
        assert_eq!(
            parse_complexity("O(log n)").unwrap(),
            ComplexityClass::OLogN
        );
        assert_eq!(parse_complexity("O(n)").unwrap(), ComplexityClass::ON);
        assert_eq!(
            parse_complexity("O(n log n)").unwrap(),
            ComplexityClass::ONLogN
        );
        assert_eq!(parse_complexity("O(n^2)").unwrap(), ComplexityClass::ON2);
        assert_eq!(parse_complexity("O(n^3)").unwrap(), ComplexityClass::ON3);
        assert_eq!(parse_complexity("O(2^n)").unwrap(), ComplexityClass::OExp);
    }

    #[test]
    fn parse_complexity_aliases() {
        assert_eq!(parse_complexity("constant").unwrap(), ComplexityClass::O1);
        assert_eq!(parse_complexity("linear").unwrap(), ComplexityClass::ON);
        assert_eq!(parse_complexity("quadratic").unwrap(), ComplexityClass::ON2);
        assert_eq!(parse_complexity("cubic").unwrap(), ComplexityClass::ON3);
        assert_eq!(
            parse_complexity("exponential").unwrap(),
            ComplexityClass::OExp
        );
    }

    #[test]
    fn parse_complexity_unknown() {
        assert!(matches!(
            parse_complexity("O(n!)"),
            Err(ScalingError::UnknownComplexity(_))
        ));
    }

    #[test]
    fn is_complexity_degraded_detects_regression() {
        assert!(is_complexity_degraded(
            ComplexityClass::ON,
            ComplexityClass::ON2
        ));
        assert!(is_complexity_degraded(
            ComplexityClass::ONLogN,
            ComplexityClass::ON2
        ));
        assert!(is_complexity_degraded(
            ComplexityClass::O1,
            ComplexityClass::ON
        ));
    }

    #[test]
    fn is_complexity_degraded_no_regression() {
        assert!(!is_complexity_degraded(
            ComplexityClass::ON2,
            ComplexityClass::ON
        ));
        assert!(!is_complexity_degraded(
            ComplexityClass::ON,
            ComplexityClass::ON
        ));
        assert!(!is_complexity_degraded(
            ComplexityClass::ON,
            ComplexityClass::O1
        ));
    }

    #[test]
    fn all_fits_sorted_by_r_squared() {
        let result = classify_complexity(&linear_data(), None).unwrap();
        for window in result.all_fits.windows(2) {
            assert!(
                window[0].1 >= window[1].1,
                "all_fits should be sorted by R-squared descending"
            );
        }
    }

    #[test]
    fn r_squared_threshold_respected() {
        let result = classify_complexity(&linear_data(), Some(0.999)).unwrap();
        // For perfect linear data, R^2 should be very high
        assert!(result.above_threshold);

        let result = classify_complexity(&linear_data(), Some(1.0)).unwrap();
        // Exact 1.0 threshold is nearly impossible
        // The fit is very good but might not be exactly 1.0
        assert!(result.r_squared > 0.99);
    }

    #[test]
    fn complexity_class_display() {
        assert_eq!(ComplexityClass::O1.to_string(), "O(1)");
        assert_eq!(ComplexityClass::OLogN.to_string(), "O(log n)");
        assert_eq!(ComplexityClass::ON.to_string(), "O(n)");
        assert_eq!(ComplexityClass::ONLogN.to_string(), "O(n log n)");
        assert_eq!(ComplexityClass::ON2.to_string(), "O(n^2)");
        assert_eq!(ComplexityClass::ON3.to_string(), "O(n^3)");
        assert_eq!(ComplexityClass::OExp.to_string(), "O(2^n)");
    }

    #[test]
    fn complexity_class_ordering() {
        assert!(ComplexityClass::O1.order() < ComplexityClass::OLogN.order());
        assert!(ComplexityClass::OLogN.order() < ComplexityClass::ON.order());
        assert!(ComplexityClass::ON.order() < ComplexityClass::ONLogN.order());
        assert!(ComplexityClass::ONLogN.order() < ComplexityClass::ON2.order());
        assert!(ComplexityClass::ON2.order() < ComplexityClass::ON3.order());
        assert!(ComplexityClass::ON3.order() < ComplexityClass::OExp.order());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_linear_data_classified_as_linear(
            slope in 0.01_f64..100.0,
            intercept in 0.0_f64..50.0
        ) {
            let sizes = [100u64, 500, 1000, 5000, 10000];
            let measurements: Vec<SizeMeasurement> = sizes
                .iter()
                .map(|&n| SizeMeasurement {
                    input_size: n,
                    time_ms: slope * n as f64 + intercept,
                })
                .collect();

            let result = classify_complexity(&measurements, None).unwrap();
            prop_assert_eq!(result.best_fit, ComplexityClass::ON);
            prop_assert!(result.r_squared > 0.99);
        }

        #[test]
        fn prop_quadratic_data_classified_as_quadratic(
            coeff in 0.0001_f64..1.0
        ) {
            let sizes = [10u64, 50, 100, 500, 1000];
            let measurements: Vec<SizeMeasurement> = sizes
                .iter()
                .map(|&n| SizeMeasurement {
                    input_size: n,
                    time_ms: coeff * (n as f64).powi(2),
                })
                .collect();

            let result = classify_complexity(&measurements, None).unwrap();
            prop_assert_eq!(result.best_fit, ComplexityClass::ON2);
            prop_assert!(result.r_squared > 0.99);
        }

        #[test]
        fn prop_constant_data_classified_as_constant(
            value in 1.0_f64..1000.0
        ) {
            let sizes = [100u64, 1000, 10000, 100000];
            let measurements: Vec<SizeMeasurement> = sizes
                .iter()
                .map(|&n| SizeMeasurement {
                    input_size: n,
                    time_ms: value,
                })
                .collect();

            let result = classify_complexity(&measurements, None).unwrap();
            prop_assert_eq!(result.best_fit, ComplexityClass::O1);
        }

        #[test]
        fn prop_degradation_is_asymmetric(
            a_idx in 0usize..7,
            b_idx in 0usize..7,
        ) {
            let classes = [
                ComplexityClass::O1,
                ComplexityClass::OLogN,
                ComplexityClass::ON,
                ComplexityClass::ONLogN,
                ComplexityClass::ON2,
                ComplexityClass::ON3,
                ComplexityClass::OExp,
            ];
            let a = classes[a_idx];
            let b = classes[b_idx];

            if a_idx < b_idx {
                prop_assert!(is_complexity_degraded(a, b));
                prop_assert!(!is_complexity_degraded(b, a));
            } else if a_idx == b_idx {
                prop_assert!(!is_complexity_degraded(a, b));
            }
        }
    }
}
