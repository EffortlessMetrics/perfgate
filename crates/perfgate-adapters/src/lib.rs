//! Std adapters for perfgate.
//!
//! In clean-arch terms: this is where we touch the world.

use anyhow::Context;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub timeout: Option<Duration>,
    pub output_cap_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct RunResult {
    pub wall_ms: u64,
    pub exit_code: i32,
    pub timed_out: bool,
    pub max_rss_kb: Option<u64>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    #[error("command argv must not be empty")]
    EmptyArgv,

    #[error("command timed out")]
    Timeout,

    #[error("timeout is not supported on this platform")]
    TimeoutUnsupported,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub trait ProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<RunResult, AdapterError>;
}

#[derive(Debug, Default, Clone)]
pub struct StdProcessRunner;

impl ProcessRunner for StdProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<RunResult, AdapterError> {
        if spec.argv.is_empty() {
            return Err(AdapterError::EmptyArgv);
        }

        #[cfg(unix)]
        {
            return run_unix(spec);
        }

        #[cfg(not(unix))]
        {
            if spec.timeout.is_some() {
                return Err(AdapterError::TimeoutUnsupported);
            }
            run_portable(spec)
        }
    }
}

fn truncate(mut bytes: Vec<u8>, cap: usize) -> Vec<u8> {
    if bytes.len() > cap {
        bytes.truncate(cap);
    }
    bytes
}

#[cfg(not(unix))]
fn run_portable(spec: &CommandSpec) -> Result<RunResult, AdapterError> {
    use std::process::Command;

    let start = Instant::now();
    let mut cmd = Command::new(&spec.argv[0]);
    if spec.argv.len() > 1 {
        cmd.args(&spec.argv[1..]);
    }

    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }

    for (k, v) in &spec.env {
        cmd.env(k, v);
    }

    let out = cmd
        .output()
        .with_context(|| format!("failed to run {:?}", spec.argv))
        .map_err(AdapterError::Other)?;

    let wall_ms = start.elapsed().as_millis() as u64;
    let exit_code = out.status.code().unwrap_or(-1);

    Ok(RunResult {
        wall_ms,
        exit_code,
        timed_out: false,
        max_rss_kb: None,
        stdout: truncate(out.stdout, spec.output_cap_bytes),
        stderr: truncate(out.stderr, spec.output_cap_bytes),
    })
}

#[cfg(unix)]
fn run_unix(spec: &CommandSpec) -> Result<RunResult, AdapterError> {
    use std::io::Read;
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    use std::thread;

    let start = Instant::now();

    let mut cmd = Command::new(&spec.argv[0]);
    if spec.argv.len() > 1 {
        cmd.args(&spec.argv[1..]);
    }

    if let Some(cwd) = &spec.cwd {
        cmd.current_dir(cwd);
    }

    for (k, v) in &spec.env {
        cmd.env(k, v);
    }

    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn {:?}", spec.argv))
        .map_err(AdapterError::Other)?;

    let pid = child.id() as libc::pid_t;

    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");

    let cap = spec.output_cap_bytes;

    let out_handle = thread::spawn(move || read_with_cap(&mut stdout, cap));
    let err_handle = thread::spawn(move || read_with_cap(&mut stderr, cap));

    let (status_raw, rusage, timed_out) = wait4_with_timeout(pid, spec.timeout)?;

    // Safety: we have reaped the child via wait4; drop the Child handle without waiting.
    drop(child);

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();

    let wall_ms = start.elapsed().as_millis() as u64;

    let exit_status = std::process::ExitStatus::from_raw(status_raw);
    let exit_code = exit_status.code().unwrap_or(-1);

    let max_rss_kb = rusage.map(|ru| ru_maxrss_kb(&ru));

    Ok(RunResult {
        wall_ms,
        exit_code,
        timed_out,
        max_rss_kb,
        stdout,
        stderr,
    })
}

#[cfg(unix)]
fn read_with_cap<R: Read>(reader: &mut R, cap: usize) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut tmp = [0u8; 8192];

    loop {
        match reader.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                if buf.len() < cap {
                    let remaining = cap - buf.len();
                    let take = remaining.min(n);
                    buf.extend_from_slice(&tmp[..take]);
                }
            }
            Err(_) => break,
        }
    }

    buf
}

#[cfg(unix)]
fn wait4_with_timeout(
    pid: libc::pid_t,
    timeout: Option<Duration>,
) -> Result<(libc::c_int, Option<libc::rusage>, bool), AdapterError> {
    use std::mem;

    let start = Instant::now();
    let mut status: libc::c_int = 0;
    let mut ru: libc::rusage = unsafe { mem::zeroed() };

    let mut timed_out = false;

    loop {
        let options = if timeout.is_some() { libc::WNOHANG } else { 0 };

        let res = unsafe { libc::wait4(pid, &mut status as *mut libc::c_int, options, &mut ru) };

        if res == pid {
            break;
        }

        if res == 0 {
            // still running
            if let Some(t) = timeout {
                if start.elapsed() >= t {
                    timed_out = true;
                    unsafe {
                        libc::kill(pid, libc::SIGKILL);
                    }
                    // Reap it.
                    let res2 =
                        unsafe { libc::wait4(pid, &mut status as *mut libc::c_int, 0, &mut ru) };
                    if res2 != pid {
                        return Err(AdapterError::Other(anyhow::anyhow!(
                            "wait4 after kill failed: {:?}",
                            std::io::Error::last_os_error()
                        )));
                    }
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
        }

        if res == -1 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(AdapterError::Other(anyhow::anyhow!("wait4 failed: {err}")));
        }

        // Any other pid is unexpected.
        return Err(AdapterError::Other(anyhow::anyhow!(
            "wait4 returned unexpected pid: {res}"
        )));
    }

    Ok((status, Some(ru), timed_out))
}

#[cfg(unix)]
fn ru_maxrss_kb(ru: &libc::rusage) -> u64 {
    let raw = ru.ru_maxrss as u64;

    // On Linux, ru_maxrss is KB.
    // On macOS, ru_maxrss is bytes.
    #[cfg(target_os = "macos")]
    {
        raw / 1024
    }

    #[cfg(not(target_os = "macos"))]
    {
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // **Feature: comprehensive-test-coverage, Property 8: Output Truncation Invariant**
    // *For any* byte sequence and cap value, the truncated output SHALL have length `min(original_length, cap)`.
    //
    // **Validates: Requirements 9.3**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// Property test: truncated length equals min(original_length, cap)
        #[test]
        fn truncate_length_equals_min_of_original_and_cap(
            bytes in proptest::collection::vec(any::<u8>(), 0..1000),
            cap in 0usize..2000
        ) {
            let original_len = bytes.len();
            let result = truncate(bytes, cap);
            let expected_len = original_len.min(cap);
            prop_assert_eq!(
                result.len(),
                expected_len,
                "truncated length should be min({}, {}) = {}, but got {}",
                original_len,
                cap,
                expected_len,
                result.len()
            );
        }

        /// Property test: truncated content is a prefix of the original
        #[test]
        fn truncate_preserves_prefix(
            bytes in proptest::collection::vec(any::<u8>(), 0..1000),
            cap in 0usize..2000
        ) {
            let original = bytes.clone();
            let result = truncate(bytes, cap);

            // The result should be a prefix of the original
            prop_assert!(
                original.starts_with(&result),
                "truncated output should be a prefix of the original"
            );
        }

        /// Property test: when cap >= original_length, output equals original
        #[test]
        fn truncate_no_op_when_cap_exceeds_length(
            bytes in proptest::collection::vec(any::<u8>(), 0..500)
        ) {
            let original = bytes.clone();
            let original_len = original.len();
            // Use a cap that is >= original length
            let cap = original_len + 100;
            let result = truncate(bytes, cap);

            prop_assert_eq!(
                result,
                original,
                "when cap ({}) >= original_length ({}), output should equal original",
                cap,
                original_len
            );
        }
    }

    // Additional unit tests for edge cases
    #[test]
    fn truncate_empty_vec() {
        let result = truncate(vec![], 10);
        assert_eq!(result, vec![]);
    }

    #[test]
    fn truncate_with_zero_cap() {
        let result = truncate(vec![1, 2, 3, 4, 5], 0);
        assert_eq!(result, vec![]);
    }

    #[test]
    fn truncate_exact_cap() {
        let bytes = vec![1, 2, 3, 4, 5];
        let result = truncate(bytes.clone(), 5);
        assert_eq!(result, bytes);
    }

    #[test]
    fn truncate_one_over_cap() {
        let bytes = vec![1, 2, 3, 4, 5];
        let result = truncate(bytes, 4);
        assert_eq!(result, vec![1, 2, 3, 4]);
    }

    // =========================================================================
    // Unit tests for adapter error conditions
    // Validates: Requirements 11.3
    // =========================================================================

    /// Test that StdProcessRunner::run returns AdapterError::EmptyArgv when argv is empty
    #[test]
    fn empty_argv_returns_error() {
        let runner = StdProcessRunner;
        let spec = CommandSpec {
            argv: vec![],
            cwd: None,
            env: vec![],
            timeout: None,
            output_cap_bytes: 1024,
        };

        let result = runner.run(&spec);
        assert!(result.is_err(), "Expected error for empty argv");

        let err = result.unwrap_err();
        assert!(
            matches!(err, AdapterError::EmptyArgv),
            "Expected AdapterError::EmptyArgv, got {:?}",
            err
        );
    }

    /// Test that AdapterError::EmptyArgv has a descriptive error message
    #[test]
    fn empty_argv_error_message_is_descriptive() {
        let err = AdapterError::EmptyArgv;
        let msg = err.to_string();
        assert!(
            msg.contains("argv") && msg.contains("empty"),
            "Error message should mention 'argv' and 'empty', got: {}",
            msg
        );
    }

    /// Test that AdapterError::Timeout has a descriptive error message
    #[test]
    fn timeout_error_message_is_descriptive() {
        let err = AdapterError::Timeout;
        let msg = err.to_string();
        assert!(
            msg.contains("timed out") || msg.contains("timeout"),
            "Error message should mention 'timeout', got: {}",
            msg
        );
    }

    /// Test that AdapterError::TimeoutUnsupported has a descriptive error message
    #[test]
    fn timeout_unsupported_error_message_is_descriptive() {
        let err = AdapterError::TimeoutUnsupported;
        let msg = err.to_string();
        assert!(
            msg.contains("timeout") && msg.contains("not supported"),
            "Error message should mention 'timeout' and 'not supported', got: {}",
            msg
        );
    }

    /// Test that on non-Unix platforms, timeout returns TimeoutUnsupported error
    /// On Unix platforms, this test verifies the timeout functionality works
    #[cfg(not(unix))]
    #[test]
    fn timeout_on_non_unix_returns_unsupported() {
        let runner = StdProcessRunner;
        let spec = CommandSpec {
            argv: vec!["echo".to_string(), "hello".to_string()],
            cwd: None,
            env: vec![],
            timeout: Some(Duration::from_secs(10)),
            output_cap_bytes: 1024,
        };

        let result = runner.run(&spec);
        assert!(result.is_err(), "Expected error for timeout on non-Unix");

        let err = result.unwrap_err();
        assert!(
            matches!(err, AdapterError::TimeoutUnsupported),
            "Expected AdapterError::TimeoutUnsupported, got {:?}",
            err
        );
    }

    /// Test that on Unix platforms, timeout is supported and works correctly
    #[cfg(unix)]
    #[test]
    fn timeout_on_unix_is_supported() {
        let runner = StdProcessRunner;
        // Use a command that completes quickly
        let spec = CommandSpec {
            argv: vec!["echo".to_string(), "hello".to_string()],
            cwd: None,
            env: vec![],
            timeout: Some(Duration::from_secs(10)),
            output_cap_bytes: 1024,
        };

        let result = runner.run(&spec);
        assert!(
            result.is_ok(),
            "Timeout should be supported on Unix, got error: {:?}",
            result.err()
        );

        let run_result = result.unwrap();
        assert!(!run_result.timed_out, "Command should not have timed out");
        assert_eq!(run_result.exit_code, 0, "Command should have succeeded");
    }

    /// Test that on Unix platforms, a command that exceeds timeout is killed
    #[cfg(unix)]
    #[test]
    fn timeout_kills_long_running_command() {
        let runner = StdProcessRunner;
        // Use sleep command that would take longer than the timeout
        let spec = CommandSpec {
            argv: vec!["sleep".to_string(), "10".to_string()],
            cwd: None,
            env: vec![],
            timeout: Some(Duration::from_millis(100)),
            output_cap_bytes: 1024,
        };

        let start = std::time::Instant::now();
        let result = runner.run(&spec);
        let elapsed = start.elapsed();

        assert!(
            result.is_ok(),
            "Should return Ok with timed_out flag, got error: {:?}",
            result.err()
        );

        let run_result = result.unwrap();
        assert!(run_result.timed_out, "Command should have timed out");

        // Verify the command was killed within a reasonable time (not the full 10 seconds)
        assert!(
            elapsed < Duration::from_secs(2),
            "Command should have been killed quickly, but took {:?}",
            elapsed
        );
    }

    /// Test that AdapterError::Other wraps anyhow errors correctly
    #[test]
    fn other_error_wraps_anyhow() {
        let inner_err = anyhow::anyhow!("test error message");
        let err = AdapterError::Other(inner_err);
        let msg = err.to_string();
        assert!(
            msg.contains("test error message"),
            "Error message should contain the inner error, got: {}",
            msg
        );
    }

    /// Test that empty argv check happens before any process spawning
    #[test]
    fn empty_argv_check_is_immediate() {
        let runner = StdProcessRunner;
        let spec = CommandSpec {
            argv: vec![],
            cwd: Some(std::path::PathBuf::from("/nonexistent/path")),
            env: vec![("SOME_VAR".to_string(), "value".to_string())],
            timeout: Some(Duration::from_secs(1)),
            output_cap_bytes: 1024,
        };

        // This should return EmptyArgv error immediately, not fail due to
        // invalid cwd or other issues
        let result = runner.run(&spec);
        assert!(
            matches!(result, Err(AdapterError::EmptyArgv)),
            "Should return EmptyArgv before checking other parameters"
        );
    }
}
