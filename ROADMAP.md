# perfgate Roadmap

This document outlines the planned evolution of perfgate. v0.15.0 is the first published release. Future work is grounded in what exists in the codebase today.

## Near-Term (0.16.x)

### Storage Hardening
- [ ] **PostgreSQL connection pooling**: The PostgreSQL backend (`perfgate-server/src/storage/postgres.rs`) works but needs connection pool tuning, retry logic, and health checks under load.
- [ ] **S3 lifecycle policies**: The `object_store` integration supports S3/GCS/Azure uploads, but there is no retention or cleanup policy for old receipts.
- [ ] **SQLite WAL mode**: Enable WAL for the SQLite backend to improve concurrent read performance.

### Authentication & Authorization
- [ ] **OIDC stabilization**: GitHub Actions OIDC works (`perfgate-auth`), but needs testing with GitLab CI and custom providers.
- [ ] **API key management CLI**: Auth types and role-based scoping exist, but there is no CLI for creating, listing, or revoking keys.

### Platform Parity
- [ ] **Windows metric gaps**: `page_faults` and `ctx_switches` are not yet collected on Windows (only `cpu_ms` and `max_rss_kb`).
- [x] **Timeout support on Windows**: Implemented via `try_wait()` polling loop with `child.kill()` on expiration.

## Medium-Term (0.17.x)

### Observability & Audit
- [ ] **Audit logging**: Verdict history exists in the server, but there is no audit trail for baseline promotions, deletions, or key changes.
- [ ] **Prometheus endpoint**: Export format exists (`perfgate-export`), but the server does not expose a `/metrics` scrape endpoint.

### Noise & Stability
- [ ] **Noise policy tuning**: `NoisePolicy` (ignore/warn/skip) exists but paired mode retries could be smarter about when to give up.
- [ ] **Flakiness tracking**: CV-based detection exists per-run, but there is no cross-run flakiness history.

### Documentation & Ecosystem
- [ ] **CI plugin guides**: GitHub Actions guide and GitLab CI guide exist in `docs/`, but Bitbucket and CircleCI are undocumented.
- [ ] **Schema evolution strategy**: Schemas are versioned (v1), but there is no documented policy for how v2 schemas would coexist.

## Long-Term (Toward 1.0)

- [ ] **API and schema freeze**: Stabilize all public JSON contracts and REST endpoints before 1.0.
- [ ] **Pluggable renderers**: Handlebars template support exists for Markdown; generalize to a plugin system for custom output formats.
- [ ] **Cross-project comparisons**: The server namespaces by project, but there is no way to compare benchmarks across projects.
- [ ] **Weighted fleet aggregation**: `perfgate aggregate` merges receipts, but does not yet account for runner variance or weighting.

---

## Shipped in v0.15.0 (First Release)

Everything below shipped in v0.15.0, the first published release. Development milestones prior to this (v0.1.0 through v0.5.0) were internal iterations tracked in [CHANGELOG.md](CHANGELOG.md).

### Intelligent Gating
- [x] **LLM Regression Explainer**: AI-ready diagnostic prompts for PRs (`perfgate explain`).
- [x] **Regression Blame**: Automated mapping of regressions to `Cargo.lock` dependency changes (`perfgate blame`).
- [x] **Automated Bisection**: `git bisect` combined with `paired` benchmarking (`perfgate bisect`).
- [x] **Fleet Aggregation**: Merging results from multiple runners into weighted verdicts (`perfgate aggregate`).

### Core Platform
- [x] **15 CLI commands**: run, compare, md, github-annotations, report, promote, export, check, paired, baseline, summary, aggregate, bisect, blame, explain.
- [x] **Baseline Server**: REST API with SQLite, PostgreSQL, and S3/GCS/Azure storage backends.
- [x] **Paired Benchmarking**: Noise-resistant interleaved execution with significance-based retries.
- [x] **Cockpit Mode**: `sensor.report.v1` output for dashboard integration.
- [x] **Statistical Significance**: Welch's t-test with configurable alpha, confidence intervals, and `--require-significance`.

### Infrastructure
- [x] **26 workspace crates**: Clean-architecture modularization with I/O-free domain core.
- [x] **Versioned schemas**: `perfgate.run.v1`, `perfgate.compare.v1`, `perfgate.report.v1`, `sensor.report.v1`.
- [x] **Multi-format export**: CSV, JSONL, HTML, Prometheus, JUnit.
- [x] **GitHub Actions OIDC**: Token-based authentication for CI runners.
- [x] **Self-dogfooding CI**: Triple-lane gating (Smoke, Perf, Nightly) with automated baseline refreshes.
- [x] **Rust 2024 edition**: Full workspace on Edition 2024 and Rust 1.92.
