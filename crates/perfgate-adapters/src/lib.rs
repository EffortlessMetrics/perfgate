//! Std adapters for perfgate.
//!
//! In clean-arch terms: this is where we touch the world.

mod fake;

pub use fake::FakeProcessRunner;

pub use perfgate_error::AdapterError;
use perfgate_sha256::sha256_hex;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

/// Command to execute.
#[derive(Debug, Clone, Default)]
pub struct CommandSpec {
    pub name: String,
    pub cwd: Option<PathBuf>,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub timeout: Option<Duration>,
    pub output_cap_bytes: usize,
}

/// Result of a single execution.
#[derive(Debug, Clone, Default)]
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
    /// Bytes read from disk (best-effort).
    pub io_read_bytes: Option<u64>,
    /// Bytes written to disk (best-effort).
    pub io_write_bytes: Option<u64>,
    /// Total network packets (best-effort).
    pub network_packets: Option<u64>,
    /// CPU energy used in microjoules (RAPL on Linux).
    pub energy_uj: Option<u64>,
    /// Size of executed binary in bytes (best-effort).
    pub binary_bytes: Option<u64>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub trait ProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<RunResult, AdapterError>;
}

/// Helper to truncate stdout/stderr bytes.
fn truncate(mut bytes: Vec<u8>, cap: usize) -> Vec<u8> {
    if cap > 0 && bytes.len() > cap {
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

    let out = cmd.output().map_err(|e| AdapterError::RunCommand {
        command: spec.argv.join(" "),
        reason: e.to_string(),
    })?;

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
        io_read_bytes: None,
        io_write_bytes: None,
        network_packets: None,
        energy_uj: None,
        binary_bytes,
        stdout: truncate(out.stdout, spec.output_cap_bytes),
        stderr: truncate(out.stderr, spec.output_cap_bytes),
    })
}

#[cfg(windows)]
fn run_windows(spec: &CommandSpec) -> Result<RunResult, AdapterError> {
    use std::process::{Command, Stdio};

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

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| AdapterError::RunCommand {
        command: spec.argv.join(" "),
        reason: e.to_string(),
    })?;

    let exit_status = if let Some(timeout) = spec.timeout {
        match child
            .wait_timeout(timeout)
            .map_err(|e| AdapterError::Other(e.to_string()))?
        {
            Some(status) => status,
            None => {
                child.kill().ok();
                return Err(AdapterError::Timeout);
            }
        }
    } else {
        child
            .wait()
            .map_err(|e| AdapterError::Other(e.to_string()))?
    };

    let wall_ms = start.elapsed().as_millis() as u64;
    let exit_code = exit_status.code().unwrap_or(-1);

    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();

    // Windows memory and IO info requires the handle before child is dropped or stdout/stderr taken?
    // Actually we need the handle to call GetProcessMemoryInfo and GetProcessIoCounters.
    let handle = child.as_raw_handle();
    let max_rss_kb = get_max_rss_windows(handle);
    let (io_read_bytes, io_write_bytes) = get_io_counters_windows(handle);

    if let Some(mut stdout) = child.stdout.take() {
        use std::io::Read;
        stdout.read_to_end(&mut stdout_buf).ok();
    }
    if let Some(mut stderr) = child.stderr.take() {
        use std::io::Read;
        stderr.read_to_end(&mut stderr_buf).ok();
    }

    Ok(RunResult {
        wall_ms,
        exit_code,
        timed_out: false,
        cpu_ms: None,
        page_faults: None,
        ctx_switches: None,
        max_rss_kb,
        io_read_bytes,
        io_write_bytes,
        network_packets: None,
        energy_uj: None,
        binary_bytes,
        stdout: truncate(stdout_buf, spec.output_cap_bytes),
        stderr: truncate(stderr_buf, spec.output_cap_bytes),
    })
}

#[cfg(windows)]
fn get_max_rss_windows(handle: std::os::windows::io::RawHandle) -> Option<u64> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    unsafe {
        if GetProcessMemoryInfo(
            HANDLE(handle as _),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
        .is_ok()
        {
            Some((counters.PeakWorkingSetSize / 1024) as u64)
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn get_io_counters_windows(handle: std::os::windows::io::RawHandle) -> (Option<u64>, Option<u64>) {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::{GetProcessIoCounters, IO_COUNTERS};

    let mut counters = IO_COUNTERS::default();
    unsafe {
        if GetProcessIoCounters(HANDLE(handle as _), &mut counters).is_ok() {
            (
                Some(counters.ReadTransferCount),
                Some(counters.WriteTransferCount),
            )
        } else {
            (None, None)
        }
    }
}

#[cfg(unix)]
fn run_unix(spec: &CommandSpec) -> Result<RunResult, AdapterError> {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};

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

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut usage_before = unsafe { std::mem::zeroed::<libc::rusage>() };
    let _ = unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage_before) };

    let start = Instant::now();

    let out = if let Some(timeout) = spec.timeout {
        let mut child = cmd.spawn().map_err(|e| AdapterError::RunCommand {
            command: spec.argv.join(" "),
            reason: e.to_string(),
        })?;

        match child
            .wait_timeout(timeout)
            .map_err(|e| AdapterError::Other(e.to_string()))?
        {
            Some(_) => child
                .wait_with_output()
                .map_err(|e| AdapterError::RunCommand {
                    command: spec.argv.join(" "),
                    reason: e.to_string(),
                })?,
            None => {
                child.kill().ok();
                return Err(AdapterError::Timeout);
            }
        }
    } else {
        cmd.output().map_err(|e| AdapterError::RunCommand {
            command: spec.argv.join(" "),
            reason: e.to_string(),
        })?
    };

    let wall_ms = start.elapsed().as_millis() as u64;
    let exit_code = out
        .status
        .code()
        .or_else(|| out.status.signal())
        .unwrap_or(-1);

    let mut cpu_ms = None;
    let mut max_rss_kb = None;
    let mut page_faults = None;
    let mut ctx_switches = None;

    let mut usage_after = unsafe { std::mem::zeroed::<libc::rusage>() };
    if unsafe { libc::getrusage(libc::RUSAGE_CHILDREN, &mut usage_after) } == 0 {
        let user_ms = diff_timeval_ms(usage_after.ru_utime, usage_before.ru_utime);
        let sys_ms = diff_timeval_ms(usage_after.ru_stime, usage_before.ru_stime);

        cpu_ms = Some(user_ms.saturating_add(sys_ms));
        max_rss_kb = Some(usage_after.ru_maxrss as u64);
        page_faults =
            Some((usage_after.ru_majflt as u64).saturating_sub(usage_before.ru_majflt as u64));
        ctx_switches = Some(
            (usage_after.ru_nvcsw as u64)
                .saturating_sub(usage_before.ru_nvcsw as u64)
                .saturating_add(
                    (usage_after.ru_nivcsw as u64).saturating_sub(usage_before.ru_nivcsw as u64),
                ),
        );
    }

    Ok(RunResult {
        wall_ms,
        exit_code,
        timed_out: false,
        cpu_ms,
        page_faults,
        ctx_switches,
        max_rss_kb,
        io_read_bytes: None,
        io_write_bytes: None,
        network_packets: None,
        energy_uj: None,
        binary_bytes,
        stdout: truncate(out.stdout, spec.output_cap_bytes),
        stderr: truncate(out.stderr, spec.output_cap_bytes),
    })
}

/// Standard process runner using std::process::Command.
#[derive(Clone, Debug, Default)]
pub struct StdProcessRunner;

impl ProcessRunner for StdProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<RunResult, AdapterError> {
        if spec.argv.is_empty() {
            return Err(AdapterError::EmptyArgv);
        }

        #[cfg(windows)]
        {
            run_windows(spec)
        }
        #[cfg(unix)]
        {
            run_unix(spec)
        }
        #[cfg(all(not(unix), not(windows)))]
        {
            run_portable(spec)
        }
    }
}

/// Host fingerprinting and metadata collection.
pub trait HostProbe {
    fn probe(&self, options: &HostProbeOptions) -> perfgate_types::HostInfo;
}

#[derive(Debug, Clone, Default)]
pub struct HostProbeOptions {
    pub include_hostname_hash: bool,
}

#[derive(Clone, Debug, Default)]
pub struct StdHostProbe;

impl HostProbe for StdHostProbe {
    fn probe(&self, options: &HostProbeOptions) -> perfgate_types::HostInfo {
        let hostname_hash = if options.include_hostname_hash {
            hostname::get()
                .ok()
                .map(|h| sha256_hex(h.to_string_lossy().as_bytes()))
        } else {
            None
        };

        perfgate_types::HostInfo {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            cpu_count: Some(num_cpus::get_physical() as u32),
            memory_bytes: Some(get_total_memory()),
            hostname_hash,
        }
    }
}

fn get_total_memory() -> u64 {
    #[cfg(windows)]
    {
        use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut mem_status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe {
            if GlobalMemoryStatusEx(&mut mem_status).is_ok() {
                mem_status.ullTotalPhys
            } else {
                0
            }
        }
    }
    #[cfg(unix)]
    {
        // Simple fallback for total memory on Unix
        0
    }
    #[cfg(all(not(unix), not(windows)))]
    {
        0
    }
}

fn binary_bytes_for_command(spec: &CommandSpec) -> Option<u64> {
    spec.argv.first().and_then(|cmd| {
        let path = Path::new(cmd);
        if path.exists() {
            std::fs::metadata(path).ok().map(|m| m.len())
        } else {
            // Try searching in PATH
            which::which(cmd)
                .ok()
                .and_then(|p| std::fs::metadata(p).ok().map(|m| m.len()))
        }
    })
}

// Extension trait for Command to support timeout on Windows/Unix
trait CommandTimeoutExt {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

#[cfg(windows)]
impl CommandTimeoutExt for std::process::Child {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
        use windows::Win32::System::Threading::WaitForSingleObject;

        let handle = self.as_raw_handle();
        let millis = timeout.as_millis() as u32;

        unsafe {
            let res = WaitForSingleObject(HANDLE(handle as _), millis);
            if res == WAIT_OBJECT_0 {
                self.wait().map(Some)
            } else if res == WAIT_TIMEOUT {
                Ok(None)
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
    }
}

#[cfg(unix)]
impl CommandTimeoutExt for std::process::Child {
    fn wait_timeout(
        &mut self,
        timeout: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if let Some(status) = self.try_wait()? {
                return Ok(Some(status));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(None)
    }
}

#[cfg(unix)]
fn diff_timeval_ms(after: libc::timeval, before: libc::timeval) -> u64 {
    let mut sec = after.tv_sec as i64 - before.tv_sec as i64;
    let mut usec = after.tv_usec as i64 - before.tv_usec as i64;

    if usec < 0 {
        sec -= 1;
        usec += 1_000_000;
    }

    // Ensure we don't underflow if rusage somehow goes backwards (unlikely)
    if sec < 0 {
        return 0;
    }

    (sec as u64) * 1000 + (usec as u64) / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_works() {
        let data = vec![1, 2, 3, 4, 5];
        assert_eq!(truncate(data.clone(), 3), vec![1, 2, 3]);
        assert_eq!(truncate(data.clone(), 10), data);
        assert_eq!(truncate(data.clone(), 0), data);
    }

    /// read_with_cap truncates to cap.
    #[test]
    fn read_with_cap_truncates() {
        fn read_with_cap<R: std::io::Read>(reader: &mut R, cap: usize) -> Vec<u8> {
            let mut buf = vec![0u8; cap];
            let n = reader.read(&mut buf).unwrap();
            buf.truncate(n);
            buf
        }

        let mut reader: &[u8] = b"hello world";
        let result = read_with_cap(&mut reader, 5);
        assert_eq!(result, b"hello");
    }
}
