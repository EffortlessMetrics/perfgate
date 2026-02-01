# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
cargo test -p perfgate-adapters
cargo test -p perfgate-cli

# Run a single test by name
cargo test test_name

# Format and lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# Full CI check (fmt, clippy, test, schema generation)
cargo run -p xtask -- ci

# Generate JSON schemas to schemas/
cargo run -p xtask -- schema

# Run mutation testing (requires cargo-mutants installed)
cargo run -p xtask -- mutants
cargo run -p xtask -- mutants --crate perfgate-domain --summary

# Run the CLI
cargo run -p perfgate -- --help
cargo run -p perfgate -- run --name bench --out out.json -- echo hello
cargo run -p perfgate -- compare --baseline base.json --current cur.json --out cmp.json
cargo run -p perfgate -- md --compare cmp.json
cargo run -p perfgate -- github-annotations --compare cmp.json
```

## Fuzzing (requires nightly)

```bash
cd fuzz
cargo +nightly fuzz list
cargo +nightly fuzz run parse_run_receipt
```

## Architecture

This is a clean-architecture Rust workspace for performance budgets and baseline diffs in CI.

**Crate dependency flow (inner to outer):**

```
perfgate-types (receipt/config structs, JSON schema)
       ↓
perfgate-domain (pure math/policy, I/O-free)
       ↓
perfgate-adapters (process runner, system metrics)
       ↓
perfgate-app (use-cases, markdown/annotation rendering)
       ↓
perfgate-cli (clap CLI, JSON I/O)
```

**Key design principles:**
- `perfgate-domain` is intentionally I/O-free: it does statistics and budget policy only
- `perfgate-adapters` contains platform-specific code (Unix `wait4()` for `max_rss_kb`)
- Receipt types are versioned (`perfgate.run.v1`, `perfgate.compare.v1`) and have JSON Schema support via `schemars`
- The `arbitrary` feature flag enables structure-aware fuzzing

**Exit codes for `compare` command:**
- `0`: pass (or warn without `--fail-on-warn`)
- `1`: tool error (I/O, parse, spawn failures)
- `2`: fail (budget violated)
- `3`: warn treated as failure (with `--fail-on-warn`)

## Testing Strategy

- **Property-based tests**: Use `proptest` in `perfgate-types` and `perfgate-app` for serialization round-trips and rendering completeness
- **BDD tests**: Cucumber feature files in `features/` with step definitions in `tests/cucumber.rs`
- **Integration tests**: CLI tests in `crates/perfgate-cli/tests/`
- **Mutation testing**: Target kill rates by crate (domain: 100%, types: 95%, app: 90%, adapters: 80%, cli: 70%)

## Platform Notes

- Timeout support requires Unix (uses `wait4` with `WNOHANG` polling)
- On non-Unix platforms, timeouts return `AdapterError::TimeoutUnsupported`
- `max_rss_kb` collection only works on Unix via `rusage`
- BDD tests skip `@unix` tagged scenarios on Windows
