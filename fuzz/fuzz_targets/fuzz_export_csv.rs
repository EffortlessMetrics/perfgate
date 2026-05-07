//! Fuzz target for CSV export.
//!
//! This target verifies that csv_escape never panics on any input.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // CSV escape should never panic on any input
    let s = String::from_utf8_lossy(data);
    let _ = perfgate_export::csv_escape(&s);
});
