# perfgate-adapters

Platform-specific process execution and metric collection for the
[perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

Every other crate in the workspace is platform-agnostic. `perfgate-adapters`
is the single place where OS APIs, `libc`, and `std::process` are used --
keeping the rest of the codebase testable and portable.

## Platform support

| Capability | Unix | Windows | Other |
|---|---|---|---|
| Process execution | `wait4` + `WNOHANG` polling | `WaitForSingleObject` | `std::process::Command` |
| Timeout / kill | SIGKILL after deadline | `WaitForSingleObject` + kill | not supported |
| CPU time (`cpu_ms`) | `rusage` (user + system) | -- | -- |
| Peak RSS (`max_rss_kb`) | `rusage.ru_maxrss` | `GetProcessMemoryInfo` | -- |
| Page faults | `rusage.ru_majflt` | -- | -- |
| Context switches | `rusage` (vol + invol) | -- | -- |
| I/O bytes | -- | `GetProcessIoCounters` | -- |
| Memory detection | `/proc/meminfo`, `sysctl` | `GlobalMemoryStatusEx` | -- |
| Hostname hash | SHA-256 | SHA-256 | SHA-256 |

> **RSS unit quirk:** Linux reports `ru_maxrss` in KB; macOS reports bytes
> (divided by 1024 internally).

## Key types

| Type | Role |
|---|---|
| `CommandSpec` | Describes a command: argv, cwd, env, timeout, output cap |
| `RunResult` | Execution output: wall_ms, exit_code, cpu_ms, max_rss_kb, stdout/stderr, ... |
| `AdapterError` | Typed errors: `EmptyArgv`, `Timeout`, `RunCommand`, `Other` |
| `ProcessRunner` (trait) | Abstracts process execution for dependency injection |
| `HostProbe` (trait) | Collects OS, arch, CPU count, memory, optional hostname hash |
| `FakeProcessRunner` | Deterministic test double for `ProcessRunner` |

## Design

- **Traits enable testing** -- `ProcessRunner` and `HostProbe` are trait objects
  so the app layer can inject fakes without spawning real processes.
- **Output capping** -- stdout/stderr are truncated to `output_cap_bytes` (default 8 KB).
- **No policy logic** -- this crate only collects data; thresholds and verdicts
  live in `perfgate-budget` and `perfgate-domain`.

```text
perfgate-types + perfgate-domain + perfgate-adapters --> perfgate-app
```

## License

Licensed under either Apache-2.0 or MIT.
