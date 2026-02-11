# Gemini Context: perfgate

This file provides comprehensive context for AI interactions within the `perfgate` repository.

## Project Overview

`perfgate` is a high-performance, modular Rust CLI tool designed for **performance budgeting** and **baseline diffing** in CI/PR automation environments. It enables developers to gate pull requests based on performance regressions, using stable JSON receipts and compact Markdown reports.

### Main Technologies
- **Language**: Rust (Workspace-based)
- **Serialization**: `serde`, `serde_json`
- **Schema**: JSON Schema via `schemars`
- **Testing**: `cucumber` (BDD), `proptest` (Property-based), `cargo-fuzz` (Fuzzing), `cargo-mutants` (Mutation testing)
- **System Metrics**: Unix `rusage` (`wait4`), Windows `GlobalMemoryStatusEx`

### Architecture
The project follows a modular workspace architecture with clear separation of concerns:
- `perfgate-types`: Versioned receipt and configuration data structures.
- `perfgate-domain`: Pure logic for statistics computation and budget comparison (I/O-free).
- `perfgate-adapters`: Best-effort system metrics collection and process execution.
- `perfgate-app`: High-level use cases and report rendering (Markdown, GitHub Annotations).
- `perfgate-cli`: CLI interface using `clap`, handling file I/O and orchestration.
- `xtask`: Repository automation for local workflows and CI.

## Building and Running

### Common Commands
- **Build**: `cargo build --workspace`
- **Run CLI**: `cargo run -p perfgate -- [args]`
- **Install Locally**: `cargo install --path crates/perfgate-cli`
- **Help**: `perfgate --help`

### Local Workflow (xtask)
Automation tasks are managed via `xtask` for consistency:
- **Run CI suite**: `cargo run -p xtask -- ci` (clippy, fmt, tests, schemas, conformance)
- **Generate Schemas**: `cargo run -p xtask -- schema` (outputs to `schemas/`)
- **Validate Fixtures**: `cargo run -p xtask -- conform`
- **Sync Fixtures**: `cargo run -p xtask -- sync-fixtures`
- **Run Mutation Tests**: `cargo run -p xtask -- mutants`

## Development Conventions

### Coding Style
- Follow standard Rust idiomatic practices.
- Enforce strict clippy linting: `cargo run -p xtask -- ci` runs clippy with `-D warnings`.
- Documentation should be updated in `docs/` for significant architectural changes.

### Changelog Management
- Update `CHANGELOG.md` under the `[Unreleased]` section for every PR.
- Follow the [Keep a Changelog](https://keepachangelog.com/) format.

### Testing Strategy
`perfgate` employs a rigorous multi-layered testing strategy:
1. **Unit Tests**: For individual functions and edge cases.
2. **Property Tests**: Using `proptest` for algorithmic correctness across universal properties.
3. **BDD Tests**: Using `cucumber` for user-facing CLI behavior (features in `features/`).
4. **Fuzz Tests**: Using `cargo-fuzz` for malformed input robustness.
5. **Mutation Tests**: Using `cargo-mutants` to ensure test effectiveness.

#### Mutation Testing Kill Rate Targets
- `perfgate-domain`: **100%** (Pure logic)
- `perfgate-types`: **95%**
- `perfgate-app`: **90%**
- `perfgate-adapters`: **80%**
- `perfgate-cli`: **70%** (I/O heavy)

## Key Files & Artifacts
- `perfgate.toml`: Default configuration file for `check` workflows.
- `artifacts/perfgate/`: Default directory for generated receipts and reports.
- `contracts/`: Vendored schemas and fixtures for external integration (e.g., Cockpit).
- `baselines/`: Recommended storage for performance baseline receipts.
