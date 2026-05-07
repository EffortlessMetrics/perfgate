# perfgate Architecture

This document describes the architectural design of perfgate, a selective build-truth sensor for performance budgets in CI pipelines.

## Role Statement

**perfgate is a selective build-truth sensor.** It gates merges on explicit performance budgets by comparing black-box command receipts to baselines.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

## Ecosystem Role and Cockpit Integration

perfgate is the selective performance gate sensor. It answers one question: "Did this change regress end-to-end performance beyond an explicit budget?"

Default lane posture:
- Label-gated or opt-in by workflow
- Non-blocking by default, but visible when it runs
- Missing baselines are warnings, not passes

Cockpit ingest contract:
- Cockpit reads `artifacts/perfgate/report.json` as the canonical output
- `artifacts/perfgate/compare.json` is absent when no baseline exists

Recommended cockpit policy defaults:
```toml
[sensors.perfgate]
blocking = false
missing = "skip"
require_label = "run-perf"
```

## Truth Layer

perfgate operates as a **build truth** component: it measures and reports performance characteristics of arbitrary commands without understanding their internals. This is a selective sensor approach:

- **Black-box measurement**: perfgate measures wall-clock time, memory usage (RSS), and derived throughput without instrumenting the target command
- **Explicit budgets**: Regression thresholds are user-defined, not inferred
- **Deterministic verdicts**: Given the same inputs, perfgate MUST produce the same verdict

## Non-Goals

perfgate intentionally avoids these responsibilities:

1. **Mandatory baseline service**: perfgate core does NOT require a centralized server. Users MAY use the optional baseline server for centralized management, but file-based and cloud storage baselines remain fully supported

2. **General-purpose profiler**: perfgate is not a profiler-first system and does NOT instrument every run or automatically identify hot paths. It measures whole-command execution first, and may optionally capture a flamegraph after a warn/fail regression for follow-up diagnosis

3. **Test runner/director**: perfgate does NOT orchestrate test suites or manage parallelism. It runs a single command specification

4. **Heavy inferential modeling**: perfgate does NOT perform complex model-based inference or confidence-interval tuning. It supports optional Welch p-value analysis and simple threshold-based policy.

5. **Host normalization**: perfgate does NOT normalize measurements across different hardware. Host fingerprinting is informational only

## Architectural Decision Records

Significant architectural changes are documented in [ADRs](adrs/). See:
- [ADR 0001: Workspace Modularization and Micro-crates](adrs/0001-workspace-modularization-and-micro-crates.md)
- [ADR 0002: Domain Logic Split (Budget, Stats, Significance)](adrs/0002-domain-logic-split-budget-stats-significance.md)
- [ADR 0003: Presentation Layer Split (Render, Export, CLI)](adrs/0003-presentation-layer-split-render-export-cli.md)

## Crate Boundaries

perfgate keeps strict clean-architecture boundaries, but those boundaries are
no longer intended to map one-to-one to public crates. The 0.16 public-surface
contract is:

| Public package | Role |
|----------------|------|
| `perfgate` | Main embeddable facade |
| `perfgate-cli` | Installs the `perfgate` binary |
| `perfgate-types` | Stable receipts, schemas, config, and API contracts |
| `perfgate-client` | Baseline service client |
| `perfgate-server` | Baseline service binary/library |

The remaining workspace packages are internal seams, private test/dev packages,
or workspace-only compatibility wrappers. The current policy files are:

- [`policy/public_crates.txt`](../policy/public_crates.txt)
- [`policy/absorbed_crates.txt`](../policy/absorbed_crates.txt)
- [`docs/CRATE_SEAMS.md`](CRATE_SEAMS.md)

Architecture enforcement is executable:

```bash
cargo run -p xtask -- public-surface
cargo run -p xtask -- arch
```

`cargo run -p xtask -- public-surface --strict` is the final release gate. It
now passes on `main` and enforces the five-package publishable allowlist above.

### Component Layers

The current source layout still has named crates for several internal seams.
Treat this diagram as the enforced dependency direction, not as a public API
promise:

```
┌─────────────────────────────────────────────────────────────────┐
│                    perfgate-cli / perfgate-server                │
│                    (outer adapters and binaries)                 │
├─────────────────────────────────────────────────────────────────┤
│       perfgate::presentation::{render, export, sensor}           │
│                    (Presentation Layer)                         │
├─────────────────────────────────────────────────────────────────┤
│                 perfgate::app | perfgate-client                  │
│                    (Use-Case Orchestration)                     │
├─────────────────────────────────────────────────────────────────┤
│                       perfgate::runtime                         │
│                    (Infrastructure/IO seam)                     │
├─────────────────────────────────────────────────────────────────┤
│                       perfgate::domain                          │
│                    (Domain Business Logic)                      │
├─────────────────────────────────────────────────────────────────┤
│                         perfgate-types                          │
│                    (Core Types & Validation)                    │
└─────────────────────────────────────────────────────────────────┘
```

### Dependency Flow

Dependencies flow inward toward the core types and domain logic:

1. **Core**: `perfgate-types` is the stable receipt/config and validation foundation.
2. **Domain**: `perfgate::domain` owns statistics, significance testing, host mismatch detection, budget evaluation, scaling analysis, and comparison policy.
3. **Infrastructure**: `perfgate::runtime` provides the "outer" world access (process execution, system info).
4. **App**: `perfgate::app` wires together domain logic and infrastructure to fulfill user requests.
5. **Presentation**: `perfgate::presentation::{render, export, sensor}` formats the results for various consumers, with workspace-only compatibility wrappers preserving the previous crate names during the 0.16 migration.
6. **CLI**: `perfgate-cli` is the thin entry point.

### Internal Seam Responsibilities

- **perfgate::domain::stats**: Pure statistical aggregators (U64Summary, F64Summary).
- **perfgate::domain::significance**: P-value and statistical significance testing.
- **perfgate::domain::budget**: Logic for comparing metrics against thresholds.
- **perfgate::domain::scaling**: Complexity model fitting and scaling validation.
- **perfgate::presentation::render**: Markdown and terminal rendering logic; `perfgate-render` is a compatibility wrapper.
- **perfgate::presentation::export**: Multi-format data exporters (CSV, Prometheus, etc.); `perfgate-export` is a compatibility wrapper.
- **perfgate::presentation::sensor**: Sensor report envelopes for cockpit-style integrations; `perfgate-sensor` is a compatibility wrapper.
- **perfgate-types::error**: Shared error taxonomy; `perfgate-error` is a compatibility wrapper.
- **perfgate-types::fingerprint**: Deterministic fingerprinting for reports.
- **perfgate::runtime**: Runtime process execution, host probing, and platform metrics; `perfgate-adapters` is a workspace-only compatibility wrapper.

### Baseline Service Stack

```
┌─────────────────────────────────────────────────────────────────┐
│                      perfgate-server                             │
│        (REST API, memory/SQLite/PostgreSQL storage)             │
├─────────────────────────────────────────────────────────────────┤
│                      perfgate-client                             │
│       (API client, fallback storage, retry logic)               │
├─────────────────────────────────────────────────────────────────┤
│                       perfgate-types                             │
│              (receipt/config structs, JSON schema)              │
└─────────────────────────────────────────────────────────────────┘
```

### Dependency Flow

Dependencies flow inward only:

**Core CLI Stack:**
```
perfgate-types (innermost)
       ↓
perfgate::domain
       ↓
perfgate::runtime
       ↓
perfgate::app
       ↓
perfgate-cli (outermost)
```

**Client/Server Stack:**
```
perfgate-types (shared)
       ↓
perfgate-client
       ↓
perfgate-server (standalone)
       ↓
perfgate-cli (integrates client)
```

### Crate Responsibilities

#### perfgate-types

- MUST define all receipt and config data structures
- MUST provide JSON Schema support via `schemars`
- MUST maintain backward compatibility for schema versions
- SHALL NOT perform I/O or contain business logic

#### perfgate::domain

- MUST be I/O-free: statistics and policy only
- MUST implement median computation, delta calculation, and verdict determination
- MUST handle overflow-safe arithmetic for u64 statistics
- MUST implement paired comparison logic for interleaved measurements
- MUST implement host mismatch detection logic
- SHALL NOT depend on external services or filesystem

#### perfgate::runtime

- MUST implement platform-specific code (Unix `wait4()` and best-effort Windows process APIs)
- MUST define trait abstractions for process execution (`ProcessRunner`)
- MUST define trait abstractions for host probing (`HostProbe`)
- MUST define trait abstractions for time (`Clock`)
- SHOULD provide best-effort system metrics
- SHOULD collect CPU time metrics (`cpu_ms`) on Unix via `rusage`
- SHOULD collect best-effort CPU and RSS metrics on Windows

#### perfgate::app

- MUST orchestrate runtime adapters and domain logic
- MUST implement use-cases: run, compare, check, report, promote, export, paired
- MUST generate markdown and GitHub annotation output
- MUST build `sensor.report.v1` envelopes for cockpit mode
- SHALL NOT parse CLI arguments or perform direct filesystem I/O

#### perfgate-cli

- MUST parse CLI arguments using clap
- MUST perform JSON/TOML I/O for receipts and config files
- MUST map domain errors to appropriate exit codes
- SHOULD use atomic writes for output files
- MAY integrate perfgate-client for server-backed baseline operations

#### perfgate-client

- MUST provide async API client for baseline service communication
- MUST implement automatic retry logic with exponential backoff
- MUST support fallback to local storage when server is unavailable
- MUST handle authentication via API keys
- SHALL NOT depend on perfgate-server implementation details

#### perfgate-server

- MUST provide REST API for baseline CRUD operations
- MUST support multiple storage backends (in-memory, SQLite, PostgreSQL)
- MUST implement role-based access control (viewer, contributor, promoter, admin)
- MUST support multi-tenancy via project namespacing
- MUST track baseline version history
- SHALL NOT depend on perfgate-cli

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
- `sensor.report.v1` - Sensor integration envelope (cockpit mode, vendored at `contracts/schemas/`)

### Versioning Rules

1. The `schema` field in receipts MUST contain the version string
2. Breaking changes REQUIRE a new version (e.g., `v2`)
3. Additive changes with defaults MAY remain in the current version
4. JSON Schema files are generated to `schemas/` directory
