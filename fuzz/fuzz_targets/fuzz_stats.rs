//! Fuzz target for the statistics functions.
//!
//! This target verifies that statistical functions maintain their invariants
//! (ordering, correctness) and never panic on arbitrary input.

#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Arbitrary)]
struct StatsInput {
    values: Vec<u64>,
}

fuzz_target!(|input: StatsInput| {
    if !input.values.is_empty() {
        if let Ok(summary) = perfgate_stats::summarize_u64(&input.values) {
            // Ordering invariant
            assert!(summary.min <= summary.median);
            assert!(summary.median <= summary.max);

            // Min/max should be from the input
            assert!(input.values.contains(&summary.min));
            assert!(input.values.contains(&summary.max));
        }
    }
});








