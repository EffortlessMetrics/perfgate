# perfgate-adapters

Platform boundary layer — process execution, system metrics collection, and host probing. This is where platform-specific code lives.

## Build and Test

```bash
cargo test -p perfgate-adapters
```

Mutation testing target: **80% kill rate**.

## What This Crate Contains

Traits and implementations for running processes and collecting system information.

### Source Layout

- `src/lib.rs` — All adapter types, traits, and platform-specific implementations

### Key Traits

**`ProcessRunner`**:
- `fn run(&self, spec: &CommandSpec) -> Result<RunResult, AdapterError>`
- Implemented by `StdProcessRunner`

**`HostProbe`**:
- `fn probe(&self, options: &HostProbeOptions) -> HostInfo`
- Implemented by `StdHostProbe`

### Key Types

- `CommandSpec` — argv, cwd, env, timeout, output_cap_bytes
- `RunResult` — wall_ms, exit_code, timed_out, cpu_ms, max_rss_kb, stdout/stderr
- `AdapterError` — `EmptyArgv`, `Timeout`, `TimeoutUnsupported`, `Other` (thiserror)
- `HostProbeOptions` — `include_hostname_hash: bool`

### Platform Behavior

| Feature | Unix | Windows |
|---------|------|---------|
| Process execution | `wait4()` with `WNOHANG` polling | `std::process::Command::output()` |
| Timeout | SIGKILL after deadline | `child.kill()` after deadline |
| CPU time (`cpu_ms`) | `rusage.ru_utime + ru_stime` | `None` |
| RSS (`max_rss_kb`) | `rusage.ru_maxrss` | `GetProcessMemoryInfo` (`PeakWorkingSetSize`) |
| Page faults (`page_faults`) | `rusage.ru_majflt` (major only) | `GetProcessMemoryInfo` (`PageFaultCount`) |
| Context switches (`ctx_switches`) | `rusage.ru_nvcsw + ru_nivcsw` | `None` (no Windows API equivalent) |
| Memory detection | `/proc/meminfo` (Linux), `sysctl` (macOS) | `GlobalMemoryStatusEx` |
| Hostname hash | SHA-256 of hostname | SHA-256 of hostname |

**RSS unit quirk**: Linux reports `ru_maxrss` in KB; macOS reports in bytes (divided by 1024).

### Implementation Details

- **Output capture**: 8KB buffer with capping at `output_cap_bytes`
- **SHA-256**: Inline implementation (no crypto dependency) for hostname hashing
- **Hostname hash is opt-in**: Only collected when `include_hostname_hash` is true

## Design Rules

- **Platform code stays here** — Other crates must not import `libc`, `std::process`, or platform APIs directly.
- **Traits enable testing** — `ProcessRunner` and `HostProbe` are trait objects so the app layer can inject fakes.
- **Errors are typed** — Use `AdapterError` variants, not `anyhow` strings.
