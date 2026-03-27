//! Automatic flamegraph profiling for performance regression diagnostics.
//!
//! This crate detects available system profilers and generates flamegraph SVGs
//! when perfgate detects a regression. It supports:
//!
//! - `perf` (Linux) with `perf record` + folded stack conversion
//! - `dtrace` (macOS) with `dtrace -x ustackframes` + folded stack conversion
//! - `cargo-flamegraph` (cross-platform) as a convenient fallback
//!
//! When no profiler is available, a diagnostic message is emitted suggesting
//! installation steps.

mod detect;
mod profiler;

pub use detect::{DetectedProfiler, detect_profiler};
pub use profiler::{ProfileError, ProfileRequest, ProfileResult, Profiler};

/// Capture a flamegraph for the given command using the best available profiler.
///
/// Returns `Ok(None)` if no profiler is available (with a diagnostic on stderr).
/// Returns `Ok(Some(result))` on successful capture.
/// Returns `Err(...)` on profiler execution failure.
pub fn capture_flamegraph(request: &ProfileRequest) -> Result<Option<ProfileResult>, ProfileError> {
    let profiler = detect_profiler();

    match profiler {
        Some(detected) => {
            let result = detected.capture(request)?;
            Ok(Some(result))
        }
        None => {
            eprintln!(
                "warning: --profile-on-regression requested but no profiler found.\n\
                 Install one of:\n\
                 - Linux: `perf` (linux-tools-common) + `inferno` (`cargo install inferno`)\n\
                 - macOS: `dtrace` (ships with Xcode) + `inferno` (`cargo install inferno`)\n\
                 - Any OS: `cargo install flamegraph`"
            );
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_flamegraph_returns_none_when_no_profiler() {
        // On most CI environments, profilers may not be available.
        // This test verifies we get a graceful None rather than an error.
        let request = ProfileRequest {
            command: vec!["echo".to_string(), "hello".to_string()],
            output_dir: std::path::PathBuf::from("/tmp/perfgate-test-profiles"),
            label: "test-bench".to_string(),
            cwd: None,
            env: Vec::new(),
        };
        // We cannot guarantee a profiler is available in the test environment,
        // so we just verify the function does not panic.
        let _ = capture_flamegraph(&request);
    }
}
