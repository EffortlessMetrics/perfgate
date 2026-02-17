# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Baseline pattern auto-discovery** for `check` via `defaults.baseline_pattern` in config (supports `{bench}` placeholder).
- **Markdown templating** with Handlebars:
  - `perfgate md --template ...`
  - `perfgate report --md-template ...`
  - `perfgate check --md-template ...` (with config fallback `defaults.markdown_template`)
- **GitHub Actions outputs** for `check` via `--output-github`, writing verdict/count outputs to `$GITHUB_OUTPUT`.
- **Official GitHub composite action** (`perfgate-action`) for zero-config setup with optional artifact upload.
- **New export formats**: `html` and `prometheus` alongside existing `csv` and `jsonl`.
- **Cloud baseline backends** for `check` and `promote`:
  - `s3://...` baseline locations
  - `gs://...` baseline locations
- **Schema lock verification** via `xtask schema-check`, with drift detection for missing/modified/extra schema files in `schemas/`.
- **Stabilized `xtask conform` third-party mode**: `--fixtures` now validates all `*.json` files in the provided directory.
- **CI provider guides**: added canonical getting-started docs for GitHub Actions and GitLab CI.

## [0.3.0] - 2026-02-16

### Added

- **Finding fingerprinting** — Each sensor report finding now includes a `fingerprint` field containing the SHA-256 hex digest of a deterministic preimage for collision-resistant deduplication:
  - Metric findings: `sha256("{check_id}:{code}:{metric_name}")`
  - Error findings: `sha256("{check_id}:{code}:{stage}")`
  - Truncation findings: `sha256("tool.truncation:truncated")`
  - Multi-bench findings: `sha256("{bench_name}:{check_id}:{code}:{metric_name}")`

- **Finding truncation** — `SensorReportBuilder` supports configurable `max_findings` limit. When exceeded, findings are truncated and a `tool.truncation:truncated` meta-finding is appended with `{total_findings, shown_findings}` data. Truncation also adds `"truncated"` to `verdict.reasons` and includes `findings_total`/`findings_emitted` in report-level `data`.

- **Schema validation** — New `xtask conform` command validates JSON fixtures against the vendored `sensor.report.v1` schema:
  - `cargo run -p xtask -- conform` validates all golden fixtures
  - `cargo run -p xtask -- conform --file path/to/file.json` validates a single file
  - `cargo run -p xtask -- conform --fixtures path/to/dir` validates all `sensor_report_*.json` files in a directory
  - Integrated into the `xtask ci` pipeline

- **Config presets** — Bundled configuration presets at `presets/`:
  - `standard.toml` — Balanced accuracy and speed (repeat=5, warmup=1, threshold=20%)
  - `release.toml` — High accuracy with tight threshold (repeat=10, warmup=2, threshold=10%)
  - `tier1-fast.toml` — Quick validation with wide threshold (repeat=3, warmup=1, threshold=30%)

- **Contract fixtures** — Golden sensor report fixtures at `contracts/fixtures/` covering pass, fail, warn, error, no-baseline, and multi-bench scenarios

- **Constants** — `CHECK_ID_TOOL_TRUNCATION`, `FINDING_CODE_TRUNCATED`, `VERDICT_REASON_TRUNCATED`

### Changed

- **ABI hardening for sensor.report.v1** — Cockpit output conforms to the fleet contract:
  - `SensorReport.data` and `SensorFinding.data` changed from typed structs to opaque `serde_json::Value`
  - Removed `SensorReportData` struct; data section now contains only `summary` (no `compare` key)
  - Added `Skipped` variant to `CapabilityStatus`
  - Error reports use `tool.runtime` check_id, `runtime_error` code, and structured `{stage, error_kind}` data
  - Baseline reason normalized to `no_baseline` token instead of freeform path strings
  - Extras files renamed to versioned format: `perfgate.run.v1.json`, `perfgate.compare.v1.json`, `perfgate.report.v1.json`
  - Artifacts sorted by `(type, path)` for deterministic output
  - Cockpit `--all` mode properly aggregates across multiple benchmarks with per-bench extras subdirectories
  - Sensor report schema vendored at `contracts/schemas/sensor.report.v1.schema.json` (no longer auto-generated)
  - Added `serde_json` as regular dependency of `perfgate-types` (was dev-only)
  - Added constants: `CHECK_ID_TOOL_RUNTIME`, `FINDING_CODE_RUNTIME_ERROR`, `VERDICT_REASON_TOOL_ERROR`, stage/error_kind constants, `BASELINE_REASON_NO_BASELINE`

## [0.2.0] - 2026-02-05

### Added

- **New CLI commands**
  - `perfgate check` - Config-driven one-command workflow that runs bench, compares to baseline, and generates all artifacts
  - `perfgate report` - Generate perfgate.report.v1 envelope from compare receipt for cockpit integration
  - `perfgate promote` - Copy/normalize a run receipt to become a new baseline
  - `perfgate export` - Export run receipt data to CSV or JSONL format

- **Paired benchmarking mode** - Run baseline and current benchmarks in interleaved pairs for more accurate comparisons on noisy systems

- **CPU time tracking** - Collect user and system CPU time metrics on Unix platforms via `rusage`

- **Host mismatch detection** - Warn when baseline and current runs were executed on different hosts

- **New schemas**
  - `perfgate.report.v1` schema for cockpit-compatible report envelopes
  - `perfgate.config.v1` schema for TOML configuration files
  - `sensor.report.v1` schema for sensor integration

- **Configuration options**
  - Canonical artifact layout: `artifacts/perfgate/{run.json, compare.json, report.json, comment.md}`
  - `--require-baseline` flag for check command to fail when baseline is missing
  - `--out-dir` option for check command to specify artifact output directory
  - `--paired` flag for paired benchmarking mode
  - `--cockpit` flag for cockpit-compatible output format

### Changed

- Check command now generates all artifacts in one invocation (run.json, compare.json, report.json, comment.md)

## [0.1.0] - 2026-02-01

Initial release of perfgate, a CLI tool for performance budgets and baseline diffs in CI.

### Added

- **Core CLI commands**
  - `perfgate run` - Execute a command and collect performance metrics (wall time, max RSS on Unix)
  - `perfgate compare` - Compare current metrics against a baseline with configurable thresholds
  - `perfgate md` - Render comparison results as a Markdown table for PR comments
  - `perfgate github-annotations` - Output GitHub Actions annotations for CI integration

- **Clean architecture workspace**
  - `perfgate-types` - Receipt and config types with JSON Schema support via `schemars`
  - `perfgate-domain` - Pure, I/O-free statistics and budget policy logic
  - `perfgate-adapters` - Process runner with platform-specific metrics collection
  - `perfgate-app` - Use-cases and rendering logic
  - `perfgate-cli` - Clap-based CLI with JSON I/O
  - `xtask` - Repository automation (schema generation, mutation testing, CI checks)

- **Versioned JSON receipts**
  - `perfgate.run.v1` schema for run results
  - `perfgate.compare.v1` schema for comparison results
  - JSON Schema generation via `cargo run -p xtask -- schema`

- **Configurable comparison policy**
  - `--threshold` for maximum allowed regression percentage
  - `--warn-factor` for early warning detection
  - `--fail-on-warn` to treat warnings as failures
  - Distinct exit codes: 0 (pass), 1 (error), 2 (fail), 3 (warn as failure)

- **Measurement features**
  - Configurable `--repeat` count for statistical significance
  - `--warmup` iterations to prime caches
  - `--work` units for throughput calculation
  - `--timeout` support on Unix platforms

- **Comprehensive testing infrastructure**
  - BDD tests with Cucumber for all CLI commands
  - Property-based tests with `proptest` for serialization and statistics
  - Fuzz targets with `cargo-fuzz` for robustness testing
  - Mutation testing support via `cargo-mutants` with per-crate kill rate targets

- **CI workflows**
  - Automated testing on pull requests
  - Mutation testing workflow
  - Fuzzing workflow

- **Documentation**
  - README with quickstart guide and architecture overview
  - CONTRIBUTING guide with development workflow
  - TESTING guide with comprehensive testing strategy
  - Mutation testing documentation with troubleshooting

### Platform Notes

- Timeout support requires Unix (uses `wait4` with `WNOHANG` polling)
- `max_rss_kb` collection only works on Unix via `rusage`
- BDD tests skip `@unix` tagged scenarios on Windows

[Unreleased]: https://github.com/EffortlessMetrics/perfgate/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/EffortlessMetrics/perfgate/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/EffortlessMetrics/perfgate/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/EffortlessMetrics/perfgate/releases/tag/v0.1.0
