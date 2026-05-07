//! Complexity class definitions and model evaluation.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Supported computational complexity classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ComplexityClass {
    /// O(1) - Constant time
    O1,
    /// O(log n) - Logarithmic time
    OLogN,
    /// O(n) - Linear time
    ON,
    /// O(n log n) - Linearithmic time
    ONLogN,
    /// O(n^2) - Quadratic time
    ON2,
    /// O(n^3) - Cubic time
    ON3,
    /// O(2^n) - Exponential time
    OExp,
}

impl ComplexityClass {
    /// Returns the ordering value for this complexity class.
    /// Lower values indicate faster growth rates.
    pub fn order(&self) -> u8 {
        match self {
            ComplexityClass::O1 => 0,
            ComplexityClass::OLogN => 1,
            ComplexityClass::ON => 2,
            ComplexityClass::ONLogN => 3,
            ComplexityClass::ON2 => 4,
            ComplexityClass::ON3 => 5,
            ComplexityClass::OExp => 6,
        }
    }

    /// Canonical string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            ComplexityClass::O1 => "O(1)",
            ComplexityClass::OLogN => "O(log n)",
            ComplexityClass::ON => "O(n)",
            ComplexityClass::ONLogN => "O(n log n)",
            ComplexityClass::ON2 => "O(n^2)",
            ComplexityClass::ON3 => "O(n^3)",
            ComplexityClass::OExp => "O(2^n)",
        }
    }

    /// All supported complexity classes, ordered from fastest to slowest growth.
    pub fn all() -> &'static [ComplexityClass] {
        &[
            ComplexityClass::O1,
            ComplexityClass::OLogN,
            ComplexityClass::ON,
            ComplexityClass::ONLogN,
            ComplexityClass::ON2,
            ComplexityClass::ON3,
            ComplexityClass::OExp,
        ]
    }

    /// Evaluate the model function given input size `n` and fitted coefficients.
    ///
    /// Each model has the form `f(n) = a * g(n) + b` where `g(n)` is the
    /// characteristic function for the complexity class:
    /// - O(1): f(n) = a
    /// - O(log n): f(n) = a * ln(n) + b
    /// - O(n): f(n) = a * n + b
    /// - O(n log n): f(n) = a * n * ln(n) + b
    /// - O(n^2): f(n) = a * n^2 + b
    /// - O(n^3): f(n) = a * n^3 + b
    /// - O(2^n): f(n) = a * 2^n + b
    pub fn evaluate(&self, n: f64, coefficients: &[f64]) -> f64 {
        let a = coefficients.first().copied().unwrap_or(0.0);
        let b = coefficients.get(1).copied().unwrap_or(0.0);

        match self {
            ComplexityClass::O1 => a,
            ComplexityClass::OLogN => a * n.ln() + b,
            ComplexityClass::ON => a * n + b,
            ComplexityClass::ONLogN => a * n * n.ln() + b,
            ComplexityClass::ON2 => a * n * n + b,
            ComplexityClass::ON3 => a * n * n * n + b,
            ComplexityClass::OExp => a * (2.0_f64).powf(n) + b,
        }
    }

    /// Returns the characteristic function value g(n) for this complexity class.
    /// Used in linear regression: we fit y = a * g(n) + b.
    pub fn characteristic(&self, n: f64) -> f64 {
        match self {
            ComplexityClass::O1 => 1.0,
            ComplexityClass::OLogN => n.ln(),
            ComplexityClass::ON => n,
            ComplexityClass::ONLogN => n * n.ln(),
            ComplexityClass::ON2 => n * n,
            ComplexityClass::ON3 => n * n * n,
            ComplexityClass::OExp => (2.0_f64).powf(n),
        }
    }
}

impl fmt::Display for ComplexityClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_o1() {
        let class = ComplexityClass::O1;
        assert_eq!(class.evaluate(100.0, &[5.0]), 5.0);
        assert_eq!(class.evaluate(1000.0, &[5.0]), 5.0);
    }

    #[test]
    fn evaluate_on() {
        let class = ComplexityClass::ON;
        assert_eq!(class.evaluate(100.0, &[2.0, 5.0]), 205.0);
        assert_eq!(class.evaluate(200.0, &[2.0, 5.0]), 405.0);
    }

    #[test]
    fn evaluate_on2() {
        let class = ComplexityClass::ON2;
        assert_eq!(class.evaluate(10.0, &[1.0, 0.0]), 100.0);
        assert_eq!(class.evaluate(20.0, &[1.0, 0.0]), 400.0);
    }

    #[test]
    fn evaluate_on3() {
        let class = ComplexityClass::ON3;
        assert_eq!(class.evaluate(10.0, &[1.0, 0.0]), 1000.0);
    }

    #[test]
    fn evaluate_ologn() {
        let class = ComplexityClass::OLogN;
        let val = class.evaluate(std::f64::consts::E, &[1.0, 0.0]);
        assert!((val - 1.0).abs() < 1e-10);
    }

    #[test]
    fn evaluate_onlogn() {
        let class = ComplexityClass::ONLogN;
        let n = std::f64::consts::E;
        let val = class.evaluate(n, &[1.0, 0.0]);
        // n * ln(n) = e * 1 = e
        assert!((val - std::f64::consts::E).abs() < 1e-10);
    }

    #[test]
    fn evaluate_oexp() {
        let class = ComplexityClass::OExp;
        let val = class.evaluate(3.0, &[1.0, 0.0]);
        assert!((val - 8.0).abs() < 1e-10);
    }

    #[test]
    fn characteristic_values() {
        let n = 100.0;
        assert_eq!(ComplexityClass::O1.characteristic(n), 1.0);
        assert!((ComplexityClass::OLogN.characteristic(n) - n.ln()).abs() < 1e-10);
        assert_eq!(ComplexityClass::ON.characteristic(n), n);
        assert!((ComplexityClass::ONLogN.characteristic(n) - n * n.ln()).abs() < 1e-10);
        assert_eq!(ComplexityClass::ON2.characteristic(n), n * n);
        assert_eq!(ComplexityClass::ON3.characteristic(n), n * n * n);
    }

    #[test]
    fn all_classes_ordered() {
        let all = ComplexityClass::all();
        for window in all.windows(2) {
            assert!(window[0].order() < window[1].order());
        }
    }

    #[test]
    fn display_round_trips_through_as_str() {
        for class in ComplexityClass::all() {
            assert_eq!(class.to_string(), class.as_str());
        }
    }

    #[test]
    fn serde_round_trip() {
        for class in ComplexityClass::all() {
            let json = serde_json::to_string(class).unwrap();
            let back: ComplexityClass = serde_json::from_str(&json).unwrap();
            assert_eq!(*class, back);
        }
    }
}
