//! Profiler detection: probes the system for available profiling tools.

use crate::profiler::{
    CargoFlamegraphProfiler, DtraceProfiler, PerfProfiler, ProfileError, ProfileRequest,
    ProfileResult, Profiler,
};
use std::process::Command;

/// A profiler that was detected on the current system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedProfiler {
    /// Linux `perf` with `inferno-flamegraph` for SVG rendering.
    Perf,
    /// macOS `dtrace` with `inferno-flamegraph` for SVG rendering.
    Dtrace,
    /// Cross-platform `cargo flamegraph` wrapper.
    CargoFlamegraph,
}

impl DetectedProfiler {
    /// Human-readable name of the profiler.
    pub fn name(&self) -> &'static str {
        match self {
            DetectedProfiler::Perf => "perf + inferno",
            DetectedProfiler::Dtrace => "dtrace + inferno",
            DetectedProfiler::CargoFlamegraph => "cargo-flamegraph",
        }
    }

    /// Capture a flamegraph using this profiler.
    pub fn capture(&self, request: &ProfileRequest) -> Result<ProfileResult, ProfileError> {
        match self {
            DetectedProfiler::Perf => PerfProfiler.capture(request),
            DetectedProfiler::Dtrace => DtraceProfiler.capture(request),
            DetectedProfiler::CargoFlamegraph => CargoFlamegraphProfiler.capture(request),
        }
    }
}

/// Detect the best available profiler on the current system.
///
/// Priority order:
/// 1. `perf` + `inferno-flamegraph` (Linux)
/// 2. `dtrace` + `inferno-flamegraph` (macOS)
/// 3. `cargo flamegraph` (any platform)
/// 4. `None` if nothing is available
pub fn detect_profiler() -> Option<DetectedProfiler> {
    if cfg!(target_os = "linux") && is_command_available("perf") && has_inferno() {
        return Some(DetectedProfiler::Perf);
    }

    if cfg!(target_os = "macos") && is_command_available("dtrace") && has_inferno() {
        return Some(DetectedProfiler::Dtrace);
    }

    if is_command_available("cargo-flamegraph") || has_cargo_flamegraph_subcommand() {
        return Some(DetectedProfiler::CargoFlamegraph);
    }

    None
}

/// Check whether a binary is available on PATH.
fn is_command_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Check whether `inferno-flamegraph` is available (part of the `inferno` crate).
fn has_inferno() -> bool {
    is_command_available("inferno-flamegraph")
}

/// Check whether `cargo flamegraph` subcommand is available.
fn has_cargo_flamegraph_subcommand() -> bool {
    Command::new("cargo")
        .args(["flamegraph", "--help"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_profiler_name_is_human_readable() {
        assert_eq!(DetectedProfiler::Perf.name(), "perf + inferno");
        assert_eq!(DetectedProfiler::Dtrace.name(), "dtrace + inferno");
        assert_eq!(DetectedProfiler::CargoFlamegraph.name(), "cargo-flamegraph");
    }

    #[test]
    fn is_command_available_returns_false_for_nonexistent() {
        assert!(!is_command_available("perfgate-nonexistent-tool-xyz-12345"));
    }

    #[test]
    fn detect_profiler_does_not_panic() {
        // Just verify detection completes without panic on any platform.
        let _result = detect_profiler();
    }

    #[test]
    fn detected_profiler_variants_are_distinct() {
        assert_ne!(DetectedProfiler::Perf, DetectedProfiler::Dtrace);
        assert_ne!(DetectedProfiler::Perf, DetectedProfiler::CargoFlamegraph);
        assert_ne!(DetectedProfiler::Dtrace, DetectedProfiler::CargoFlamegraph);
    }
}
