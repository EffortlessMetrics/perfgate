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
fn read_with_cap<R: std::io::Read>(reader: &mut R, cap: usize) -> Vec<u8> {
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

// ----------------------------
// Host fingerprinting
// ----------------------------

use perfgate_types::HostInfo;

/// Options for host probing.
#[derive(Debug, Clone, Default)]
pub struct HostProbeOptions {
    /// If true, include a SHA-256 hash of the hostname for fingerprinting.
    /// This is opt-in for privacy reasons.
    pub include_hostname_hash: bool,
}

/// Trait for probing host system information.
pub trait HostProbe {
    /// Probe the current host and return system information.
    fn probe(&self, options: &HostProbeOptions) -> HostInfo;
}

/// Standard implementation of HostProbe using platform APIs.
#[derive(Debug, Default, Clone)]
pub struct StdHostProbe;

impl HostProbe for StdHostProbe {
    fn probe(&self, options: &HostProbeOptions) -> HostInfo {
        HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count: probe_cpu_count(),
            memory_bytes: probe_memory_bytes(),
            hostname_hash: if options.include_hostname_hash {
                probe_hostname_hash()
            } else {
                None
            },
        }
    }
}

/// Get the number of logical CPUs.
/// Returns None if the count cannot be determined.
fn probe_cpu_count() -> Option<u32> {
    // Use std::thread::available_parallelism which is available since Rust 1.59
    std::thread::available_parallelism()
        .ok()
        .map(|n| n.get() as u32)
}

/// Get total system memory in bytes.
/// Returns None if memory cannot be determined.
fn probe_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        probe_memory_linux()
    }

    #[cfg(target_os = "macos")]
    {
        probe_memory_macos()
    }

    #[cfg(target_os = "windows")]
    {
        probe_memory_windows()
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn probe_memory_linux() -> Option<u64> {
    // Read from /proc/meminfo
    let content = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in content.lines() {
        if line.starts_with("MemTotal:") {
            // Format: "MemTotal:       16384000 kB"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                if let Ok(kb) = parts[1].parse::<u64>() {
                    return Some(kb * 1024); // Convert KB to bytes
                }
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn probe_memory_macos() -> Option<u64> {
    // Use sysctl to get hw.memsize
    use std::mem;

    let mut memsize: u64 = 0;
    let mut size = mem::size_of::<u64>();
    let name = c"hw.memsize";

    let ret = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut memsize as *mut u64 as *mut libc::c_void,
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };

    if ret == 0 {
        Some(memsize)
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn probe_memory_windows() -> Option<u64> {
    use std::mem;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct MEMORYSTATUSEX {
        dwLength: u32,
        dwMemoryLoad: u32,
        ullTotalPhys: u64,
        ullAvailPhys: u64,
        ullTotalPageFile: u64,
        ullAvailPageFile: u64,
        ullTotalVirtual: u64,
        ullAvailVirtual: u64,
        ullAvailExtendedVirtual: u64,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(lpBuffer: *mut MEMORYSTATUSEX) -> i32;
    }

    let mut status: MEMORYSTATUSEX = unsafe { mem::zeroed() };
    status.dwLength = mem::size_of::<MEMORYSTATUSEX>() as u32;

    let ret = unsafe { GlobalMemoryStatusEx(&mut status) };

    if ret != 0 {
        Some(status.ullTotalPhys)
    } else {
        None
    }
}

/// Get a SHA-256 hash of the hostname.
/// Returns None if hostname cannot be determined.
fn probe_hostname_hash() -> Option<String> {
    let hostname = hostname::get().ok()?;
    let hostname_str = hostname.to_string_lossy();

    // Simple SHA-256 implementation for privacy-preserving fingerprint
    Some(sha256_hex(hostname_str.as_bytes()))
}

/// Compute SHA-256 hash and return as hex string.
/// This is a minimal implementation to avoid adding a crypto dependency.
fn sha256_hex(data: &[u8]) -> String {
    use std::fmt::Write;

    // SHA-256 constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    // Initial hash values
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pre-processing: adding padding bits
    let ml = (data.len() as u64) * 8; // message length in bits
    let mut padded = data.to_vec();
    padded.push(0x80);

    // Pad to 56 mod 64 bytes
    while (padded.len() % 64) != 56 {
        padded.push(0x00);
    }

    // Append original length in bits as 64-bit big-endian
    padded.extend_from_slice(&ml.to_be_bytes());

    // Process each 64-byte chunk
    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];

        // Copy chunk into first 16 words
        for (i, word_bytes) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word_bytes[0], word_bytes[1], word_bytes[2], word_bytes[3]]);
        }

        // Extend the first 16 words into the remaining 48 words
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        // Initialize working variables
        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        // Compression function main loop
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        // Add the compressed chunk to the current hash value
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    // Produce the final hash value (big-endian)
    let mut result = String::with_capacity(64);
    for val in h.iter() {
        write!(result, "{:08x}", val).unwrap();
    }
    result
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
        assert_eq!(result, Vec::<u8>::new());
    }

    #[test]
    fn truncate_with_zero_cap() {
        let result = truncate(vec![1, 2, 3, 4, 5], 0);
        assert_eq!(result, Vec::<u8>::new());
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

    // =========================================================================
    // Unit tests for host fingerprinting
    // Validates: Host info probing for noise mitigation
    // =========================================================================

    /// Test that StdHostProbe returns valid os and arch strings
    #[test]
    fn host_probe_returns_valid_os_arch() {
        let probe = StdHostProbe;
        let options = HostProbeOptions::default();
        let info = probe.probe(&options);

        // os should be non-empty and match std::env::consts::OS
        assert!(!info.os.is_empty(), "os should not be empty");
        assert_eq!(info.os, std::env::consts::OS);

        // arch should be non-empty and match std::env::consts::ARCH
        assert!(!info.arch.is_empty(), "arch should not be empty");
        assert_eq!(info.arch, std::env::consts::ARCH);
    }

    /// Test that cpu_count is populated on most platforms
    #[test]
    fn host_probe_returns_cpu_count() {
        let probe = StdHostProbe;
        let options = HostProbeOptions::default();
        let info = probe.probe(&options);

        // cpu_count should be Some on most platforms
        // We test that if available, it's a sensible value
        if let Some(count) = info.cpu_count {
            assert!(count >= 1, "cpu_count should be at least 1, got {}", count);
            assert!(
                count <= 1024,
                "cpu_count should be at most 1024, got {}",
                count
            );
        }
    }

    /// Test that memory_bytes is populated on most platforms
    #[test]
    fn host_probe_returns_memory() {
        let probe = StdHostProbe;
        let options = HostProbeOptions::default();
        let info = probe.probe(&options);

        // memory_bytes should be Some on most platforms
        // We test that if available, it's a sensible value (at least 128MB, at most 128TB)
        if let Some(bytes) = info.memory_bytes {
            assert!(
                bytes >= 128 * 1024 * 1024,
                "memory_bytes should be at least 128MB, got {}",
                bytes
            );
            assert!(
                bytes <= 128 * 1024 * 1024 * 1024 * 1024,
                "memory_bytes should be at most 128TB, got {}",
                bytes
            );
        }
    }

    /// Test that hostname_hash is None when not requested
    #[test]
    fn host_probe_no_hostname_by_default() {
        let probe = StdHostProbe;
        let options = HostProbeOptions {
            include_hostname_hash: false,
        };
        let info = probe.probe(&options);

        assert!(
            info.hostname_hash.is_none(),
            "hostname_hash should be None when not requested"
        );
    }

    /// Test that hostname_hash is populated when requested
    #[test]
    fn host_probe_returns_hostname_hash_when_requested() {
        let probe = StdHostProbe;
        let options = HostProbeOptions {
            include_hostname_hash: true,
        };
        let info = probe.probe(&options);

        // hostname_hash should be Some and be a valid SHA-256 hex string (64 chars)
        if let Some(hash) = &info.hostname_hash {
            assert_eq!(
                hash.len(),
                64,
                "hostname_hash should be 64 hex chars, got {}",
                hash.len()
            );
            assert!(
                hash.chars().all(|c| c.is_ascii_hexdigit()),
                "hostname_hash should be hex, got {}",
                hash
            );
        }
        // Note: hostname_hash might be None if hostname cannot be determined
    }

    /// Test that SHA-256 implementation produces correct results
    #[test]
    fn sha256_produces_correct_hash() {
        // Test vector: empty string
        let empty_hash = sha256_hex(b"");
        assert_eq!(
            empty_hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "SHA-256 of empty string should match expected"
        );

        // Test vector: "hello"
        let hello_hash = sha256_hex(b"hello");
        assert_eq!(
            hello_hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            "SHA-256 of 'hello' should match expected"
        );

        // Test vector: "hello world"
        let hello_world_hash = sha256_hex(b"hello world");
        assert_eq!(
            hello_world_hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9",
            "SHA-256 of 'hello world' should match expected"
        );
    }

    /// Test that the same hostname produces the same hash (deterministic)
    #[test]
    fn hostname_hash_is_deterministic() {
        let probe = StdHostProbe;
        let options = HostProbeOptions {
            include_hostname_hash: true,
        };

        let info1 = probe.probe(&options);
        let info2 = probe.probe(&options);

        assert_eq!(
            info1.hostname_hash, info2.hostname_hash,
            "hostname_hash should be deterministic"
        );
    }
}
