# Agent Context: perfgate

This file provides guidance to autonomous agents when working with code in this repository.

## Build and Test Commands

```bash
# Build all crates
cargo build --all

# Run all tests (unit, integration, property-based, BDD)
cargo test --all

# Run BDD/cucumber tests specifically
cargo test --test cucumber

# Run tests for a specific crate
cargo test -p perfgate-domain
cargo test -p perfgate-types
cargo test -p perfgate-app
cargo test -p perfgate-cli
cargo test -p perfgate-server
cargo test -p perfgate-client
cargo test -p perfgate-budget
cargo test -p perfgate-export
cargo test -p perfgate-render
cargo test -p perfgate-sensor
cargo test -p perfgate-significance
cargo test -p perfgate-stats
cargo test -p perfgate-adapters
cargo test -p perfgate-host-detect
cargo test -p perfgate-paired
cargo test -p perfgate-error
cargo test -p perfgate-validation
cargo test -p perfgate-sha256
cargo test -p perfgate-fake
cargo test -p perfgate-auth
cargo test -p perfgate-config
cargo test -p perfgate-api
cargo test -p perfgate-summary
cargo test -p perfgate-selfbench

# Run a single test by name
cargo test test_name

# Format and lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# Full CI check (fmt, clippy, test, schema generation)
cargo run -p xtask -- ci

# Generate JSON schemas to schemas/
cargo run -p xtask -- schema

# Validate fixtures against vendored schema
cargo run -p xtask -- conform
cargo run -p xtask -- conform --file path/to/report.json
cargo run -p xtask -- conform --fixtures path/to/dir

# Run mutation testing (requires cargo-mutants installed)
cargo run -p xtask -- mutants
cargo run -p xtask -- mutants --crate perfgate-domain --summary

# Run the CLI
cargo run -p perfgate -- --help
cargo run -p perfgate -- run --name bench --out out.json -- echo hello
cargo run -p perfgate -- compare --baseline base.json --current cur.json --out cmp.json
cargo run -p perfgate -- md --compare cmp.json
cargo run -p perfgate -- github-annotations --compare cmp.json
cargo run -p perfgate -- report --compare cmp.json --out report.json
cargo run -p perfgate -- promote --current out.json --to baselines/bench.json
cargo run -p perfgate -- export --run out.json --format csv --out data.csv
cargo run -p perfgate -- check --config perfgate.toml --bench my-bench
cargo run -p perfgate -- check --config perfgate.toml --bench my-bench --mode cockpit
cargo run -p perfgate -- paired --name my-bench --baseline-cmd "echo baseline" --current-cmd "echo current" --repeat 10 --out cmp.json
cargo run -p perfgate -- baseline list --project my-project
cargo run -p perfgate -- summary cmp.json
cargo run -p perfgate -- aggregate run1.json run2.json --out aggregated.json
cargo run -p perfgate -- bisect --good abc123 --bad HEAD --executable ./target/release/my-bench
cargo run -p perfgate -- blame --baseline old-Cargo.lock --current Cargo.lock
cargo run -p perfgate -- explain --compare cmp.json
```

## Fuzzing (requires nightly)

```bash
cd fuzz
cargo +nightly fuzz list
cargo +nightly fuzz run parse_run_receipt
```

## Architecture

This is a clean-architecture Rust workspace for performance budgets and baseline diffs in CI. The architecture is modularized into 26 workspace crates:

| Crate | Responsibility |
|-------|----------------|
| `perfgate-types` | Core domain types and stable schemas |
| `perfgate-error` | Shared error types and categorization |
| `perfgate-domain` | Core business logic and statistical computations |
| `perfgate-stats` | Descriptive statistics (median, p95, etc.) |
| `perfgate-significance` | Statistical significance testing (Welch's t-test) |
| `perfgate-budget` | Budget evaluation and verdict logic |
| `perfgate-sha256` | Minimal SHA-256 implementation for fingerprints |
| `perfgate-host-detect` | Host fingerprinting and mismatch detection |
| `perfgate-validation` | Schema validation and contract testing |
| `perfgate-adapters` | Low-level system adapters (rusage, process execution) |
| `perfgate-paired` | Paired benchmarking implementation |
| `perfgate-auth` | Authentication and authorization types |
| `perfgate-api` | API types and models for baseline service |
| `perfgate-config` | Configuration loading and merging logic |
| `perfgate-app` | Orchestration layer for CLI commands |
| `perfgate-summary` | Summarization of comparison receipts |
| `perfgate-render` | Markdown and terminal rendering |
| `perfgate-export` | Multi-format export (CSV, JSONL, HTML, Prometheus, JUnit) |
| `perfgate-sensor` | Cockpit mode and sensor report generation |
| `perfgate-server` | Centralized Baseline Service API (REST/Axum) |
| `perfgate-client` | Client library for Baseline Service interaction |
| `perfgate-cli` | Command-line interface and argument parsing |
| `perfgate` | Unified facade library (re-exports core crates) |
| `perfgate-fake` | Test fixtures and mock data generators |
| `perfgate-selfbench` | Internal benchmarking workloads for self-dogfooding |
| `xtask` | Repository automation (schemas, CI, conformance, mutants) |

**Key design principles:**
- `perfgate-domain` is intentionally I/O-free: it does statistics and budget policy only
- `perfgate-adapters` contains platform-specific code (Unix `wait4()` for `max_rss_kb`)
- Receipt types are versioned (`perfgate.run.v1`, `perfgate.compare.v1`, `perfgate.report.v1`) and have JSON Schema support via `schemars`
- The `arbitrary` feature flag enables structure-aware fuzzing

**Exit codes (all commands):**
- `0`: success (or warn without `--fail-on-warn`)
- `1`: tool/runtime error (I/O, parse, spawn failures)
- `2`: policy fail (budget violated)
- `3`: warn treated as failure (with `--fail-on-warn`)

**Canonical artifact layout:**
```
artifacts/perfgate/
├── run.json        # perfgate.run.v1
├── compare.json    # perfgate.compare.v1
├── report.json     # perfgate.report.v1 (cockpit ingestion)
└── comment.md      # PR comment markdown
```

## Testing Strategy

- **Property-based tests**: Use `proptest` in `perfgate-types` and `perfgate-app` for serialization round-trips and rendering completeness
- **BDD tests**: Cucumber feature files in `features/` with step definitions in `tests/cucumber.rs`
- **Integration tests**: CLI tests in `crates/perfgate-cli/tests/`
- **Mutation testing**: Target kill rates by category:
  - Core Domain (`domain`, `types`, `budget`, `stats`): **95-100%**
  - Application (`app`, `client`, `server`): **90%**
  - Adapters & Infrastructure (`adapters`, `host-detect`, `paired`): **80-85%**
  - Presentation (`export`, `render`, `sensor`): **80%**
  - CLI (`cli`): **70%**

## Platform Notes

- Timeout support requires Unix (uses `wait4` with `WNOHANG` polling)
- On non-Unix platforms, timeouts return `AdapterError::TimeoutUnsupported`
- `max_rss_kb` collection only works on Unix via `rusage`
- BDD tests skip `@unix` tagged scenarios on Windows
