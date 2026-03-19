//! Fuzz target for the validation function.
//!
//! This target verifies that validate_bench_name never panics
//! and always returns either Ok or Err for any input.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = perfgate_validation::validate_bench_name(s);
        // Should never panic, always return Ok or Err
    }
});

