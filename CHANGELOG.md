# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/EffortlessMetrics/perfgate/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/EffortlessMetrics/perfgate/releases/tag/v0.1.0
