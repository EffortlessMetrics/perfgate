//! Fuzz target for the SHA-256 implementation.
//!
//! This target verifies that the SHA-256 hash function is deterministic,
//! always produces valid hex output, and never panics.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let hash1 = perfgate_sha256::sha256_hex(data);
    let hash2 = perfgate_sha256::sha256_hex(data);

    // Determinism: same input should always produce same output
    assert_eq!(hash1, hash2);

    // Output should always be 64 hex characters
    assert_eq!(hash1.len(), 64);
    assert!(hash1.chars().all(|c| c.is_ascii_hexdigit()));
});








