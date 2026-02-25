//! Std adapters for perfgate.
//!
//! In clean-arch terms: this is where we touch the world.

mod fake;

pub use fake::FakeProcessRunner;

use anyhow::Context;
use perfgate_sha256::sha256_hex;
use std::path::{Path, PathBuf};
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
    /// CPU time (user + system) in milliseconds.
    /// Collected on Unix via rusage and best-effort on Windows.
    pub cpu_ms: Option<u64>,
    /// Major page faults (Unix only).
    pub page_faults: Option<u64>,
    /// Voluntary + involuntary context switches (Unix only).
    pub ctx_switches: Option<u64>,
    /// Peak resident set size in KB.
    /// Collected on Unix via rusage and best-effort on Windows.
    pub max_rss_kb: Option<u64>,
    /// Size of executed binary in bytes (best-effort).
    pub binary_bytes: Option<u64>,
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

        #[cfg(windows)]
        {
            if spec.timeout.is_some() {
                return Err(AdapterError::TimeoutUnsupported);
            }
            run_windows(spec)
        }

        #[cfg(all(not(unix), not(windows)))]
        {
            if spec.timeout.is_some() {
                return Err(AdapterError::TimeoutUnsupported);
            }
            run_portable(spec)
        }
    }
}

#[allow(dead_code)]
fn truncate(mut bytes: Vec<u8>, cap: usize) -> Vec<u8> {
    if bytes.len() > cap {
        bytes.truncate(cap);
    }
    bytes
}

#[cfg(all(not(unix), not(windows)))]
fn run_portable(spec: &CommandSpec) -> Result<RunResult, AdapterError> {
    use std::process::Command;

    let start = Instant::now();
    let binary_bytes = binary_bytes_for_command(spec);
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
        cpu_ms: None,
        page_faults: None,
        ctx_switches: None,
        max_rss_kb: None,
        binary_bytes,
        stdout: truncate(out.stdout, spec.output_cap_bytes),
        stderr: truncate(out.stderr, spec.output_cap_bytes),
    })
}

#[cfg(windows)]
fn run_windows(spec: &CommandSpec) -> Result<RunResult, AdapterError> {
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};
    use std::thread;

    let start = Instant::now();
    let binary_bytes = binary_bytes_for_command(spec);

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

    let mut stdout = child.stdout.take().expect("stdout piped");
    let mut stderr = child.stderr.take().expect("stderr piped");
    let cap = spec.output_cap_bytes;

    let out_handle = thread::spawn(move || read_with_cap(&mut stdout, cap));
    let err_handle = thread::spawn(move || read_with_cap(&mut stderr, cap));

    let status = child
        .wait()
        .with_context(|| format!("failed to wait for {:?}", spec.argv))
        .map_err(AdapterError::Other)?;

    let (cpu_ms, max_rss_kb) = probe_process_usage_windows(child.as_raw_handle());

    let stdout = out_handle.join().unwrap_or_default();
    let stderr = err_handle.join().unwrap_or_default();

    let wall_ms = start.elapsed().as_millis() as u64;
    let exit_code = status.code().unwrap_or(-1);

    Ok(RunResult {
        wall_ms,
        exit_code,
        timed_out: false,
        cpu_ms,
        page_faults: None,
        ctx_switches: None,
        max_rss_kb,
        binary_bytes,
        stdout,
        stderr,
    })
}

#[cfg(unix)]
fn run_unix(spec: &CommandSpec) -> Result<RunResult, AdapterError> {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    use std::thread;

    let start = Instant::now();
    let binary_bytes = binary_bytes_for_command(spec);

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

    let cpu_ms = rusage.map(|ru| ru_cpu_ms(&ru));
    let page_faults = rusage.map(|ru| ru_page_faults(&ru));
    let ctx_switches = rusage.map(|ru| ru_ctx_switches(&ru));
    let max_rss_kb = rusage.map(|ru| ru_maxrss_kb(&ru));

    Ok(RunResult {
        wall_ms,
        exit_code,
        timed_out,
        cpu_ms,
        page_faults,
        ctx_switches,
        max_rss_kb,
        binary_bytes,
        stdout,
        stderr,
    })
}

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

#[cfg(windows)]
fn probe_process_usage_windows(
    handle: std::os::windows::io::RawHandle,
) -> (Option<u64>, Option<u64>) {
    use std::ffi::c_void;
    use std::mem;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct FileTime {
        dwLowDateTime: u32,
        dwHighDateTime: u32,
    }

    #[repr(C)]
    #[allow(non_snake_case)]
    struct ProcessMemoryCounters {
        cb: u32,
        PageFaultCount: u32,
        PeakWorkingSetSize: usize,
        WorkingSetSize: usize,
        QuotaPeakPagedPoolUsage: usize,
        QuotaPagedPoolUsage: usize,
        QuotaPeakNonPagedPoolUsage: usize,
        QuotaNonPagedPoolUsage: usize,
        PagefileUsage: usize,
        PeakPagefileUsage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetProcessTimes(
            hProcess: *mut c_void,
            lpCreationTime: *mut FileTime,
            lpExitTime: *mut FileTime,
            lpKernelTime: *mut FileTime,
            lpUserTime: *mut FileTime,
        ) -> i32;
    }

    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            Process: *mut c_void,
            ppsmemCounters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    fn filetime_to_u64(ft: &FileTime) -> u64 {
        ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
    }

    let raw = handle.cast::<c_void>();

    let mut creation: FileTime = unsafe { mem::zeroed() };
    let mut exit: FileTime = unsafe { mem::zeroed() };
    let mut kernel: FileTime = unsafe { mem::zeroed() };
    let mut user: FileTime = unsafe { mem::zeroed() };

    let cpu_ms =
        if unsafe { GetProcessTimes(raw, &mut creation, &mut exit, &mut kernel, &mut user) } != 0 {
            let total_100ns = filetime_to_u64(&kernel).saturating_add(filetime_to_u64(&user));
            Some(total_100ns / 10_000)
        } else {
            None
        };

    let mut counters: ProcessMemoryCounters = unsafe { mem::zeroed() };
    counters.cb = mem::size_of::<ProcessMemoryCounters>() as u32;
    let max_rss_kb = if unsafe { GetProcessMemoryInfo(raw, &mut counters, counters.cb) } != 0 {
        Some((counters.PeakWorkingSetSize as u64) / 1024)
    } else {
        None
    };

    (cpu_ms, max_rss_kb)
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
fn ru_cpu_ms(ru: &libc::rusage) -> u64 {
    // ru_utime and ru_stime are timeval structs with tv_sec (seconds) and tv_usec (microseconds)
    let user_ms = (ru.ru_utime.tv_sec as u64) * 1000 + (ru.ru_utime.tv_usec as u64) / 1000;
    let sys_ms = (ru.ru_stime.tv_sec as u64) * 1000 + (ru.ru_stime.tv_usec as u64) / 1000;
    user_ms + sys_ms
}

#[cfg(unix)]
fn ru_page_faults(ru: &libc::rusage) -> u64 {
    // ru_majflt is major page faults.
    clamp_nonnegative_c_long(ru.ru_majflt)
}

#[cfg(unix)]
fn ru_ctx_switches(ru: &libc::rusage) -> u64 {
    // ru_nvcsw: voluntary; ru_nivcsw: involuntary context switches.
    clamp_nonnegative_c_long(ru.ru_nvcsw).saturating_add(clamp_nonnegative_c_long(ru.ru_nivcsw))
}

#[cfg(unix)]
fn clamp_nonnegative_c_long(v: libc::c_long) -> u64 {
    if v < 0 { 0 } else { v as u64 }
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

fn binary_bytes_for_command(spec: &CommandSpec) -> Option<u64> {
    let cmd = spec.argv.first()?;
    let path = resolve_command_path(cmd, spec.cwd.as_deref())?;
    std::fs::metadata(path).ok().map(|m| m.len())
}

fn resolve_command_path(command: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let command_path = Path::new(command);

    // If command contains separators or is absolute, resolve directly (relative to cwd if provided).
    if command_path.is_absolute() || command_path.components().count() > 1 {
        let candidate = if command_path.is_absolute() {
            command_path.to_path_buf()
        } else if let Some(dir) = cwd {
            dir.join(command_path)
        } else {
            command_path.to_path_buf()
        };
        return candidate.is_file().then_some(candidate);
    }

    // Otherwise, resolve via PATH lookup.
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }

        #[cfg(windows)]
        {
            if candidate.extension().is_none() {
                let pathext = std::env::var_os("PATHEXT").unwrap_or(".COM;.EXE;.BAT;.CMD".into());
                for ext in pathext.to_string_lossy().split(';') {
                    let ext = ext.trim();
                    if ext.is_empty() {
                        continue;
                    }
                    let mut with_ext = candidate.clone();
                    let normalized = ext.trim_start_matches('.');
                    with_ext.set_extension(normalized);
                    if with_ext.is_file() {
                        return Some(with_ext);
                    }
                }
            }
        }
    }

    None
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

    if ret == 0 { Some(memsize) } else { None }
}

#[cfg(target_os = "windows")]
fn probe_memory_windows() -> Option<u64> {
    use std::mem;

    #[repr(C)]
    #[allow(non_snake_case)]
    struct MemoryStatusEx {
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
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(lpBuffer: *mut MemoryStatusEx) -> i32;
    }

    let mut status: MemoryStatusEx = unsafe { mem::zeroed() };
    status.dwLength = mem::size_of::<MemoryStatusEx>() as u32;

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
    Some(sha256_hex(hostname_str.as_bytes()))
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

    /// Test that Windows collects best-effort CPU and RSS metrics.
    #[cfg(windows)]
    #[test]
    fn windows_collects_best_effort_metrics() {
        let runner = StdProcessRunner;
        let spec = CommandSpec {
            argv: vec![
                "cmd".to_string(),
                "/c".to_string(),
                "echo".to_string(),
                "hello".to_string(),
            ],
            cwd: None,
            env: vec![],
            timeout: None,
            output_cap_bytes: 1024,
        };

        let result = runner.run(&spec).expect("windows run should succeed");
        assert_eq!(result.exit_code, 0, "command should succeed");
        assert!(
            result.cpu_ms.is_some(),
            "cpu_ms should be available on Windows (best-effort)"
        );
        assert!(
            result.max_rss_kb.is_some(),
            "max_rss_kb should be available on Windows (best-effort)"
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
            let hash_len = hash.len();
            assert_eq!(
                hash_len, 64,
                "hostname_hash should be 64 hex chars, got {}",
                hash_len
            );
            assert!(
                hash.chars().all(|c| c.is_ascii_hexdigit()),
                "hostname_hash should be hex, got {}",
                hash
            );
        }
        // Note: hostname_hash might be None if hostname cannot be determined
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
