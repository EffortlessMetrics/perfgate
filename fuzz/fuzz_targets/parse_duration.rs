#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Parse as humantime duration - only attempt if valid UTF-8
    if let Ok(s) = std::str::from_utf8(data) {
        // Fuzz humantime duration parsing
        // This should gracefully handle any invalid string without panicking
        let _ = humantime::parse_duration(s);
    }
});
