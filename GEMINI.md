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
The perfgate v2.x architecture is modularized into 25 specialized crates:

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
| `xtask` | Repository automation for local workflows and CI |

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
Minimum kill rates by category:
- **Core Domain** (`domain`, `types`, `budget`, `stats`): **95-100%**
- **Application** (`app`, `client`, `server`): **90%**
- **Adapters & Infrastructure** (`adapters`, `host-detect`, `paired`): **80-85%**
- **Presentation** (`export`, `render`, `sensor`): **80%**
- **CLI** (`cli`): **70%**

## Key Files & Artifacts
- `perfgate.toml`: Default configuration file for `check` workflows.
- `artifacts/perfgate/`: Default directory for generated receipts and reports.
- `contracts/`: Vendored schemas and fixtures for external integration (e.g., Cockpit).
- `baselines/`: Recommended storage for performance baseline receipts.
