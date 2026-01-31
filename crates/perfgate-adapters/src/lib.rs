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
            return run_portable(spec);
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
                    let res2 = unsafe {
                        libc::wait4(pid, &mut status as *mut libc::c_int, 0, &mut ru)
                    };
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
