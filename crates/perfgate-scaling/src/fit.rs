//! Curve fitting via ordinary least squares regression.
//!
//! For each complexity model, we perform a linear regression of the form:
//!   y = a * g(n) + b
//! where g(n) is the characteristic function for the complexity class.
//!
//! For O(1), we use a simple mean (no intercept needed).
//! For O(2^n), we cap input sizes to avoid overflow.

use crate::models::ComplexityClass;
use crate::report::SizeMeasurement;

/// Fit a single complexity model to the data.
///
/// Returns `(complexity_class, r_squared, coefficients)`.
/// Coefficients are `[a, b]` for `y = a * g(n) + b`.
#[must_use = "pure computation; call site should use the returned fit result"]
pub fn fit_model(
    measurements: &[SizeMeasurement],
    class: ComplexityClass,
) -> (ComplexityClass, f64, Vec<f64>) {
    match class {
        ComplexityClass::O1 => fit_constant(measurements),
        ComplexityClass::OExp => fit_exponential(measurements),
        _ => fit_linear_model(measurements, class),
    }
}

/// Fit all complexity models and return results sorted by fit quality.
#[must_use = "pure computation; call site should use the returned model fits"]
pub fn fit_all_models(measurements: &[SizeMeasurement]) -> Vec<(ComplexityClass, f64, Vec<f64>)> {
    ComplexityClass::all()
        .iter()
        .map(|&class| fit_model(measurements, class))
        .collect()
}

/// Compute R-squared (coefficient of determination) for given predictions.
///
/// R^2 = 1 - SS_res / SS_tot
/// where SS_res = sum((y_i - y_hat_i)^2) and SS_tot = sum((y_i - y_mean)^2)
#[must_use = "pure computation; call site should use the returned R-squared value"]
pub fn r_squared(actual: &[f64], predicted: &[f64]) -> f64 {
    if actual.len() != predicted.len() || actual.is_empty() {
        return 0.0;
    }

    let n = actual.len() as f64;
    let y_mean: f64 = actual.iter().sum::<f64>() / n;

    let ss_tot: f64 = actual.iter().map(|&y| (y - y_mean).powi(2)).sum();
    let ss_res: f64 = actual
        .iter()
        .zip(predicted.iter())
        .map(|(&y, &y_hat)| (y - y_hat).powi(2))
        .sum();

    if ss_tot < f64::EPSILON {
        // All values are the same -- perfect fit for constant model
        if ss_res < f64::EPSILON {
            return 1.0;
        }
        return 0.0;
    }

    // Clamp to avoid negative R^2 (which indicates very bad fit)
    (1.0 - ss_res / ss_tot).max(-1.0)
}

/// Fit the O(1) constant model: y = a (mean of y values).
fn fit_constant(measurements: &[SizeMeasurement]) -> (ComplexityClass, f64, Vec<f64>) {
    let n = measurements.len() as f64;
    let mean: f64 = measurements.iter().map(|m| m.time_ms).sum::<f64>() / n;

    let actual: Vec<f64> = measurements.iter().map(|m| m.time_ms).collect();
    let predicted: Vec<f64> = vec![mean; measurements.len()];

    let r2 = r_squared(&actual, &predicted);
    (ComplexityClass::O1, r2, vec![mean])
}

/// Fit a linear model y = a * g(n) + b using ordinary least squares.
fn fit_linear_model(
    measurements: &[SizeMeasurement],
    class: ComplexityClass,
) -> (ComplexityClass, f64, Vec<f64>) {
    let n = measurements.len() as f64;

    // Transform input sizes using the characteristic function
    let xs: Vec<f64> = measurements
        .iter()
        .map(|m| class.characteristic(m.input_size as f64))
        .collect();
    let ys: Vec<f64> = measurements.iter().map(|m| m.time_ms).collect();

    // Check for degenerate case where all x values are the same
    let x_min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if (x_max - x_min).abs() < f64::EPSILON {
        // Degenerate: treat as constant
        let y_mean = ys.iter().sum::<f64>() / n;
        let predicted: Vec<f64> = vec![y_mean; measurements.len()];
        let r2 = r_squared(&ys, &predicted);
        return (class, r2, vec![0.0, y_mean]);
    }

    // OLS: y = a*x + b
    // a = (n * sum(x*y) - sum(x) * sum(y)) / (n * sum(x^2) - (sum(x))^2)
    // b = (sum(y) - a * sum(x)) / n
    let sum_x: f64 = xs.iter().sum();
    let sum_y: f64 = ys.iter().sum();
    let sum_xy: f64 = xs.iter().zip(ys.iter()).map(|(x, y)| x * y).sum();
    let sum_x2: f64 = xs.iter().map(|x| x * x).sum();

    let denominator = n * sum_x2 - sum_x * sum_x;
    if denominator.abs() < f64::EPSILON {
        let y_mean = sum_y / n;
        let predicted: Vec<f64> = vec![y_mean; measurements.len()];
        let r2 = r_squared(&ys, &predicted);
        return (class, r2, vec![0.0, y_mean]);
    }

    let a = (n * sum_xy - sum_x * sum_y) / denominator;
    let b = (sum_y - a * sum_x) / n;

    let predicted: Vec<f64> = xs.iter().map(|&x| a * x + b).collect();
    let r2 = r_squared(&ys, &predicted);

    (class, r2, vec![a, b])
}

/// Fit the O(2^n) exponential model.
///
/// For exponential, we cap the characteristic function to avoid overflow
/// and use the same OLS approach on the transformed values.
fn fit_exponential(measurements: &[SizeMeasurement]) -> (ComplexityClass, f64, Vec<f64>) {
    let max_safe_exponent = 50.0; // 2^50 is about 1e15

    // Check if any input size would cause overflow
    let max_size = measurements.iter().map(|m| m.input_size).max().unwrap_or(0);

    if max_size as f64 > max_safe_exponent {
        // Exponential model is not applicable for large inputs
        let ys: Vec<f64> = measurements.iter().map(|m| m.time_ms).collect();
        let y_mean = ys.iter().sum::<f64>() / ys.len() as f64;
        let predicted: Vec<f64> = vec![y_mean; measurements.len()];
        let r2 = r_squared(&ys, &predicted);
        return (ComplexityClass::OExp, r2, vec![0.0, y_mean]);
    }

    fit_linear_model(measurements, ComplexityClass::OExp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_squared_perfect_fit() {
        let actual = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let predicted = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let r2 = r_squared(&actual, &predicted);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn r_squared_no_fit() {
        let actual = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        // Mean is 3.0
        let predicted = vec![3.0, 3.0, 3.0, 3.0, 3.0];
        let r2 = r_squared(&actual, &predicted);
        assert!(r2.abs() < 1e-10);
    }

    #[test]
    fn r_squared_constant_actual() {
        let actual = vec![5.0, 5.0, 5.0, 5.0];
        let predicted = vec![5.0, 5.0, 5.0, 5.0];
        let r2 = r_squared(&actual, &predicted);
        assert!((r2 - 1.0).abs() < 1e-10);
    }

    #[test]
    fn r_squared_empty() {
        assert_eq!(r_squared(&[], &[]), 0.0);
    }

    #[test]
    fn r_squared_mismatched_lengths() {
        assert_eq!(r_squared(&[1.0, 2.0], &[1.0]), 0.0);
    }

    #[test]
    fn fit_constant_model() {
        let data = vec![
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
        ];
        let (class, r2, coeffs) = fit_model(&data, ComplexityClass::O1);
        assert_eq!(class, ComplexityClass::O1);
        assert!((r2 - 1.0).abs() < 1e-10);
        assert!((coeffs[0] - 5.0).abs() < 1e-10);
    }

    #[test]
    fn fit_linear_model_perfect() {
        let data = vec![
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
        ];
        let (class, r2, coeffs) = fit_model(&data, ComplexityClass::ON);
        assert_eq!(class, ComplexityClass::ON);
        assert!(r2 > 0.99, "R^2 = {}", r2);
        // slope should be close to 0.1 (10/100)
        assert!((coeffs[0] - 0.1).abs() < 1e-6, "slope = {}", coeffs[0]);
    }

    #[test]
    fn fit_quadratic_model_perfect() {
        let data = vec![
            SizeMeasurement {
                input_size: 10,
                time_ms: 100.0,
            },
            SizeMeasurement {
                input_size: 20,
                time_ms: 400.0,
            },
            SizeMeasurement {
                input_size: 30,
                time_ms: 900.0,
            },
            SizeMeasurement {
                input_size: 40,
                time_ms: 1600.0,
            },
        ];
        let (class, r2, coeffs) = fit_model(&data, ComplexityClass::ON2);
        assert_eq!(class, ComplexityClass::ON2);
        assert!(r2 > 0.99, "R^2 = {}", r2);
        // coefficient should be close to 1.0
        assert!(
            (coeffs[0] - 1.0).abs() < 1e-3,
            "coefficient = {}",
            coeffs[0]
        );
    }

    #[test]
    fn fit_exponential_large_inputs_degrades_gracefully() {
        let data = vec![
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
        ];
        let (class, _r2, _coeffs) = fit_model(&data, ComplexityClass::OExp);
        assert_eq!(class, ComplexityClass::OExp);
        // Should not panic or produce NaN
    }

    #[test]
    fn fit_all_models_returns_all_classes() {
        let data = vec![
            SizeMeasurement {
                input_size: 10,
                time_ms: 10.0,
            },
            SizeMeasurement {
                input_size: 20,
                time_ms: 20.0,
            },
            SizeMeasurement {
                input_size: 30,
                time_ms: 30.0,
            },
        ];
        let results = fit_all_models(&data);
        assert_eq!(results.len(), ComplexityClass::all().len());
    }
}
