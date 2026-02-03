# perfgate Architecture

This document describes the architectural design of perfgate, a selective build-truth sensor for performance budgets in CI pipelines.

## Role Statement

**perfgate is a selective build-truth sensor.** It gates merges on explicit performance budgets by comparing black-box command receipts to baselines.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

## Truth Layer

perfgate operates as a **build truth** component: it measures and reports performance characteristics of arbitrary commands without understanding their internals. This is a selective sensor approach:

- **Black-box measurement**: perfgate measures wall-clock time, memory usage (RSS), and derived throughput without instrumenting the target command
- **Explicit budgets**: Regression thresholds are user-defined, not inferred
- **Deterministic verdicts**: Given the same inputs, perfgate MUST produce the same verdict

## Non-Goals

perfgate intentionally avoids these responsibilities:

1. **Baseline service**: perfgate does NOT manage baseline storage. Users MUST handle baseline persistence (git, artifact storage, databases)

2. **Profiler**: perfgate does NOT profile code or identify hot paths. It measures whole-command execution only

3. **Test runner/director**: perfgate does NOT orchestrate test suites or manage parallelism. It runs a single command specification

4. **Statistical inference**: perfgate does NOT perform significance testing or confidence intervals. It uses simple threshold-based policy on medians

5. **Host normalization**: perfgate does NOT normalize measurements across different hardware. Host fingerprinting is informational only

## Crate Boundaries

perfgate follows clean architecture principles with strictly layered dependencies:

```
┌─────────────────────────────────────────────────────────────────┐
│                        perfgate-cli                              │
│                    (clap CLI, JSON I/O)                         │
├─────────────────────────────────────────────────────────────────┤
│                        perfgate-app                              │
│          (use-cases, markdown/annotation rendering)             │
├─────────────────────────────────────────────────────────────────┤
│                      perfgate-adapters                           │
│             (process runner, system metrics)                    │
├─────────────────────────────────────────────────────────────────┤
│                       perfgate-domain                            │
│                (pure math/policy, I/O-free)                     │
├─────────────────────────────────────────────────────────────────┤
│                       perfgate-types                             │
│              (receipt/config structs, JSON schema)              │
└─────────────────────────────────────────────────────────────────┘
```

### Dependency Flow

Dependencies flow inward only:

```
perfgate-types (innermost)
       ↓
perfgate-domain
       ↓
perfgate-adapters
       ↓
perfgate-app
       ↓
perfgate-cli (outermost)
```

### Crate Responsibilities

#### perfgate-types

- MUST define all receipt and config data structures
- MUST provide JSON Schema support via `schemars`
- MUST maintain backward compatibility for schema versions
- SHALL NOT perform I/O or contain business logic

#### perfgate-domain

- MUST be I/O-free: statistics and policy only
- MUST implement median computation, delta calculation, and verdict determination
- MUST handle overflow-safe arithmetic for u64 statistics
- SHALL NOT depend on external services or filesystem

#### perfgate-adapters

- MUST implement platform-specific code (Unix `wait4()` for `max_rss_kb`)
- MUST define trait abstractions for process execution (`ProcessRunner`)
- MUST define trait abstractions for host probing (`HostProbe`)
- MUST define trait abstractions for time (`Clock`)
- SHOULD provide best-effort system metrics

#### perfgate-app

- MUST orchestrate adapters and domain logic
- MUST implement use-cases: run, compare, check, report, promote, export
- MUST generate markdown and GitHub annotation output
- SHALL NOT parse CLI arguments or perform direct filesystem I/O

#### perfgate-cli

- MUST parse CLI arguments using clap
- MUST perform JSON/TOML I/O for receipts and config files
- MUST map domain errors to appropriate exit codes
- SHOULD use atomic writes for output files

## Ports and Adapters

perfgate defines three primary ports (traits) in the adapter layer:

### ProcessRunner

```rust
pub trait ProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<RunResult, AdapterError>;
}
```

- MUST execute a command specification and return timing/exit information
- MUST support optional timeout (Unix only)
- MUST capture stdout/stderr up to a configurable limit
- SHOULD collect `max_rss_kb` on Unix via `rusage`

### HostProbe

```rust
pub trait HostProbe {
    fn probe(&self, options: &HostProbeOptions) -> HostInfo;
}
```

- MUST return OS and architecture strings
- SHOULD return CPU count and memory size
- MAY return a privacy-preserving hostname hash (opt-in)

### Clock

```rust
pub trait Clock: Send + Sync {
    fn now_rfc3339(&self) -> String;
}
```

- MUST return current time in RFC 3339 format
- MUST be deterministic within a single call (no mid-operation drift)

## Determinism Guarantees

perfgate provides the following determinism guarantees:

1. **Receipt determinism**: Given identical command execution results, the same receipt structure MUST be produced (excluding timestamps and run IDs)

2. **Comparison determinism**: Given identical baseline and current receipts with identical budgets, the same comparison result MUST be produced

3. **Report determinism**: Given identical compare receipts, the same report MUST be produced (verified via property tests)

4. **Rendering determinism**: Markdown and annotation output MUST be stable for identical inputs

5. **Export determinism**: CSV and JSONL exports MUST produce identical output for identical inputs, with metrics sorted alphabetically

## Exit Semantics

All perfgate commands MUST use consistent exit codes:

| Code | Meaning | When |
|------|---------|------|
| `0` | Success | Command completed successfully; or warn without `--fail-on-warn`; or no baseline without `--require-baseline` |
| `1` | Tool error | I/O errors, parse failures, spawn failures, missing required arguments |
| `2` | Policy fail | Budget violated (regression exceeds threshold) |
| `3` | Warn as failure | Warn verdict with `--fail-on-warn` flag |

### Exit Code Precedence

When multiple conditions apply:

1. Tool errors (exit 1) take precedence over policy failures
2. Policy failures (exit 2) take precedence over warnings
3. `--fail-on-warn` elevates warnings to exit 3

## Schema Versioning

Receipt types are versioned with string identifiers:

- `perfgate.run.v1` - Run measurement receipt
- `perfgate.compare.v1` - Comparison result
- `perfgate.report.v1` - Cockpit-compatible report envelope
- `perfgate.config.v1` - Configuration file schema

### Versioning Rules

1. The `schema` field in receipts MUST contain the version string
2. Breaking changes REQUIRE a new version (e.g., `v2`)
3. Additive changes with defaults MAY remain in the current version
4. JSON Schema files are generated to `schemas/` directory
