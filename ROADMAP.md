# perfgate Roadmap

This document outlines the planned evolution of `perfgate`. We are currently in the **0.x.y stabilization phase**, focusing on building a world-class performance gating tool before reaching 1.0.

## Strategic Outlook (2026-2027)

### Now: Foundations & Dogfooding (0.5.x)
- **Hardening**: Stabilizing the 20-crate architecture.
- **Self-Dogfooding**: Strict gating on the `perfgate` repo itself.
- **CLI Polishing**: Landing `summary` and improved error diagnostics.

### Next: The Learning Loop & Enterprise Server (0.6.0 - 0.10.0)
- **Automation**: Moving from "manual gating" to "intelligent observation."
- **Server Depth**: Production-grade storage, auth, and visualization.

### Later: Intelligent Gating & Ecosystem (0.11.0 - 0.15.0)
- **Analytics**: p-value bisection, noise detection, and trend analysis.
- **Integrations**: Expanding beyond GitHub into the broader CI/CD ecosystem.

---

## The 10-Release Plan (0.6.0 - 0.15.0)

### 0.6.0: The Learning Loop
- [x] **Weekly Variance Summaries**: `xtask` command to aggregate nightly trend JSONL into a human-readable stability report.
- [x] **Threshold Recommendations**: Automatically suggest `perfgate.toml` threshold updates based on observed 7-day variance.
- [ ] **Trend Persistence**: First-class support for storing trend data in local Git or S3 without requiring the full server.

### 0.7.0: Production Storage
- [ ] **PostgreSQL Backend**: Implementation of the `BaselineStore` trait for Postgres in `perfgate-server`.
- [ ] **S3/Object Storage**: Support for storing raw receipts in S3/GCS while keeping metadata in DB.
- [ ] **Migration Tooling**: CLI helpers to move baselines from local files to the server.

### 0.8.0: Security & Identity
- [ ] **OIDC Integration**: Support for GitHub/GitLab identity providers in the server.
- [ ] **Service Accounts**: Scoped API keys for CI runners with "contributor" only permissions.
- [ ] **RBAC Hardening**: Fine-grained permissions for promoting baselines vs. just viewing them.

### 0.9.0: Visual Insights
- [ ] **Server Web UI**: A minimal Read-only dashboard to view performance trends and baseline history.
- [ ] **Metric Graphing**: Plotting wall time and RSS trends directly in the server UI.
- [ ] **Verdict History**: Tracking how often a benchmark fails vs. passes over time.

### 0.10.0: Noise & Flakiness Detection
- [ ] **Noise Detection**: Automatically identify "unstable" benchmarks with high coefficient of variation.
- [ ] **Auto-Skipping**: Option to skip/warn instead of fail for benchmarks identified as flaky.
- [ ] **Retry Logic**: Built-in `paired` retry support when significance is not reached.

### 0.11.0: Deep Observability
- [ ] **IO & Network Metrics**: Track disk bytes read/written and network packets via `rusage` extensions.
- [ ] **Power/Energy Metrics**: Experimental support for tracking CPU energy usage (RAPL on Linux).
- [ ] **Binary Delta Blame**: Map `binary_bytes` changes to specific dependency updates in `Cargo.lock`.

### 0.12.0: Ecosystem Expansion
- [ ] **GitLab CI Integration**: Official templates and documentation for GitLab environments.
- [ ] **Jenkins/Generic Plugin**: A stable JSON-to-JUnit or similar adapter for legacy CI systems.
- [ ] **Liquid Template Hub**: A repository of community-contributed Markdown templates for PR comments.

### 0.13.0: Automated Bisection
- [ ] **Performance Bisection**: A CLI tool that uses `git bisect` combined with `perfgate paired` to find the exact commit that introduced a regression.
- [ ] **Regression Blame**: Automatically identifying the likely author of a performance dip.

### 0.14.0: Distributed Gating
- [ ] **Fleet Aggregation**: Ability to run benchmarks on multiple machines and aggregate results into a single "weighted" verdict.
- [ ] **Matrix Gating**: Support for gating across a matrix of OS/Arch combinations before merging to main.

### 0.15.0: The Intelligent Gater (AI Alpha)
- [ ] **LLM Regression Explainer**: Integration with LLMs to analyze code diffs + performance deltas to provide a "likely cause" explanation in PRs.
- [ ] **Automated Playbooks**: Suggesting specific optimization strategies based on the metric that regressed.

---

## Current Status (v0.5.x)
- [x] **v0.5.0 Self-Dogfooding**: Continuous performance learning loops, multi-lane CI gating, and bot-driven baseline refreshes. [DONE]
- [x] **v0.4.0 Baseline Server**: Centralized baseline management with RBAC and REST API. [DONE]
- [x] **Micro-crate Architecture**: Full decomposition into 20 specialized crates. [DONE]
- [x] **Contract Hardening**: Locked `v1` schemas and deterministic reporting. [DONE]

