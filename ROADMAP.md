# perfgate Roadmap

This document outlines the planned evolution of `perfgate`. We are currently in the **0.x.y stabilization phase**, focusing on building a world-class performance gating tool before reaching 1.0.

## Strategic Outlook (2026-2027)

### Now: Security & Identity (0.8.x)
- **Auth**: Implementing OIDC and service account support for secure CI integration.
- **RBAC**: Hardening role-based access control for baseline management.

### Next: Visual Insights (0.9.0 - 0.10.0)
- **Dashboard**: Minimal web UI for viewing performance trends.
- **Noise Detection**: Identifying flaky benchmarks and unstable environments.

### Later: Intelligent Gating & Ecosystem (0.11.0 - 0.15.0)
- **Analytics**: p-value bisection, noise detection, and trend analysis.
- **Integrations**: Expanding beyond GitHub into the broader CI/CD ecosystem.

---

## The 10-Release Plan (0.6.0 - 0.15.0)

### 0.6.0: The Learning Loop
- [x] **Weekly Variance Summaries**: `xtask` command to aggregate nightly trend JSONL into a human-readable stability report.
- [x] **Threshold Recommendations**: Automatically suggest `perfgate.toml` threshold updates based on observed 7-day variance.
- [x] **Trend Persistence**: First-class support for storing trend data in local Git or S3 without requiring the full server.

### 0.7.0: Production Storage
- [x] **PostgreSQL Backend**: Implementation of the `BaselineStore` trait for Postgres in `perfgate-server`. [DONE]
- [x] **S3/Object Storage**: Support for storing raw receipts in S3/GCS while keeping metadata in DB. [DONE]
- [x] **Migration Tooling**: CLI helpers to move baselines from local files to the server. [DONE]


### 0.8.0: Security & Identity
- [x] **OIDC Integration**: Support for GitHub/GitLab identity providers in the server. [DONE]
- [x] **Service Accounts**: Scoped API keys for CI runners with "contributor" only permissions. [DONE]
- [x] **RBAC Hardening**: Fine-grained permissions for project isolation and benchmark-level regex scoping. [DONE]

### 0.9.0: Visual Insights
- [x] **Server Web UI**: A minimal Read-only dashboard to view performance trends and baseline history. [DONE]
- [x] **Metric Graphing**: Plotting wall time and RSS trends directly in the server UI. [DONE]
- [x] **Verdict History**: Tracking how often a benchmark fails vs. passes over time. [DONE]

### 0.10.0: Noise & Flakiness Detection
- [x] **Noise Detection**: Automatically identify "unstable" benchmarks with high coefficient of variation. [DONE]
- [x] **Auto-Skipping**: Option to skip/warn instead of fail for benchmarks identified as flaky. [DONE]
- [x] **Retry Logic**: Built-in `paired` retry support when significance is not reached. [DONE]

### 0.11.0: Deep Observability
- [x] **IO & Network Metrics**: Track disk bytes read/written and network packets via `rusage` extensions. [DONE]
- [x] **Power/Energy Metrics**: Experimental support for tracking CPU energy usage (RAPL on Linux). [DONE]
- [x] **Binary Delta Blame**: Map `binary_bytes` changes to specific dependency updates in `Cargo.lock`. [DONE]

### 0.12.0: Ecosystem Expansion
- [x] **GitLab CI Integration**: Official templates and documentation for GitLab environments. [DONE]
- [x] **Jenkins/Generic Plugin**: A stable JSON-to-JUnit or similar adapter for legacy CI systems. [DONE]
- [x] **Liquid Template Hub**: A repository of community-contributed Markdown templates for PR comments. [DONE]

### 0.13.0: Automated Bisection
- [x] **Performance Bisection**: A CLI tool that uses `git bisect` combined with `perfgate paired` to find the exact commit that introduced a regression. [DONE]
- [x] **Regression Blame**: Automatically identifying the likely author of a performance dip. [DONE]

### 0.14.0: Distributed Gating
- [x] **Fleet Aggregation**: Ability to run benchmarks on multiple machines and aggregate results into a single "weighted" verdict. [DONE]
- [x] **Matrix Gating**: Support for gating across a matrix of OS/Arch combinations before merging to main. [DONE]

### 0.15.0: The Intelligent Gater (AI Alpha)
- [ ] **LLM Regression Explainer**: Integration with LLMs to analyze code diffs + performance deltas to provide a "likely cause" explanation in PRs.
- [ ] **Automated Playbooks**: Suggesting specific optimization strategies based on the metric that regressed.

---

## Current Status (v0.7.x)
- [x] **v0.7.0 Production Storage**: PostgreSQL backend, S3 offloading, and migration tooling. [DONE]
- [x] **v0.6.0 The Learning Loop**: Automated variance summaries, threshold recommendations, and git-based trend persistence. [DONE]
- [x] **v0.5.0 Self-Dogfooding**: Continuous performance learning loops, multi-lane CI gating, and bot-driven baseline refreshes. [DONE]
- [x] **v0.4.0 Baseline Server**: Centralized baseline management with RBAC and REST API. [DONE]
- [x] **Micro-crate Architecture**: Full decomposition into 25 specialized crates. [DONE]
- [x] **Contract Hardening**: Locked `v1` schemas and deterministic reporting. [DONE]

