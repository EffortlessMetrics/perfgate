# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **Security Scoping** — API keys can now be restricted to specific projects and benchmark name patterns (regex).
- **Project Isolation** — The baseline server now enforces strict project-level isolation. Keys without global admin scope are restricted to their assigned project.
- **Enhanced CLI** — `perfgate-server` now supports expanded API key definitions: `--api-keys role:key:project:regex`.

### Fixed
- **Integration Test schema compatibility** — Updated mock server responses to match the v0.5.0 schema requirements (tags, promoted_at).
- **Baseline visibility** — Fixed logic that incorrectly hid baselines with zero samples from summaries.

## [0.5.0] - 2026-03-16

### Added
- **Self-Dogfooding Infrastructure** — `perfgate` now uses itself to gate its own performance across three CI lanes (Smoke, Perf, and Nightly).
- **Multi-Lane CI Workflows** — Implemented `perfgate-self.yml` and `perfgate-nightly.yml` with unique artifact naming and authoritative runner pinning (`ubuntu-24.04`).
- **Hardened Workload Wrappers** — Introduced `.ci/perf/lib.sh` for shared binary resolution and strict exit code classification (allowing 0, 2, 3 while failing on crashes).
- **Automated Baseline Lifecycle** — Nightly calibration now generates candidate baselines and automatically proposes refreshes via bot-driven Pull Requests.
- **Learning Loop & Trends** — Added trend export to JSONL and Prometheus formats in the nightly lane for long-term drift analysis.
- **Paired Observation Lane** — New "PR-vs-Main" lane dogfoods interleaved benchmarking by comparing the current binary directly against the last blessed `main` binary.
- **Enhanced Repo Automation** — Added `xtask dogfood` subcommands for fixture regeneration and artifact verification, plus a framework for `docs-sync`.
- **New Micro-crate** — Introduced `perfgate` facade crate as the high-level entrypoint for the ecosystem.

## [0.4.1] - 2026-03-12

### Changed
- **Architectural Decoupling** — Successfully moved core business logic (baseline resolution, budget building, and verdict calculation) from the CLI into `perfgate-app` modules for better reusability.
- **Dependency Standardization** — All internal crate dependencies now consistently use `workspace = true` for easier maintenance.
- **Improved CLI Orchestration** — Refactored large CLI functions to use a clean `CheckConfig` struct, reducing complexity.

### Fixed
- **CI Stability** — Resolved schema drift issues caused by cross-platform line ending differences.
- **Documentation** — Added missing `baseline` command details to the CLI crate README.

## [0.4.0] - 2026-03-12

### Added

- **Standardized API Versioning** — Migrated the `perfgate-server` REST API to a versioned `/api/v1` namespace for long-term stability.
- **REST API Endpoints** — Implemented comprehensive baseline management via REST:
  - `POST /api/v1/projects/{project}/baselines` - Upload baseline
  - `GET /api/v1/projects/{project}/baselines/{benchmark}/latest` - Get latest baseline
  - `GET /api/v1/projects/{project}/baselines` - Filtered/paginated list of baselines
  - `POST /api/v1/projects/{project}/baselines/{benchmark}/promote` - Promote baseline version
  - `DELETE /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}` - Soft delete baseline
- **Operational Health** — Exposed `/health` at root and `/api/v1/health` for monitoring and load balancer integration.
- **PostgreSQL Storage (Preview)** — Initial storage adapter skeleton for PostgreSQL persistence in `perfgate-server`.
- **Windows Parity** — Added `page_faults` collection to Windows best-effort metrics via `GetProcessMemoryInfo`.
- **E2E Integration Suite** — Added a real-world server integration test suite (`real_server_integration.rs`) that verifies full workflows against a live in-memory instance.
- **Test Utilities Feature** — Introduced `test-utils` feature in `perfgate-server` to expose internal assembly helpers for integration tests without widening the default public API surface.
- **CLI Mock Server Tests** — Added `cli_mock_server_tests.rs` utilizing `wiremock` to validate CLI client behavior in isolation.
- **Full BDD Coverage** — New `baseline_command.feature` ensuring all new baseline management subcommands are verified via user-facing scenarios.
- **Baseline Pattern Auto-discovery** — New `defaults.baseline_pattern` in config (supports `{bench}` placeholder) for `check` workflow.
- **Markdown Templating** — Support for Handlebars templates in `md`, `report`, and `check` commands via `--template`.
- **GitHub Actions Integration** — Added `--output-github` to `check` command for native GITHUB_OUTPUT support.
- **Cloud Baseline Backends** — Support for `s3://` and `gs://` baseline locations in `check` and `promote`.
- **Per-metric Statistic Selection** — Support for gating on specific statistics (e.g., `P95` wall time) via `--metric-stat` or config.
- **Statistical Significance Analysis** — Optional Welch's t-test integration for detecting statistically relevant regressions.
- **Ecosystem Documentation** — Aligned all READMEs, diagrams, and guides with the 19-crate micro-architecture; added ADRs 0001-0005.

### Changed

- **Improved Client Robustness** — `perfgate-client` now automatically normalizes server URLs to ensure trailing slashes, preventing path segment stripping.
- **Store Parity** — Hardened `InMemoryStore` to maintain 100% feature parity with the SQLite backend, including all complex query filters.
- **Enhanced CONTRIBUTING.md** — Added comprehensive PR checklist, code style guide, and testing requirements.
- **Mutation Testing Targets** — Expanded `mutants.toml` to cover all 19 workspace crates for CI enforcement.

### Fixed

- **Server SQLite Pagination** — Resolved a critical bug where record counts were miscalculated when filters were active.
- **Pagination Defaults** — Fixed `ListBaselinesQuery::default()` to correctly default `limit` to 50 instead of 0 in both client and server.
- **Auth Middleware Reliability** — Fixed path matching to correctly handle nested and aliased health check routes.
- **Test Stability** — Standardized API keys to be strictly alphanumeric and normalized CLI error output assertions across platforms.

## [0.3.0] - 2026-02-16

### Added

- **Finding fingerprinting** — Deterministic SHA-256 digests for finding deduplication.
- **Finding truncation** — Support for `max_findings` limit in `SensorReportBuilder`.
- **Schema validation** — New `xtask conform` command for JSON fixture validation.
- **Config presets** — Bundled configuration presets at `presets/` (standard, release, tier1-fast).

### Changed

- **ABI hardening for sensor.report.v1** — Cockpit output conforms to the fleet contract.
- **Versioned Artifacts** — Extras files renamed to `perfgate.run.v1.json`, etc.

## [0.2.0] - 2026-02-05

### Added

- **New CLI commands**: `check`, `report`, `promote`, `export`.
- **Paired benchmarking mode** for interleaved A/B comparisons.
- **CPU time tracking** on Unix platforms via `rusage`.
- **Host mismatch detection** warning.

## [0.1.0] - 2026-02-01

Initial release of perfgate, a CLI tool for performance budgets and baseline diffs in CI.
