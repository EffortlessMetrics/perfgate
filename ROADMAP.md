# perfgate Roadmap

This document outlines the planned evolution of `perfgate`. It serves as a high-level guide for contributors and users to understand the project's direction.

## Vision
To become the standard, low-friction "build truth" sensor for performance gating in modern CI/CD pipelines, providing stable, versioned, and actionable performance data.

---

## Current Status (v0.2.x)
- [x] Core CLI commands: `run`, `compare`, `report`, `check`, `paired`, `promote`, `export`.
- [x] Modular workspace architecture.
- [x] Stable v1 JSON receipt schemas.
- [x] Cockpit integration mode (`--mode cockpit`).
- [x] Multi-layered testing: BDD, Property, Fuzz, Mutation.
- [x] Basic Windows support; Full Unix support (RSS, CPU time).

---

## Milestone 1: v1.0 Core Stabilization
**Target: Q1 2026**
*Focus: Reliability, documentation completeness, and initial ecosystem hardening.*

- [ ] **Contract Hardening**
  - [ ] Finalize and lock `v1` schemas for receipts and config.
  - [ ] Ensure byte-for-byte deterministic output for all report generators.
  - [ ] Detect and flag extra/stale files in contract mirror checks.
- [ ] **Documentation & DX**
  - [ ] Complete documentation alignment (metrics names, CLI flags).
  - [ ] Provide canonical "Getting Started" guides for common CI providers (GitHub Actions, GitLab CI).
  - [ ] Standardize error stage/kind classifications across all failure paths.
- [ ] **Tooling**
  - [ ] Deduplicate `xtask` and test helpers into shared library modules.
  - [ ] Stabilize `conform` command for third-party sensor validation.

---

## Milestone 2: v1.1 Extended Observability
**Target: Q2 2026**
*Focus: Deeper system metrics and improved platform parity.*

- [ ] **New Metrics (Unix)**
  - [ ] `page_faults`: Track major page faults to detect memory pressure changes.
  - [ ] `ctx_switches`: Track voluntary and involuntary context switches.
- [ ] **Static Analysis Metrics**
  - [ ] `binary_bytes`: Track changes in compiled executable size.
- [ ] **Improved Platform Support**
  - [ ] Explore best-effort CPU time and memory tracking for Windows.
  - [ ] Support custom environment injection for `paired` mode executions.
- [ ] **Configuration Enhancements**
  - [ ] Implement metric-specific budgets in `perfgate.toml` (overriding globals).
  - [ ] Support regex-based bench selection in `check --all`.

---

## Milestone 3: v1.2 Ecosystem & CI Integration
**Target: Q3 2026**
*Focus: Seamless integration into the developer's workflow.*

- [ ] **Native GitHub Actions Integration**
  - [ ] Official `perfgate-action` for zero-config setup.
  - [ ] Support setting Action outputs (verdict, counts) for workflow branching.
- [ ] **Flexible Reporting**
  - [ ] Support Liquid/Handlebars templates for Markdown comments.
  - [ ] Multi-format export (e.g., HTML summary, Prometheus/OpenTelemetry push).
- [ ] **Baseline Management**
  - [ ] Auto-discovery of baselines using naming patterns.
  - [ ] S3/GCS backend support for `promote` and `check`.

---

## Milestone 4: v2.0 Fleet-Scale Performance
**Target: 2027+**
*Focus: Advanced analytics and long-term trend management.*

- [ ] **Baseline Service API**
  - [ ] Optional client/server mode to fetch baselines from a central service.
  - [ ] Centralized authentication and permission model for promoting baselines.
- [ ] **Advanced Analytics**
  - [ ] Statistical significance testing (p-values) for large sample sizes.
  - [ ] Multi-dimensional gating (e.g., gate on `P95` wall time + `median` RSS).
- [ ] **Schema Evolution**
  - [ ] Transition to `v2` schemas if breaking changes are required for advanced features.
