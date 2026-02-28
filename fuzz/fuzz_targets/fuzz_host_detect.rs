//! Fuzz target for host mismatch detection.
//!
//! This target verifies that `detect_host_mismatch` never panics on
//! arbitrary HostInfo pairs, including edge cases with empty strings,
//! zero values, and mixed Option fields.

#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use perfgate_types::HostInfo;

#[derive(Debug, Arbitrary)]
struct HostDetectInput {
    baseline: HostInfo,
    current: HostInfo,
}

fuzz_target!(|input: HostDetectInput| {
    let forward = perfgate_host_detect::detect_host_mismatch(&input.baseline, &input.current);
    let reverse = perfgate_host_detect::detect_host_mismatch(&input.current, &input.baseline);

    // Symmetry invariant: if forward detects mismatch, reverse should too
    match (&forward, &reverse) {
        (None, None) => {}
        (Some(f), Some(r)) => {
            assert_eq!(f.reasons.len(), r.reasons.len());
        }
        _ => panic!(
            "symmetry violated: forward={:?}, reverse={:?}",
            forward, reverse
        ),
    }

    // Idempotence: same host should never mismatch
    assert!(
        perfgate_host_detect::detect_host_mismatch(&input.baseline, &input.baseline).is_none()
    );
    assert!(perfgate_host_detect::detect_host_mismatch(&input.current, &input.current).is_none());
});
