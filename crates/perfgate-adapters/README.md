# perfgate-adapters

Infrastructure adapters for process execution and host probing.

## Responsibilities

- Runs benchmark commands through `StdProcessRunner` and `ProcessRunner`.
- Captures wall-clock timing, exit code, timeout status, and capped stdout/stderr.
- Collects process metrics when available:
  - Unix: `cpu_ms`, `max_rss_kb`, `page_faults`, `ctx_switches` (via `wait4`/`rusage`)
  - Windows: best-effort `cpu_ms` and `max_rss_kb`
- Probes host info via `StdHostProbe` (`os`, `arch`, CPU count, memory, optional hostname hash).
- Provides `FakeProcessRunner` for deterministic tests.

## Platform Notes

- Unix supports command timeouts.
- Windows currently returns `AdapterError::TimeoutUnsupported` if timeout is requested.
- Other platforms run without timeout support and with limited metrics.

## Boundaries

- No policy or threshold logic.
- No markdown/report rendering.
- No CLI argument parsing.

## Workspace Role

`perfgate-adapters` is the "touch the world" layer used by application use-cases:

`perfgate-types` + `perfgate-domain` + `perfgate-adapters` -> `perfgate-app`

## License

Licensed under either Apache-2.0 or MIT.
