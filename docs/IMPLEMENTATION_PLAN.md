# perfgate Implementation Plan

This document serves as a maintenance plan for the perfgate codebase, describing evolution guidelines, schema versioning strategy, and current architectural status.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

## Contract Changes

### Schema Versioning Policy

**Breaking changes REQUIRE a v2 schema.**

A change is considered breaking if it:
- Removes a required field
- Changes the type of an existing field
- Changes the semantic meaning of an existing field
- Removes an enum variant
- Changes the default behavior in a way that invalidates existing receipts

**Additive changes MAY remain in the current version** if they:
- Add a new optional field with `#[serde(default)]`
- Add a new enum variant (consumers SHOULD handle unknown variants gracefully)
- Add new commands that don't affect existing artifacts

### Versioning Process

When creating a new schema version:

1. Create new type definitions (e.g., `RunReceiptV2`)
2. Define new schema constant (e.g., `RUN_SCHEMA_V2`)
3. Update CLI to write new version by default
4. Maintain backward-compatible reading of v1 schemas
5. Generate new JSON Schema file to `schemas/`
6. Update documentation to reflect changes

### Current Schema Versions (v0.x)

| Schema | Version | Status |
|--------|---------|--------|
| `perfgate.run.v1` | 1 | Current |
| `perfgate.compare.v1` | 1 | Current |
| `perfgate.report.v1` | 1 | Current |
| `perfgate.config.v1` | 1 | Current |
| `sensor.report.v1` | 1 | Current (Cockpit Mode) |
| `perfgate.baseline.v1`| 1 | Current (Baseline Server) |

## Deterministic Ordering

### New Metrics Policy

**New metrics MUST include deterministic ordering.**

When adding a new metric type:

1. Add variant to `Metric` enum
2. Implement `Ord` for the variant (determines BTreeMap ordering)
3. Update `metric_to_string()` in all modules that use it
4. Add default direction via `default_direction()`
5. Add default warn factor via `default_warn_factor()`
6. Add display unit via `display_unit()`
7. Update export column ordering documentation

### Ordering Invariants

- `BTreeMap<Metric, _>` MUST be used for all metric collections
- Export functions MUST sort metrics alphabetically by string name
- Report findings MUST be ordered by metric (BTreeMap iteration order)
- These orderings MUST be verified by property tests

## Architectural Components (22-Crate Workspace)

The perfgate architecture is modularized into 22 specialized crates:

| Crate | Responsibility |
|-------|----------------|
| `perfgate-types` | Core domain types and stable schemas |
| `perfgate-domain` | Core business logic and statistical computations |
| `perfgate-app` | Orchestration layer for CLI commands |
| `perfgate-cli` | Command-line interface and argument parsing |
| `perfgate-adapters` | Low-level system adapters (rusage, process execution) |
| `perfgate-server` | Centralized Baseline Service API (REST/Axum) |
| `perfgate-client` | Client library for Baseline Service interaction |
| `perfgate-budget` | Budget evaluation and verdict logic |
| `perfgate-export` | Multi-format export (CSV, JSONL, HTML, Prometheus) |
| `perfgate-render` | Markdown and terminal rendering |
| `perfgate-sensor` | Cockpit mode and sensor report generation |
| `perfgate-significance` | Statistical significance testing (Welch's t-test) |
| `perfgate-stats` | Descriptive statistics (median, p95, etc.) |
| `perfgate-host-detect` | Host fingerprinting and mismatch detection |
| `perfgate-paired` | Paired benchmarking implementation |
| `perfgate-error` | Shared error types and categorization |
| `perfgate-validation` | Schema validation and contract testing |
| `perfgate-sha256` | Minimal SHA-256 implementation for fingerprints |
| `perfgate-fake` | Test fixtures and mock data generators |
| `perfgate-selfbench` | Internal benchmarking workloads for self-dogfooding |
| `perfgate-summary` | Summarization logic for comparison receipts |
| `perfgate` | Unified facade library |

## Implementation Status

### Baseline Server API

**Status:** Implemented (v0.4.0)

Centralized storage for fleet-scale performance monitoring:
- REST API with Axum and multi-backend support (SQLite, PostgreSQL planned)
- Multi-tenancy (projects) and versioned baselines
- Client-side fallback to local/cloud storage for resilience
- `baseline` command group for management

### Cockpit Mode

**Status:** Implemented (v0.5.0)

Standardized integration for monitoring dashboards:
- `--mode cockpit` wraps output in `sensor.report.v1` envelope
- Stable exit code 0 for budget violations
- Versioned artifacts in `extras/` subdirectory
- Deterministic finding fingerprints

### Paired Mode

**Status:** Implemented (v0.5.0)

Interleaved baseline/current runs to reduce environmental noise:
- `perfgate paired --baseline-cmd "..." --current-cmd "..."`
- Domain logic in `perfgate-domain`, app orchestration in `perfgate-app`

### Host Mismatch Policy

**Status:** Implemented (v0.5.0)

Detection of OS, arch, CPU, and hostname differences:
- `--host-mismatch` policy support (`warn`, `error`, `ignore`)
- Implemented in `perfgate-host-detect`

### Additional Metrics

**Status:** Implemented (v0.5.0)

1. **CPU time** (`cpu_ms`): Combined user and system CPU time
2. **Page faults** (`page_faults`): Major page faults
   - Direction: Lower
   - Platform: Unix + best-effort Windows (via `GetProcessMemoryInfo`)
3. **Context switches** (`ctx_switches`): Voluntary + involuntary (Unix only)
4. **Binary size** (`binary_bytes`): Executable size tracking

### Multi-Format Export

**Status:** Implemented (v0.5.0)

The `export` command supports multiple output formats:
- **CSV**: RFC 4180 compliant for spreadsheet analysis
- **JSONL**: Newline-delimited JSON for log processing
- **HTML**: Self-contained tabular reports
- **Prometheus**: Text exposition format for monitoring systems

## Testing Requirements

### Property Test Coverage

When making changes, ensure property tests cover:

1. **Serialization round-trips**: All types MUST serialize/deserialize correctly
2. **Statistics ordering**: `min <= median <= max` MUST hold
3. **Warmup exclusion**: Warmup samples MUST NOT affect statistics
4. **Report determinism**: Same input MUST produce same output
5. **Export ordering**: Metrics MUST be sorted alphabetically

### Mutation Testing Targets (21-Crate)

Minimum kill rates by crate:

| Crate Category | Target Kill Rate |
|----------------|-----------------|
| Core Domain (`domain`, `types`, `budget`, `stats`) | 95-100% |
| Application (`app`, `client`, `server`) | 90% |
| Adapters & Infrastructure (`adapters`, `host-detect`, `paired`) | 80-85% |
| Presentation (`export`, `render`, `sensor`) | 80% |
| CLI (`cli`) | 70% |

## Deprecation Policy

When deprecating functionality:

1. **Announce**: Add deprecation notice to CHANGELOG
2. **Warn**: Emit runtime warning for one minor version
3. **Remove**: Remove in next major version

## Code Style

### Error Handling

- Use `anyhow` for CLI-level errors
- Use `thiserror` for domain/adapter error types
- Domain errors MUST NOT leak implementation details
- Adapter errors SHOULD include platform context
