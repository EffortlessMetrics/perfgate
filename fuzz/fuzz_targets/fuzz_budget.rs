#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct BudgetInput {
    baseline: f64,
    current: f64,
    threshold: f64,
    warn_threshold: f64,
}

fuzz_target!(|input: BudgetInput| {
    use perfgate_budget::{calculate_regression, determine_status};
    use perfgate_types::Direction;

    if input.baseline > 0.0 && input.threshold > 0.0 && input.warn_threshold > 0.0 {
        let regression = calculate_regression(input.baseline, input.current, Direction::Lower);
        let _ = determine_status(regression, input.threshold, input.warn_threshold);
    }
});








