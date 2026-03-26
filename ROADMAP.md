# perfgate Roadmap

This document outlines the planned evolution of `perfgate`. We are currently in the **Enterprise & Ecosystem Hardening phase**, focusing on stabilizing the platform for large-scale production use before reaching 1.0.

## Strategic Outlook (2026-2027)

### Now: Enterprise Hardening (0.16.x - 0.17.x)
- **Persistence**: Hardening PostgreSQL and S3 backends for high-availability.
- **Identity**: Multi-provider OIDC (GitHub, GitLab, Okta) and fine-grained RBAC.
- **Scale**: Optimizing server performance for fleets with 1000+ active benchmarks.

### Next: Ecosystem Expansion (0.18.x - 0.19.x)
- **CI Plugins**: Native integrations for Bitbucket Pipelines, CircleCI, and Jenkins.
- **Formatters**: Supporting industry-standard performance interchange formats.
- **API**: Stabilizing the `perfgate-client` API for third-party sensor development.

### Later: Performance Intelligence (0.20.x+)
- **Predictive Analytics**: Forecasting regressions based on trend analysis.
- **Health Scoring**: Automated "Performance Health" grades for projects and teams.
- **AI Triage**: Advanced LLM diagnostic loops with automated PR remediation.

---

## The Path to 1.0 (0.16.0 - 0.20.0)

### 0.16.0: Cloud Native Stabilization
- [ ] **High-Availability Store**: Cluster-ready PostgreSQL storage with connection pooling.
- [ ] **S3 Lifecycle Management**: Automated archival and cleanup policies for raw receipts.
- [ ] **Prometheus Exporter**: Direct metrics export from the baseline server.

### 0.17.0: Advanced Authentication
- [ ] **Multi-Provider OIDC**: Support for custom OIDC providers beyond GitHub/GitLab.
- [ ] **Service Account Management**: CLI and API for managing Scoped Service Keys.
- [ ] **Audit Logging**: Tracking all baseline modifications and promotion events.

### 0.18.0: The Formatting Powerhouse
- [ ] **JUnit/XUnit Support**: Native export to JUnit XML for integration with legacy CI reporters.
- [ ] **JSON Schema v2**: Evolving the schema to support complex multi-variate metrics.
- [ ] **Custom Renderers**: Pluggable engine for user-defined Markdown/HTML templates.

### 0.19.0: Fleet Intelligence
- [ ] **Weighted Aggregation**: Smart `aggregate` logic that accounts for runner variance.
- [ ] **Cross-Project Comparisons**: Benchmarking common components across multiple projects.
- [ ] **Regression Trends**: Visualizing the "rate of decay" across multiple releases.

### 0.20.0: Performance Health
- [ ] **Perf Scorecards**: Generating high-level summaries of performance stability.
- [ ] **Auto-Triage**: Automated assignment of regressions based on `git blame`.
- [ ] **Stable 1.0 Candidate**: Final API and Schema freeze for the 1.0.0 release.

---

## Completed Milestones

### v0.15.0: The Intelligent Gater
- [x] **LLM Regression Explainer**: AI-ready diagnostic prompts for PRs.
- [x] **Regression Blame**: Automated mapping of regressions to `Cargo.lock` changes.
- [x] **Automated Bisection**: Combining `git bisect` with `paired` benchmarking.
- [x] **Fleet Aggregation**: Merging results from multiple runners.
- [x] **Rust 2024 Migration**: Full workspace update to Edition 2024 and Rust 1.92.

### v0.6.0 - v0.14.0 Highlights
- [x] **Deep Observability**: IO, Network, and Energy (RAPL) metrics.
- [x] **Noise Detection**: CV-based flakiness detection and auto-skipping.
- [x] **Visual Insights**: Server-side dashboard and metric graphing.
- [x] **Production Storage**: PostgreSQL and S3 backends for the baseline service.
- [x] **OIDC Identity**: Initial GitHub Actions OIDC support.
- [x] **Micro-crate Refactor**: Decoupled architecture with 25+ specialized crates.

### v0.5.0 and Earlier
- [x] **Self-Dogfooding CI**: Triple-lane gating (Smoke, Perf, Nightly).
- [x] **Baseline Server**: REST API for centralized management.
- [x] **Paired Benchmarking**: Noise-resistant interleaved execution.
- [x] **Locked Schemas**: Stable versioned JSON contracts.
