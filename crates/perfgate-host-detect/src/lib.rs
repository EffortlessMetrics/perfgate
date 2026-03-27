//! Host mismatch detection for benchmarking noise reduction.
//!
//! This crate provides detection of host environment differences between
//! baseline and current benchmark runs. Host mismatches can introduce
//! significant noise into performance measurements, leading to false
//! positives or negatives in regression detection.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Example
//!
//! ```
//! use perfgate_host_detect::detect_host_mismatch;
//! use perfgate_types::HostInfo;
//!
//! let baseline = HostInfo {
//!     os: "linux".to_string(),
//!     arch: "x86_64".to_string(),
//!     cpu_count: Some(8),
//!     memory_bytes: Some(16 * 1024 * 1024 * 1024),
//!     hostname_hash: Some("abc123".to_string()),
//! };
//!
//! let current = HostInfo {
//!     os: "linux".to_string(),
//!     arch: "x86_64".to_string(),
//!     cpu_count: Some(8),
//!     memory_bytes: Some(16 * 1024 * 1024 * 1024),
//!     hostname_hash: Some("abc123".to_string()),
//! };
//!
//! assert!(detect_host_mismatch(&baseline, &current).is_none());
//! ```
//!
//! # Detection Criteria
//!
//! The function detects mismatches based on:
//!
//! - **OS mismatch**: Different operating systems (e.g., `linux` vs `windows`)
//! - **Architecture mismatch**: Different CPU architectures (e.g., `x86_64` vs `aarch64`)
//! - **CPU count**: Significant difference (> 2x) in logical CPU count
//! - **Memory**: Significant difference (> 2x) in total system memory
//! - **Hostname hash**: Different hashed hostnames (different machines)
//!
//! The 2x threshold for CPU and memory is chosen to avoid false positives
//! from minor variations (e.g., 8 vs 10 CPUs) while catching significant
//! differences (e.g., 4 vs 16 CPUs) that could affect benchmark results.

use perfgate_types::{HostInfo, HostMismatchInfo};

/// Detect host mismatches between baseline and current runs.
///
/// Returns `Some(HostMismatchInfo)` if any mismatch is detected, `None` otherwise.
///
/// # Detection Criteria
///
/// - Different `os` or `arch`
/// - Significant difference in `cpu_count` (> 2x)
/// - Significant difference in `memory_bytes` (> 2x)
/// - Different `hostname_hash` (if both present)
///
/// # Examples
///
/// Detect an OS mismatch (e.g., running benchmarks on a different platform):
///
/// ```
/// use perfgate_host_detect::detect_host_mismatch;
/// use perfgate_types::HostInfo;
///
/// let baseline = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,
///     memory_bytes: None,
///     hostname_hash: None,
/// };
///
/// let current = HostInfo {
///     os: "windows".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,
///     memory_bytes: None,
///     hostname_hash: None,
/// };
///
/// let mismatch = detect_host_mismatch(&baseline, &current);
/// assert!(mismatch.is_some());
/// assert!(mismatch.unwrap().reasons[0].contains("OS mismatch"));
/// ```
///
/// Detect an architecture mismatch (e.g., `x86_64` vs `aarch64`):
///
/// ```
/// # use perfgate_host_detect::detect_host_mismatch;
/// # use perfgate_types::HostInfo;
/// let baseline = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,
///     memory_bytes: None,
///     hostname_hash: None,
/// };
/// let current = HostInfo {
///     os: "linux".to_string(),
///     arch: "aarch64".to_string(),
///     cpu_count: None,
///     memory_bytes: None,
///     hostname_hash: None,
/// };
///
/// let mismatch = detect_host_mismatch(&baseline, &current).unwrap();
/// assert!(mismatch.reasons[0].contains("architecture mismatch"));
/// ```
///
/// Detect significant CPU count differences (> 2x ratio indicates a
/// different cloud instance type or machine class):
///
/// ```
/// # use perfgate_host_detect::detect_host_mismatch;
/// # use perfgate_types::HostInfo;
/// let baseline = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: Some(4),
///     memory_bytes: None,
///     hostname_hash: None,
/// };
/// let current = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: Some(32),
///     memory_bytes: None,
///     hostname_hash: None,
/// };
///
/// let mismatch = detect_host_mismatch(&baseline, &current).unwrap();
/// assert!(mismatch.reasons[0].contains("CPU count differs"));
/// ```
///
/// Minor CPU differences (≤ 2x) are ignored to reduce false positives:
///
/// ```
/// # use perfgate_host_detect::detect_host_mismatch;
/// # use perfgate_types::HostInfo;
/// let baseline = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: Some(8),
///     memory_bytes: None,
///     hostname_hash: None,
/// };
/// let current = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: Some(16),
///     memory_bytes: None,
///     hostname_hash: None,
/// };
///
/// // Exactly 2x is still within tolerance
/// assert!(detect_host_mismatch(&baseline, &current).is_none());
/// ```
///
/// Detect significant memory differences (different cloud instance sizes):
///
/// ```
/// # use perfgate_host_detect::detect_host_mismatch;
/// # use perfgate_types::HostInfo;
/// let baseline = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,
///     memory_bytes: Some(8 * 1024 * 1024 * 1024),   // 8 GB
///     hostname_hash: None,
/// };
/// let current = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,
///     memory_bytes: Some(64 * 1024 * 1024 * 1024),  // 64 GB
///     hostname_hash: None,
/// };
///
/// let mismatch = detect_host_mismatch(&baseline, &current).unwrap();
/// assert!(mismatch.reasons[0].contains("memory differs"));
/// ```
///
/// Detect hostname hash mismatch (benchmarks ran on different machines):
///
/// ```
/// # use perfgate_host_detect::detect_host_mismatch;
/// # use perfgate_types::HostInfo;
/// let baseline = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,
///     memory_bytes: None,
///     hostname_hash: Some("abc123".to_string()),
/// };
/// let current = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,
///     memory_bytes: None,
///     hostname_hash: Some("def456".to_string()),
/// };
///
/// let mismatch = detect_host_mismatch(&baseline, &current).unwrap();
/// assert!(mismatch.reasons[0].contains("hostname mismatch"));
/// ```
///
/// Optional fields that are `None` on either side are silently skipped:
///
/// ```
/// # use perfgate_host_detect::detect_host_mismatch;
/// # use perfgate_types::HostInfo;
/// let baseline = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: Some(4),
///     memory_bytes: Some(16 * 1024 * 1024 * 1024),
///     hostname_hash: Some("abc".to_string()),
/// };
/// let current = HostInfo {
///     os: "linux".to_string(),
///     arch: "x86_64".to_string(),
///     cpu_count: None,   // unknown — skipped
///     memory_bytes: None, // unknown — skipped
///     hostname_hash: None, // unknown — skipped
/// };
///
/// assert!(detect_host_mismatch(&baseline, &current).is_none());
/// ```
pub fn detect_host_mismatch(baseline: &HostInfo, current: &HostInfo) -> Option<HostMismatchInfo> {
    let mut reasons = Vec::new();

    if baseline.os != current.os {
        reasons.push(format!(
            "OS mismatch: baseline={}, current={}",
            baseline.os, current.os
        ));
    }

    if baseline.arch != current.arch {
        reasons.push(format!(
            "architecture mismatch: baseline={}, current={}",
            baseline.arch, current.arch
        ));
    }

    if let (Some(base_cpu), Some(curr_cpu)) = (baseline.cpu_count, current.cpu_count) {
        let ratio = if base_cpu > 0 && curr_cpu > 0 {
            (base_cpu as f64 / curr_cpu as f64).max(curr_cpu as f64 / base_cpu as f64)
        } else {
            1.0
        };
        if ratio > 2.0 {
            reasons.push(format!(
                "CPU count differs significantly: baseline={}, current={} ({:.1}x)",
                base_cpu, curr_cpu, ratio
            ));
        }
    }

    if let (Some(base_mem), Some(curr_mem)) = (baseline.memory_bytes, current.memory_bytes) {
        let ratio = if base_mem > 0 && curr_mem > 0 {
            (base_mem as f64 / curr_mem as f64).max(curr_mem as f64 / base_mem as f64)
        } else {
            1.0
        };
        if ratio > 2.0 {
            let base_gb = base_mem as f64 / (1024.0 * 1024.0 * 1024.0);
            let curr_gb = curr_mem as f64 / (1024.0 * 1024.0 * 1024.0);
            reasons.push(format!(
                "memory differs significantly: baseline={:.1}GB, current={:.1}GB ({:.1}x)",
                base_gb, curr_gb, ratio
            ));
        }
    }

    if let (Some(base_hash), Some(curr_hash)) = (&baseline.hostname_hash, &current.hostname_hash)
        && base_hash != curr_hash
    {
        reasons.push("hostname mismatch (different machines)".to_string());
    }

    if reasons.is_empty() {
        None
    } else {
        Some(HostMismatchInfo { reasons })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_host_info(os: &str, arch: &str) -> HostInfo {
        HostInfo {
            os: os.to_string(),
            arch: arch.to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        }
    }

    #[test]
    fn no_mismatch_when_identical() {
        let baseline = make_host_info("linux", "x86_64");
        let current = make_host_info("linux", "x86_64");
        assert!(detect_host_mismatch(&baseline, &current).is_none());
    }

    #[test]
    fn detects_os_mismatch() {
        let baseline = make_host_info("linux", "x86_64");
        let current = make_host_info("windows", "x86_64");
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
        let reasons = mismatch.unwrap().reasons;
        assert!(reasons.iter().any(|r| r.contains("OS mismatch")));
        assert!(reasons.iter().any(|r| r.contains("baseline=linux")));
        assert!(reasons.iter().any(|r| r.contains("current=windows")));
    }

    #[test]
    fn detects_arch_mismatch() {
        let baseline = make_host_info("linux", "x86_64");
        let current = make_host_info("linux", "aarch64");
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
        let reasons = mismatch.unwrap().reasons;
        assert!(reasons.iter().any(|r| r.contains("architecture mismatch")));
        assert!(reasons.iter().any(|r| r.contains("baseline=x86_64")));
        assert!(reasons.iter().any(|r| r.contains("current=aarch64")));
    }

    #[test]
    fn detects_cpu_count_significant_difference() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(4);
        current.cpu_count = Some(16);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
        let reasons = mismatch.unwrap().reasons;
        assert!(reasons.iter().any(|r| r.contains("CPU count differs")));
        assert!(reasons.iter().any(|r| r.contains("4.0x")));
    }

    #[test]
    fn ignores_cpu_count_minor_difference() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(8);
        current.cpu_count = Some(12);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn cpu_count_at_exact_2x_threshold_is_not_mismatch() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(4);
        current.cpu_count = Some(8);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn cpu_count_just_over_2x_is_mismatch() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(4);
        current.cpu_count = Some(9);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
        let reasons = mismatch.unwrap().reasons;
        assert!(reasons.iter().any(|r| r.contains("CPU count differs")));
    }

    #[test]
    fn detects_memory_significant_difference() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.memory_bytes = Some(8 * 1024 * 1024 * 1024);
        current.memory_bytes = Some(32 * 1024 * 1024 * 1024);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
        let reasons = mismatch.unwrap().reasons;
        assert!(reasons.iter().any(|r| r.contains("memory differs")));
        assert!(reasons.iter().any(|r| r.contains("8.0GB")));
        assert!(reasons.iter().any(|r| r.contains("32.0GB")));
    }

    #[test]
    fn ignores_memory_minor_difference() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.memory_bytes = Some(16 * 1024 * 1024 * 1024);
        current.memory_bytes = Some(24 * 1024 * 1024 * 1024);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn memory_at_exact_2x_threshold_is_not_mismatch() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.memory_bytes = Some(8 * 1024 * 1024 * 1024);
        current.memory_bytes = Some(16 * 1024 * 1024 * 1024);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn detects_hostname_difference() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.hostname_hash = Some("abc123".to_string());
        current.hostname_hash = Some("def456".to_string());
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
        let reasons = mismatch.unwrap().reasons;
        assert!(reasons.iter().any(|r| r.contains("hostname mismatch")));
    }

    #[test]
    fn ignores_hostname_when_only_baseline_has_it() {
        let mut baseline = make_host_info("linux", "x86_64");
        let current = make_host_info("linux", "x86_64");
        baseline.hostname_hash = Some("abc123".to_string());
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn ignores_hostname_when_only_current_has_it() {
        let baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        current.hostname_hash = Some("def456".to_string());
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn ignores_hostname_when_both_are_none() {
        let baseline = make_host_info("linux", "x86_64");
        let current = make_host_info("linux", "x86_64");
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn same_hostname_hash_is_not_mismatch() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.hostname_hash = Some("abc123".to_string());
        current.hostname_hash = Some("abc123".to_string());
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn detects_multiple_simultaneous_mismatches() {
        let baseline = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: Some(4),
            memory_bytes: Some(8 * 1024 * 1024 * 1024),
            hostname_hash: Some("abc".to_string()),
        };
        let current = HostInfo {
            os: "windows".to_string(),
            arch: "aarch64".to_string(),
            cpu_count: Some(32),
            memory_bytes: Some(64 * 1024 * 1024 * 1024),
            hostname_hash: Some("def".to_string()),
        };
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
        let reasons = mismatch.unwrap().reasons;
        assert_eq!(reasons.len(), 5);
    }

    #[test]
    fn partial_fields_none_handling_cpu() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(4);
        current.cpu_count = None;
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn partial_fields_none_handling_memory() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.memory_bytes = None;
        current.memory_bytes = Some(32 * 1024 * 1024 * 1024);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn zero_cpu_count_is_handled_gracefully() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(0);
        current.cpu_count = Some(8);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn zero_memory_is_handled_gracefully() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.memory_bytes = Some(0);
        current.memory_bytes = Some(32 * 1024 * 1024 * 1024);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_none());
    }

    #[test]
    fn cpu_count_ratio_works_both_directions() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");

        baseline.cpu_count = Some(16);
        current.cpu_count = Some(4);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
    }

    #[test]
    fn memory_ratio_works_both_directions() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");

        baseline.memory_bytes = Some(64 * 1024 * 1024 * 1024);
        current.memory_bytes = Some(8 * 1024 * 1024 * 1024);
        let mismatch = detect_host_mismatch(&baseline, &current);
        assert!(mismatch.is_some());
    }

    #[test]
    fn identical_fully_populated_no_mismatch() {
        let host = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: Some(8),
            memory_bytes: Some(16 * 1024 * 1024 * 1024),
            hostname_hash: Some("abc123def456".to_string()),
        };
        assert!(detect_host_mismatch(&host, &host.clone()).is_none());
    }

    #[test]
    fn equal_cpu_count_no_mismatch() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(16);
        current.cpu_count = Some(16);
        assert!(detect_host_mismatch(&baseline, &current).is_none());
    }

    #[test]
    fn equal_memory_no_mismatch() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.memory_bytes = Some(32 * 1024 * 1024 * 1024);
        current.memory_bytes = Some(32 * 1024 * 1024 * 1024);
        assert!(detect_host_mismatch(&baseline, &current).is_none());
    }

    #[test]
    fn os_mismatch_reason_contains_both_values() {
        let baseline = make_host_info("macos", "x86_64");
        let current = make_host_info("linux", "x86_64");
        let reasons = detect_host_mismatch(&baseline, &current).unwrap().reasons;
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("macos"));
        assert!(reasons[0].contains("linux"));
    }

    #[test]
    fn arch_mismatch_reason_contains_both_values() {
        let baseline = make_host_info("linux", "arm64");
        let current = make_host_info("linux", "x86_64");
        let reasons = detect_host_mismatch(&baseline, &current).unwrap().reasons;
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("arm64"));
        assert!(reasons[0].contains("x86_64"));
    }

    #[test]
    fn cpu_mismatch_reason_contains_counts_and_ratio() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(2);
        current.cpu_count = Some(8);
        let reasons = detect_host_mismatch(&baseline, &current).unwrap().reasons;
        assert!(reasons[0].contains("baseline=2"));
        assert!(reasons[0].contains("current=8"));
        assert!(reasons[0].contains("4.0x"));
    }

    #[test]
    fn hostname_mismatch_only_one_reason() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.hostname_hash = Some("aaa".to_string());
        current.hostname_hash = Some("bbb".to_string());
        let reasons = detect_host_mismatch(&baseline, &current).unwrap().reasons;
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("hostname mismatch"));
    }

    #[test]
    fn multiple_mismatches_os_and_arch() {
        let baseline = make_host_info("linux", "x86_64");
        let current = make_host_info("windows", "aarch64");
        let reasons = detect_host_mismatch(&baseline, &current).unwrap().reasons;
        assert_eq!(reasons.len(), 2);
        assert!(reasons.iter().any(|r| r.contains("OS mismatch")));
        assert!(reasons.iter().any(|r| r.contains("architecture mismatch")));
    }

    #[test]
    fn all_none_optional_fields_no_mismatch() {
        let baseline = HostInfo {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            cpu_count: None,
            memory_bytes: None,
            hostname_hash: None,
        };
        let current = baseline.clone();
        assert!(detect_host_mismatch(&baseline, &current).is_none());
    }

    #[test]
    fn both_zero_cpu_and_zero_memory_no_mismatch() {
        let mut baseline = make_host_info("linux", "x86_64");
        let mut current = make_host_info("linux", "x86_64");
        baseline.cpu_count = Some(0);
        current.cpu_count = Some(0);
        baseline.memory_bytes = Some(0);
        current.memory_bytes = Some(0);
        assert!(detect_host_mismatch(&baseline, &current).is_none());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    fn host_info_strategy() -> impl Strategy<Value = HostInfo> {
        (
            "[a-z]{3,10}",
            "[a-z0-9_]{3,10}",
            proptest::option::of(1u32..256u32),
            proptest::option::of(1u64..68719476736u64),
            proptest::option::of("[a-f0-9]{16}"),
        )
            .prop_map(
                |(os, arch, cpu_count, memory_bytes, hostname_hash)| HostInfo {
                    os,
                    arch,
                    cpu_count,
                    memory_bytes,
                    hostname_hash,
                },
            )
    }

    proptest! {
        #[test]
        fn idempotence_same_host_returns_none(host in host_info_strategy()) {
            prop_assert!(detect_host_mismatch(&host, &host).is_none());
        }

        #[test]
        fn symmetry_detect_a_b_implies_detect_b_a(
            baseline in host_info_strategy(),
            current in host_info_strategy()
        ) {
            let forward = detect_host_mismatch(&baseline, &current);
            let reverse = detect_host_mismatch(&current, &baseline);

            match (&forward, &reverse) {
                (None, None) => prop_assert!(true),
                (Some(f), Some(r)) => {
                    prop_assert_eq!(f.reasons.len(), r.reasons.len());
                }
                _ => prop_assert!(false, "symmetry violated: forward={:?}, reverse={:?}", forward, reverse),
            }
        }

        #[test]
        fn os_difference_always_detected(
            os1 in "[a-z]{3,10}",
            os2 in "[a-z]{3,10}",
            arch in "[a-z0-9_]{3,10}"
        ) {
            prop_assume!(os1 != os2);
            let baseline = HostInfo {
                os: os1.clone(),
                arch: arch.clone(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            };
            let current = HostInfo {
                os: os2.clone(),
                arch,
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            };
            let mismatch = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch.is_some());
            prop_assert!(mismatch.unwrap().reasons.iter().any(|r| r.contains("OS mismatch")));
        }

        #[test]
        fn arch_difference_always_detected(
            arch1 in "[a-z0-9_]{3,10}",
            arch2 in "[a-z0-9_]{3,10}",
            os in "[a-z]{3,10}"
        ) {
            prop_assume!(arch1 != arch2);
            let baseline = HostInfo {
                os: os.clone(),
                arch: arch1.clone(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            };
            let current = HostInfo {
                os,
                arch: arch2.clone(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            };
            let mismatch = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch.is_some());
            prop_assert!(mismatch.unwrap().reasons.iter().any(|r| r.contains("architecture mismatch")));
        }

        #[test]
        fn cpu_count_2x_plus_1_always_detected(
            small_cpu in 1u32..100u32,
        ) {
            let large_cpu = small_cpu * 2 + 1;
            let mut baseline = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(small_cpu),
                memory_bytes: None,
                hostname_hash: None,
            };
            let mut current = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(large_cpu),
                memory_bytes: None,
                hostname_hash: None,
            };

            let mismatch_forward = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch_forward.is_some());

            std::mem::swap(&mut baseline.cpu_count, &mut current.cpu_count);
            let mismatch_reverse = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch_reverse.is_some());
        }

        #[test]
        fn cpu_count_exact_2x_not_detected(
            cpu in 1u32..100u32,
        ) {
            let baseline = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(cpu),
                memory_bytes: None,
                hostname_hash: None,
            };
            let current = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: Some(cpu * 2),
                memory_bytes: None,
                hostname_hash: None,
            };
            let mismatch = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch.is_none());
        }

        #[test]
        fn memory_2x_plus_1gb_always_detected(
            small_mem_gb in 1u64..32u64,
        ) {
            let small_mem = small_mem_gb * 1024 * 1024 * 1024;
            let large_mem = small_mem * 2 + (1024 * 1024 * 1024);
            let mut baseline = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: Some(small_mem),
                hostname_hash: None,
            };
            let mut current = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: Some(large_mem),
                hostname_hash: None,
            };

            let mismatch_forward = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch_forward.is_some());

            std::mem::swap(&mut baseline.memory_bytes, &mut current.memory_bytes);
            let mismatch_reverse = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch_reverse.is_some());
        }

        #[test]
        fn hostname_hash_difference_detected_when_both_present(
            hash1 in "[a-f0-9]{16}",
            hash2 in "[a-f0-9]{16}"
        ) {
            prop_assume!(hash1 != hash2);
            let baseline = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: Some(hash1),
            };
            let current = HostInfo {
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: Some(hash2),
            };
            let mismatch = detect_host_mismatch(&baseline, &current);
            prop_assert!(mismatch.is_some());
            prop_assert!(mismatch.unwrap().reasons.iter().any(|r| r.contains("hostname mismatch")));
        }

        #[test]
        fn none_fields_do_not_cause_mismatch(
            host in host_info_strategy()
        ) {
            let minimal = HostInfo {
                os: host.os.clone(),
                arch: host.arch.clone(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            };
            let mismatch = detect_host_mismatch(&host, &minimal);
            prop_assert!(mismatch.is_none());
        }
    }
}
