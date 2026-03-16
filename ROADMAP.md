# perfgate Roadmap

This document outlines the planned evolution of `perfgate`. It serves as a high-level guide for contributors and users to understand the project's direction.

## Strategic Outlook (2026+)

### Now
- **Ecosystem Maintenance**: Hardening the 19-crate micro-architecture and ensuring stable CI/CD across all platforms. [DONE: Fixed server pagination and test coverage]
- **Documentation Alignment**: Keeping ADRs and crate-level READMEs in sync with the rapidly evolving modular codebase. [DONE: ADRs 0001-0005 created]
- **Windows Parity**: Improving best-effort system metrics (CPU/RSS/PageFaults) for Windows parity with Unix `rusage`. [DONE: page_faults added to Windows best-effort]
- **Self-Dogfooding**: Implementing strict performance gating for `perfgate` development itself, ensuring artifact stability and tool efficiency. [DONE: v0.5.0 rollout]

### Next
- **Schema Evolution**: Planning the transition to `v2` schemas to support more complex multi-dimensional gating.
- **Enhanced Baseline Server**: Adding support for pluggable storage backends (PostgreSQL, S3) and OIDC authentication.
- **Observability**: Native OpenTelemetry export support for direct ingestion into observability platforms.

### Later
- **AI-Driven Analysis**: Exploring LLM-assisted performance regression diagnosis based on historical trends.
- **Distributed Benchmarking**: Orchestrating multi-node performance tests with centralized gating.

## Vision
To become the standard, low-friction "build truth" sensor for performance gating in modern CI/CD pipelines, providing stable, versioned, and actionable performance data.

---

## Current Status (v0.5.x)
- [x] **v0.5.0 Self-Dogfooding**: Continuous performance learning loops and artifact stability gating.
- [x] **v0.4.0 Baseline Server**: Centralized baseline management with RBAC and REST API.
- [x] **Micro-crate Architecture**: Full decomposition into 20 specialized crates for better maintenance.
- [x] **Statistical Significance**: Integrated Welch's t-test and p-value support.
- [x] **Multi-format Export**: CSV, JSONL, HTML, and Prometheus support.
- [x] **Paired Benchmarking**: Interleaved execution to minimize CI noise.
- [x] **Stable Schemas**: Versioned JSON receipts with schema-locking tests.

---

## Milestone 1: v1.0 Core Stabilization
**Target: Q1 2026**
*Focus: Reliability, documentation completeness, and initial ecosystem hardening.*

- [x] **Contract Hardening**
  - [x] Finalize and lock `v1` schemas for receipts and config.
  - [x] Ensure byte-for-byte deterministic output for all report generators.
  - [x] Detect and flag extra/stale files in contract mirror checks.
- [x] **Documentation & DX**
  - [x] Complete documentation alignment (metrics names, CLI flags).
  - [x] Provide canonical "Getting Started" guides for common CI providers (GitHub Actions, GitLab CI).
  - [x] Standardize error stage/kind classifications across all failure paths.
- [x] **Tooling**
  - [x] Deduplicate `xtask` and test helpers into shared library modules.
  - [x] Stabilize `conform` command for third-party sensor validation.

---

## Milestone 2: v1.1 Extended Observability
**Target: Q2 2026**
*Focus: Deeper system metrics and improved platform parity.*

- [x] **New Metrics (Unix)**
  - [x] `page_faults`: Track major page faults to detect memory pressure changes.
  - [x] `ctx_switches`: Track voluntary and involuntary context switches.
- [x] **Static Analysis Metrics**
  - [x] `binary_bytes`: Track changes in compiled executable size.
- [x] **Improved Platform Support**
  - [x] Explore best-effort CPU time and memory tracking for Windows.
  - [x] Support custom environment injection for `paired` mode executions.
- [x] **Configuration Enhancements**
  - [x] Implement metric-specific budgets in `perfgate.toml` (overriding globals).
  - [x] Support regex-based bench selection in `check --all`.

---

## Milestone 3: v1.2 Ecosystem & CI Integration
**Target: Q3 2026**
*Focus: Seamless integration into the developer's workflow.*

- [x] **Native GitHub Actions Integration**
  - [x] Official `perfgate-action` for zero-config setup.
  - [x] Support setting Action outputs (verdict, counts) for workflow branching.
- [x] **Flexible Reporting**
  - [x] Support Liquid/Handlebars templates for Markdown comments.
  - [x] Multi-format export (e.g., HTML summary, Prometheus/OpenTelemetry push).
- [x] **Baseline Management**
  - [x] Auto-discovery of baselines using naming patterns.
  - [x] S3/GCS backend support for `promote` and `check`.

---

## Milestone 4: v2.0 Fleet-Scale Performance
**Target: 2027+**
*Focus: Advanced analytics and long-term trend management.*

- [x] **Baseline Service API**
  - [x] Optional client/server mode to fetch baselines from a central service.
  - [x] Centralized authentication and permission model for promoting baselines.
  - [x] REST API with SQLite/in-memory storage backends.
  - [x] CLI integration with `--baseline-server`, `--upload`, `--to-server` flags.
  - [x] `perfgate baseline` subcommand for baseline management.
  - [x] Role-based access control (viewer, contributor, promoter, admin).
- [x] **Advanced Analytics**
  - [x] Statistical significance testing (p-values) for large sample sizes.
  - [x] Multi-dimensional gating (e.g., gate on `P95` wall time + `median` RSS).
- [ ] **Schema Evolution**
  - [ ] Transition to `v2` schemas if breaking changes are required for advanced features.
