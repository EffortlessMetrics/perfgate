//! Profiler trait and concrete implementations for flamegraph capture.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Errors that can occur during profiling.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("profiler command failed: {command}: {reason}")]
    CommandFailed { command: String, reason: String },

    #[error("failed to create output directory {path}: {reason}")]
    CreateDir { path: String, reason: String },

    #[error("failed to write flamegraph SVG to {path}: {reason}")]
    WriteSvg { path: String, reason: String },

    #[error("profiler produced no output")]
    NoOutput,
}

/// Request to capture a flamegraph.
#[derive(Debug, Clone)]
pub struct ProfileRequest {
    /// The command to profile (argv).
    pub command: Vec<String>,

    /// Directory where the flamegraph SVG should be written.
    pub output_dir: PathBuf,

    /// Label for the flamegraph file (used in the filename).
    pub label: String,

    /// Optional working directory for the profiled command.
    pub cwd: Option<PathBuf>,

    /// Environment variables for the profiled command.
    pub env: Vec<(String, String)>,
}

/// Result of a successful flamegraph capture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResult {
    /// Path to the generated flamegraph SVG file.
    pub svg_path: PathBuf,

    /// Which profiler was used.
    pub profiler_used: String,

    /// How long the profiling took in milliseconds.
    pub duration_ms: u64,
}

/// Trait for profiler implementations.
pub trait Profiler {
    /// Capture a flamegraph for the given command.
    fn capture(&self, request: &ProfileRequest) -> Result<ProfileResult, ProfileError>;
}

/// Ensure the output directory exists.
fn ensure_output_dir(dir: &Path) -> Result<(), ProfileError> {
    std::fs::create_dir_all(dir).map_err(|e| ProfileError::CreateDir {
        path: dir.display().to_string(),
        reason: e.to_string(),
    })
}

/// Build the SVG output path from the request.
fn svg_output_path(request: &ProfileRequest) -> PathBuf {
    let sanitized_label = request
        .label
        .replace(|c: char| !c.is_alphanumeric() && c != '-' && c != '_', "_");
    request
        .output_dir
        .join(format!("flamegraph-{sanitized_label}.svg"))
}

/// Linux `perf record` + `inferno-flamegraph` profiler.
pub struct PerfProfiler;

impl Profiler for PerfProfiler {
    fn capture(&self, request: &ProfileRequest) -> Result<ProfileResult, ProfileError> {
        ensure_output_dir(&request.output_dir)?;
        let svg_path = svg_output_path(request);
        let start = Instant::now();

        // Step 1: perf record
        let perf_data = request.output_dir.join("perf.data");
        let mut perf_cmd = Command::new("perf");
        perf_cmd.args([
            "record",
            "-g",
            "--call-graph",
            "dwarf",
            "-o",
            &perf_data.display().to_string(),
            "--",
        ]);
        perf_cmd.args(&request.command);

        if let Some(cwd) = &request.cwd {
            perf_cmd.current_dir(cwd);
        }
        for (k, v) in &request.env {
            perf_cmd.env(k, v);
        }

        let perf_output = perf_cmd.output().map_err(|e| ProfileError::CommandFailed {
            command: "perf record".to_string(),
            reason: e.to_string(),
        })?;

        if !perf_output.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "perf record".to_string(),
                reason: String::from_utf8_lossy(&perf_output.stderr).to_string(),
            });
        }

        // Step 2: perf script | inferno-collapse-perf | inferno-flamegraph > svg
        let perf_script = Command::new("perf")
            .args(["script", "-i", &perf_data.display().to_string()])
            .output()
            .map_err(|e| ProfileError::CommandFailed {
                command: "perf script".to_string(),
                reason: e.to_string(),
            })?;

        if !perf_script.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "perf script".to_string(),
                reason: String::from_utf8_lossy(&perf_script.stderr).to_string(),
            });
        }

        let mut collapse = Command::new("inferno-collapse-perf")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ProfileError::CommandFailed {
                command: "inferno-collapse-perf".to_string(),
                reason: e.to_string(),
            })?;

        // Write perf script output to collapse stdin, then drop to close pipe
        use std::io::Write;
        if let Some(ref mut stdin) = collapse.stdin {
            stdin
                .write_all(&perf_script.stdout)
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-collapse-perf (write stdin)".to_string(),
                    reason: e.to_string(),
                })?;
        }
        collapse.stdin.take(); // close stdin so child sees EOF

        let collapse_output =
            collapse
                .wait_with_output()
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-collapse-perf".to_string(),
                    reason: e.to_string(),
                })?;

        if !collapse_output.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "inferno-collapse-perf".to_string(),
                reason: String::from_utf8_lossy(&collapse_output.stderr).to_string(),
            });
        }

        let mut flamegraph = Command::new("inferno-flamegraph")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ProfileError::CommandFailed {
                command: "inferno-flamegraph".to_string(),
                reason: e.to_string(),
            })?;

        if let Some(ref mut stdin) = flamegraph.stdin {
            stdin
                .write_all(&collapse_output.stdout)
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-flamegraph (write stdin)".to_string(),
                    reason: e.to_string(),
                })?;
        }
        flamegraph.stdin.take(); // close stdin so child sees EOF

        let flamegraph_output =
            flamegraph
                .wait_with_output()
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-flamegraph".to_string(),
                    reason: e.to_string(),
                })?;

        if !flamegraph_output.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "inferno-flamegraph".to_string(),
                reason: String::from_utf8_lossy(&flamegraph_output.stderr).to_string(),
            });
        }

        if flamegraph_output.stdout.is_empty() {
            return Err(ProfileError::NoOutput);
        }

        std::fs::write(&svg_path, &flamegraph_output.stdout).map_err(|e| {
            ProfileError::WriteSvg {
                path: svg_path.display().to_string(),
                reason: e.to_string(),
            }
        })?;

        // Clean up perf.data
        let _ = std::fs::remove_file(&perf_data);

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ProfileResult {
            svg_path,
            profiler_used: "perf + inferno".to_string(),
            duration_ms,
        })
    }
}

/// macOS `dtrace` + `inferno-flamegraph` profiler.
pub struct DtraceProfiler;

impl Profiler for DtraceProfiler {
    fn capture(&self, request: &ProfileRequest) -> Result<ProfileResult, ProfileError> {
        ensure_output_dir(&request.output_dir)?;
        let svg_path = svg_output_path(request);
        let start = Instant::now();

        let stacks_path = request.output_dir.join("dtrace-stacks.txt");

        // Build the dtrace probe script: profile user-space stacks at 997 Hz
        let command_str = request.command.join(" ");
        let dtrace_script = format!(
            "profile-997 /execname == \"{}\"/ {{ @[ustack(100)] = count(); }}",
            request
                .command
                .first()
                .map(|s| s.as_str())
                .unwrap_or("unknown")
        );

        // Run the command in background, capture its PID, then dtrace
        // Alternative: use dtrace -c to run the command directly
        let mut dtrace_cmd = Command::new("dtrace");
        dtrace_cmd.args([
            "-x",
            "ustackframes=100",
            "-n",
            &dtrace_script,
            "-c",
            &command_str,
            "-o",
            &stacks_path.display().to_string(),
        ]);

        if let Some(cwd) = &request.cwd {
            dtrace_cmd.current_dir(cwd);
        }
        for (k, v) in &request.env {
            dtrace_cmd.env(k, v);
        }

        let dtrace_output = dtrace_cmd
            .output()
            .map_err(|e| ProfileError::CommandFailed {
                command: "dtrace".to_string(),
                reason: e.to_string(),
            })?;

        if !dtrace_output.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "dtrace".to_string(),
                reason: String::from_utf8_lossy(&dtrace_output.stderr).to_string(),
            });
        }

        // Collapse dtrace stacks using inferno-collapse-dtrace
        let stacks_data = std::fs::read(&stacks_path).map_err(|e| ProfileError::CommandFailed {
            command: "read dtrace stacks".to_string(),
            reason: e.to_string(),
        })?;

        let mut collapse = Command::new("inferno-collapse-dtrace")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ProfileError::CommandFailed {
                command: "inferno-collapse-dtrace".to_string(),
                reason: e.to_string(),
            })?;

        use std::io::Write;
        if let Some(ref mut stdin) = collapse.stdin {
            stdin
                .write_all(&stacks_data)
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-collapse-dtrace (write stdin)".to_string(),
                    reason: e.to_string(),
                })?;
        }
        collapse.stdin.take(); // close stdin so child sees EOF

        let collapse_output =
            collapse
                .wait_with_output()
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-collapse-dtrace".to_string(),
                    reason: e.to_string(),
                })?;

        if !collapse_output.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "inferno-collapse-dtrace".to_string(),
                reason: String::from_utf8_lossy(&collapse_output.stderr).to_string(),
            });
        }

        let mut flamegraph = Command::new("inferno-flamegraph")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ProfileError::CommandFailed {
                command: "inferno-flamegraph".to_string(),
                reason: e.to_string(),
            })?;

        if let Some(ref mut stdin) = flamegraph.stdin {
            stdin
                .write_all(&collapse_output.stdout)
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-flamegraph (write stdin)".to_string(),
                    reason: e.to_string(),
                })?;
        }
        flamegraph.stdin.take(); // close stdin so child sees EOF

        let flamegraph_output =
            flamegraph
                .wait_with_output()
                .map_err(|e| ProfileError::CommandFailed {
                    command: "inferno-flamegraph".to_string(),
                    reason: e.to_string(),
                })?;

        if !flamegraph_output.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "inferno-flamegraph".to_string(),
                reason: String::from_utf8_lossy(&flamegraph_output.stderr).to_string(),
            });
        }

        if flamegraph_output.stdout.is_empty() {
            return Err(ProfileError::NoOutput);
        }

        std::fs::write(&svg_path, &flamegraph_output.stdout).map_err(|e| {
            ProfileError::WriteSvg {
                path: svg_path.display().to_string(),
                reason: e.to_string(),
            }
        })?;

        // Clean up intermediate files
        let _ = std::fs::remove_file(&stacks_path);

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ProfileResult {
            svg_path,
            profiler_used: "dtrace + inferno".to_string(),
            duration_ms,
        })
    }
}

/// Cross-platform `cargo flamegraph` profiler.
pub struct CargoFlamegraphProfiler;

impl Profiler for CargoFlamegraphProfiler {
    fn capture(&self, request: &ProfileRequest) -> Result<ProfileResult, ProfileError> {
        ensure_output_dir(&request.output_dir)?;
        let svg_path = svg_output_path(request);
        let start = Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.args([
            "flamegraph",
            "--output",
            &svg_path.display().to_string(),
            "--",
        ]);
        cmd.args(&request.command);

        if let Some(cwd) = &request.cwd {
            cmd.current_dir(cwd);
        }
        for (k, v) in &request.env {
            cmd.env(k, v);
        }

        let output = cmd.output().map_err(|e| ProfileError::CommandFailed {
            command: "cargo flamegraph".to_string(),
            reason: e.to_string(),
        })?;

        if !output.status.success() {
            return Err(ProfileError::CommandFailed {
                command: "cargo flamegraph".to_string(),
                reason: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        if !svg_path.exists() {
            return Err(ProfileError::NoOutput);
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ProfileResult {
            svg_path,
            profiler_used: "cargo-flamegraph".to_string(),
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_output_path_sanitizes_label() {
        let request = ProfileRequest {
            command: vec!["echo".to_string()],
            output_dir: PathBuf::from("/tmp/profiles"),
            label: "my bench/test:1".to_string(),
            cwd: None,
            env: Vec::new(),
        };
        let path = svg_output_path(&request);
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(filename, "flamegraph-my_bench_test_1.svg");
    }

    #[test]
    fn svg_output_path_preserves_valid_chars() {
        let request = ProfileRequest {
            command: vec!["echo".to_string()],
            output_dir: PathBuf::from("/tmp/profiles"),
            label: "bench-name_v2".to_string(),
            cwd: None,
            env: Vec::new(),
        };
        let path = svg_output_path(&request);
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(filename, "flamegraph-bench-name_v2.svg");
    }

    #[test]
    fn profile_error_display() {
        let err = ProfileError::CommandFailed {
            command: "perf record".to_string(),
            reason: "not found".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "profiler command failed: perf record: not found"
        );

        let err = ProfileError::NoOutput;
        assert_eq!(err.to_string(), "profiler produced no output");
    }

    #[test]
    fn profile_result_serialization_roundtrip() {
        let result = ProfileResult {
            svg_path: PathBuf::from("/tmp/profiles/flamegraph-bench.svg"),
            profiler_used: "perf + inferno".to_string(),
            duration_ms: 1234,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: ProfileResult = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.profiler_used, "perf + inferno");
        assert_eq!(deserialized.duration_ms, 1234);
    }

    #[test]
    fn ensure_output_dir_creates_nested_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("a").join("b").join("c");
        assert!(!nested.exists());
        ensure_output_dir(&nested).unwrap();
        assert!(nested.exists());
    }

    #[test]
    fn ensure_output_dir_succeeds_if_exists() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_output_dir(tmp.path()).unwrap();
    }
}
