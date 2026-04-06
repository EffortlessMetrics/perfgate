# perfgate Roadmap

This document outlines the planned evolution of perfgate. v0.15.0 is the first published release. Future work is grounded in what exists in the codebase today.

## Near-Term (0.16.x)

Theme: make the baseline service boring, trustworthy, and well-documented.

### Storage Hardening
- [ ] **PostgreSQL connection pooling** ([#65]): Pool tuning, retry logic, and health checks under load.
- [ ] **S3 lifecycle policies** ([#67]): Retention and cleanup for old receipts in object storage.
- [ ] **SQLite WAL mode** ([#73]): Enable WAL for concurrent read performance.

### Authentication & Authorization
- [ ] **OIDC stabilization** ([#72]): Test with GitLab CI and custom providers beyond GitHub Actions.
- [ ] **API key management CLI** ([#71]): Commands for creating, listing, revoking, and rotating keys.

### Platform Parity
- [ ] **Windows metric gaps** ([#70]): `page_faults` and `ctx_switches` are not yet collected on Windows (only `cpu_ms` and `max_rss_kb`).
- [x] **Timeout support on Windows** ([#69]): Implemented via `try_wait()` polling loop with `child.kill()` on expiration.

### Quality
- [ ] **Server integration tests** ([#76]): Automate `#[ignore]` server tests in CI.
- [ ] **CLI doc example validation** ([#83]): Validate doc examples against the actual binary.

## Medium-Term (0.17.x)

Theme: trust the signal before widening the platform surface.

### Observability & Audit
- [ ] **Audit logging** ([#68]): Audit trail for baseline promotions, deletions, and key changes.
- [ ] **Prometheus endpoint** ([#66]): `/metrics` scrape endpoint on the server.

### Noise & Stability
- [ ] **Noise policy tuning** ([#78]): Smarter paired retry logic with adaptive sample sizes.
- [ ] **Flakiness tracking** ([#79]): Cross-run flakiness history and scoring.
- [ ] **Weighted fleet aggregation** ([#81]): Account for runner variance in `perfgate aggregate`.

### Dashboard
- [ ] **Dashboard enhancement** ([#77]): Filtering, drill-down, export, and responsive layout.

### Documentation & Ecosystem
- [ ] **CI guides** ([#74]): Bitbucket Pipelines and CircleCI integration guides.
- [ ] **Schema evolution** ([#75]): Documented policy for v2 schema coexistence.
- [ ] **Crate READMEs** ([#59]): Expand thin READMEs for api, config, selfbench, summary.
- [x] **Baseline server docs** ([#51], [#52]): Validate and trim server documentation.

## Long-Term (Toward 1.0)

- [ ] **API and schema freeze**: Stabilize all public JSON contracts and REST endpoints before 1.0.
- [ ] **Cross-project federation**: Build on the shipped compare-time `--baseline-project` lookup without weakening project isolation; reserve server-side multi-project compare/query work for an explicit API and auth design.
- [ ] **Pluggable renderers** ([#82]): Generalize template support into a plugin system after the core gate and service contracts are stable.

---

[#51]: https://github.com/EffortlessMetrics/perfgate/issues/51
[#52]: https://github.com/EffortlessMetrics/perfgate/issues/52
[#59]: https://github.com/EffortlessMetrics/perfgate/issues/59
[#65]: https://github.com/EffortlessMetrics/perfgate/issues/65
[#66]: https://github.com/EffortlessMetrics/perfgate/issues/66
[#67]: https://github.com/EffortlessMetrics/perfgate/issues/67
[#68]: https://github.com/EffortlessMetrics/perfgate/issues/68
[#69]: https://github.com/EffortlessMetrics/perfgate/issues/69
[#70]: https://github.com/EffortlessMetrics/perfgate/issues/70
[#71]: https://github.com/EffortlessMetrics/perfgate/issues/71
[#72]: https://github.com/EffortlessMetrics/perfgate/issues/72
[#73]: https://github.com/EffortlessMetrics/perfgate/issues/73
[#74]: https://github.com/EffortlessMetrics/perfgate/issues/74
[#75]: https://github.com/EffortlessMetrics/perfgate/issues/75
[#76]: https://github.com/EffortlessMetrics/perfgate/issues/76
[#77]: https://github.com/EffortlessMetrics/perfgate/issues/77
[#78]: https://github.com/EffortlessMetrics/perfgate/issues/78
[#79]: https://github.com/EffortlessMetrics/perfgate/issues/79
[#81]: https://github.com/EffortlessMetrics/perfgate/issues/81
[#82]: https://github.com/EffortlessMetrics/perfgate/issues/82
[#83]: https://github.com/EffortlessMetrics/perfgate/issues/83

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
