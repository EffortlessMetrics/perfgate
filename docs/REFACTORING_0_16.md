# Public API Refactoring for 0.16.0

## Overview

This document describes the planned refactoring of perfgate's public crate surface. The goal is to collapse the current 26+ microcrates into a cleaner public facade while preserving the strong internal separation of concerns that makes the architecture maintainable.

**Current state**: Public surface is still too broad. PR #223 already absorbed `perfgate-validation`, `perfgate-auth`, `perfgate-summary`, and `perfgate-stats`, and moved the paired implementation into `perfgate-domain` behind a workspace-only `perfgate-paired` compatibility wrapper. The error contract now lives in `perfgate_types::error` behind a workspace-only `perfgate-error` compatibility wrapper, profiling now lives under `perfgate::runtime::profile`, external benchmark ingestion now lives under `perfgate::integrations::ingest`, deterministic fingerprinting now lives in `perfgate_types::fingerprint` with a `perfgate::core::fingerprint` facade path, host mismatch detection now lives in `perfgate_domain::host` with a `perfgate::domain::host` facade path, budget evaluation now lives in `perfgate_domain::budget` with a `perfgate::core::budget` facade path, scaling analysis now lives in `perfgate_domain::scaling` with a `perfgate::domain::scaling` facade path, rendering now has the facade path `perfgate::presentation::render` behind a workspace-only `perfgate-render` compatibility wrapper, export now has the facade path `perfgate::presentation::export` behind a workspace-only `perfgate-export` compatibility wrapper, sensor report generation now has the facade path `perfgate::presentation::sensor` behind a workspace-only `perfgate-sensor` compatibility wrapper, GitHub PR-comment integration now has the feature-gated facade path `perfgate::integrations::github` behind a workspace-only `perfgate-github` compatibility wrapper, and baseline-service API contracts now live in `perfgate_types::baseline_service` behind a workspace-only `perfgate-api` compatibility wrapper. Many remaining internal seams are still publishable crates (`perfgate-domain`, `perfgate-app`, etc.), which makes the package ecosystem confusing for users and couples the public API tightly to internal refactoring decisions.

**Target state**: Five public crates with strongly organized internal modules. Users depend on `perfgate`, `perfgate-types`, `perfgate-cli`, `perfgate-client`, and `perfgate-server` only. The SRP boundaries remain enforced but move from crate level to module level.

---

## Target Public Crate Surface

Only these crates should be published:

| Crate | Purpose | Keep? |
|-------|---------|-------|
| `perfgate-cli` | Installs the `perfgate` binary | Yes |
| `perfgate` | Main embeddable library and facade | Yes |
| `perfgate-types` | Stable receipts, config, schemas, wire contracts | Yes |
| `perfgate-client` | Baseline service client for external automation | Yes |
| `perfgate-server` | Baseline service binary/library | Yes |

Everything else will be absorbed into one of these crates as modules, marked `publish = false`, or deleted.

---

## Absorption Map

### Into `perfgate-types`
These are contract-adjacent and belong with the public receipt/config model:

| Current Crate | New Home | Reason |
|---------------|----------|--------|
| `perfgate-error` | `perfgate_types::error` | Error types are part of the contract; compatibility wrapper remains |
| `perfgate-validation` | `perfgate_types::validation` | Schema validation is contract-adjacent |
| `perfgate-config` | `perfgate_types::config` + `perfgate_client::ResolvedServerConfig` | Config model and file helpers are contract-adjacent; client construction belongs to the client crate |
| `perfgate-api` (shared DTOs) | `perfgate_types::baseline_service` | Wire format for baseline service |

**Current state**: `perfgate-validation` and `perfgate-config` have been deleted, and `perfgate-error` now re-exports `perfgate_types::error` as a workspace-only compatibility wrapper. Config file helpers now live in `perfgate_types::config`, resolved baseline-server client construction lives in `perfgate_client`, shared baseline-service DTOs now live in `perfgate_types::baseline_service`, and server credential-source loading now lives in `perfgate_server::CredentialSource`.

**Why first**: The types crate must be standalone and self-describing. Absorbing these dependencies first unblocks all downstream refactoring.

---

### Into `perfgate::core`
Pure logic with no runtime dependencies:

| Current Crate | New Module | Why |
|---------------|-----------|-----|
| `perfgate-stats` | `perfgate_domain::stats` now; facade re-export later | Statistical descriptors (median, p95, etc.) |
| `perfgate-budget` | `perfgate_domain::budget` now; `perfgate::core::budget` facade path | Budget evaluation and verdict logic |
| `perfgate-significance` | `perfgate_domain::significance` now; `perfgate::core::significance` facade path | Welch's t-test and statistical testing |
| `perfgate-paired` | `perfgate_domain::paired` now; facade re-export later | Paired benchmarking computation |
| `perfgate-sha256` | `perfgate_types::fingerprint` now; `perfgate::core::fingerprint` facade path | Minimal SHA-256 for baseline fingerprints |

These should be feature-gated minimally (or always-on) and have no dependency on runtime, CLI, or server code.

---

### Into `perfgate::domain`
Product policy and comparison semantics (I/O-free):

| Current Crate | New Module | Why |
|---------------|-----------|-----|
| `perfgate-domain` | `perfgate::domain` | Core business logic |
| `perfgate-host-detect` | `perfgate_domain::host` now; `perfgate::domain::host` facade path | Host fingerprinting and mismatch detection |
| `perfgate-scaling` | `perfgate_domain::scaling` now; `perfgate::domain::scaling` facade path | Autoscaling policy and trend analysis |

These remain I/O-free but depend on `core::*` modules and define product verdicts.

---

### Into `perfgate::presentation`
Output surfaces (optional features):

| Current Crate | New Module | Feature Gate |
|---------------|-----------|--------------|
| `perfgate-render` | `perfgate::presentation::render` | `render` |
| `perfgate-export` | `perfgate::presentation::export` | `export` |
| `perfgate-sensor` | `perfgate::presentation::sensor` | `sensor` |
| `perfgate-summary` | `perfgate::presentation::summary` | optional |

These should be behind feature flags to keep the default build lightweight.

---

### Into `perfgate::runtime`
I/O and external interactions:

| Current Crate | New Module | Why |
|---------------|-----------|-----|
| `perfgate-adapters` | `perfgate_app::runtime`; facade `perfgate::runtime` | absorbed; wrapper remains |
| `perfgate-profile` | `perfgate::runtime::profile` | absorbed |

Keep ports/interfaces separate from stdlib implementations:
```
perfgate::runtime::ports::ProcessRunner
perfgate::runtime::ports::HostProbe
perfgate::runtime::std::StdProcessRunner
perfgate::runtime::std::SystemHostProbe
```

---

### Into `perfgate::app`
Application orchestration:

| Current Crate | New Module | Why |
|---------------|-----------|-----|
| `perfgate-app` | `perfgate::app` | CLI/server command orchestration |

This stays at a high level and coordinates lower modules.

---

### Into `perfgate::integrations`
Feature-gated external integrations:

| Current Crate | New Module | Status |
|---------------|-----------|--------|
| `perfgate-github` | `perfgate::integrations::github` | feature-gated compatibility wrapper |
| `perfgate-ingest` | `perfgate::integrations::ingest` | absorbed |

These are not core to the product and should stay isolated from core paths.

---

### Dev-Only / Internal (Unpublished)

These remain in the workspace but with `publish = false`:

| Current Crate | New Status |
|---------------|-----------|
| `perfgate-fake` | Module `perfgate::test_support` (feature-gated) or private workspace crate |
| `perfgate-selfbench` | Private workspace crate, `publish = false` |
| `perfgate-tests` | Already `publish = false` |
| `xtask` | Already workspace-only |

---

## Proposed Internal Module Tree

```
crates/
  perfgate/
    src/
      lib.rs
      prelude.rs
      core/
        mod.rs
        stats.rs
        budget.rs
        significance.rs
        paired.rs
        fingerprint.rs
      domain/
        mod.rs
        verdict.rs
        host.rs
        scaling.rs
        blame.rs
        bisect.rs
        explain.rs
      runtime/
        mod.rs
        ports.rs
        process.rs
        host.rs
        fs.rs
        clock.rs
        profile.rs
      app/
        mod.rs
        run.rs
        compare.rs
        check.rs
        promote.rs
        report.rs
        aggregate.rs
      presentation/
        mod.rs
        render.rs
        export.rs
        sensor.rs
        summary.rs
      integrations/
        mod.rs
        github.rs
        ingest.rs
      test_support/
        mod.rs
      error.rs
      prelude.rs
  perfgate-types/
    src/
      lib.rs
      receipt/
      config/
      schema/
      validation/
      error/
      baseline_service/
  perfgate-client/
  perfgate-server/
  perfgate-cli/
```

---

## Feature Gates

```toml
[package]
name = "perfgate"

[features]
default = ["core", "runtime"]
core = []                    # Always needed: stats, budget, significance, paired
runtime = []                 # Always needed: adapters, process, host, clock
domain = []                  # Always needed: domain, host detection, scaling
app = []                     # Always needed: app orchestration
render = []                  # Optional: markdown/terminal rendering
export = ["render"]          # Optional: CSV, JSONL, HTML, Prometheus export
html = ["export"]            # Optional: HTML rendering dependencies
sensor = []                  # Optional: cockpit mode and sensor reports
github = []                  # Optional: GitHub annotations and links
ingest = []                  # Optional: data ingestion adapters
test-support = []            # Dev-only: fake data generators
all = ["core", "runtime", "domain", "app", "render", "export", "html", "sensor", "github", "ingest"]
```

---

## Critical Packaging Rule

**This cannot be solved only with `publish = false`.** If `perfgate` (a public crate) depends on unpublished internal path crates, it creates packaging conflicts when publishing.

**The real move**:
1. Move code into modules inside public crates
2. Remove the dependency from the public crate to the old microcrate
3. Mark the old crate `publish = false` or delete it

For already-published crates that must preserve a public import path for one
transition release, provide a deprecation shim or compatibility wrapper:
```rust
// crates/perfgate-paired/src/lib.rs
pub use perfgate_domain::paired::*;
```

This allows a transition release before removing the crate from future
messaging. Crates that were internal-only may be deleted directly, as PR #223
did for validation, auth, summary, and stats.

---

## Dependency Layer Rules

Once modules replace crates, enforce these rules via `xtask arch`:

```
types must not use perfgate::runtime/app/presentation/integrations
core must not use runtime/app/cli/server/client/presentation/integrations
domain must not use runtime/cli/server/client/integrations
presentation must not use runtime/app/cli/server/client/integrations
runtime must not use cli/server/client
app must not use cli/server/client
integrations may use any module except cli/server/client
cli may use anything public
server may use anything public
client may use anything public
```

Implement this as a Cargo metadata or source-level check in xtask.

---

## Migration Sequence

### Phase 1: Policy & Visibility (current PR)
Create:
- `policy/public_crates.txt` - List of intended public crates
- `policy/absorbed_crates.txt` - Mapping of absorbed crates
- `docs/CRATE_SEAMS.md` - Detailed seam analysis
- `xtask public-surface` check - Fails if a publishable package has no public or absorbed disposition

The default check enforces complete disposition coverage during the transition.
`cargo run -p xtask -- public-surface --strict` is the final release gate that
fails until only the five target public crates are publishable.

### Phase 2: Collapse Contracts (PR 2)
Move into `perfgate-types`:
- `perfgate-error` -> `perfgate_types::error`
- `perfgate-validation` -> `perfgate_types::validation`
- `perfgate-config` -> `perfgate_types::config` and `perfgate_client::ResolvedServerConfig`
- `perfgate-api` shared DTOs -> already landed in `perfgate_types::baseline_service`; server credential-source loading moved to `perfgate_server::CredentialSource`

Update all downstream crate imports. Verify `perfgate-types` remains self-contained.

### Phase 3: Collapse Core Logic (PR 3)
Move into `perfgate::core`:
- `perfgate-stats` -> already landed in `perfgate_domain::stats`; add facade path later
- `perfgate-budget` -> already landed in `perfgate_domain::budget`; facade path is `perfgate::core::budget`
- `perfgate-significance` -> already landed in `perfgate_domain::significance`; facade path is `perfgate::core::significance`
- `perfgate-paired` -> implementation already landed in `perfgate_domain::paired`; add facade path later
- `perfgate-sha256` -> `perfgate_types::fingerprint` now; `perfgate::core::fingerprint` facade path

Provide deprecation shims if crates are published. Update `perfgate` prelude.

### Phase 4: Collapse Domain (PR 4)
Move into `perfgate::domain`:
- `perfgate-domain` -> `perfgate::domain`
- `perfgate-host-detect` -> already landed in `perfgate_domain::host`; facade path is `perfgate::domain::host`
- `perfgate-scaling` -> already landed in `perfgate_domain::scaling`; facade path is `perfgate::domain::scaling`

Verify domain remains I/O-free.

### Phase 5: Collapse Presentation (PR 5)
Move into `perfgate::presentation`:
- `perfgate-render` -> `perfgate::presentation::render` (feature-gated)
- `perfgate-export` -> `perfgate::presentation::export` (feature-gated)
- `perfgate-sensor` -> `perfgate::presentation::sensor` (feature-gated)
- `perfgate-summary` -> `perfgate::presentation::summary`

Add feature gates and verify default build is lightweight.

### Phase 6: Collapse Runtime & App (PR 6)
Move into `perfgate`:
- `perfgate-adapters` -> `perfgate_app::runtime` with facade path `perfgate::runtime` (done)
- `perfgate-profile` -> `perfgate::runtime::profile` (done)
- `perfgate-app` -> `perfgate::app`
- `perfgate-github` -> `perfgate::integrations::github` (feature-gated, wrapper remains)
- `perfgate-ingest` -> `perfgate::integrations::ingest` (done)

Simplify `perfgate-cli` dependencies to mostly: `perfgate`, `perfgate-types`, `perfgate-client`, `perfgate-server`.

### Phase 7: Cleanup & Documentation (PR 7)
For every absorbed crate:
- Delete if not published and not needed
- Convert to deprecated shim if published
- Mark `publish = false` if it remains workspace-only

Update documentation:
- `README.md` - Remove 26-crate architecture mention
- `docs/ARCHITECTURE.md` - Describe public packages + internal modules
- `CLAUDE.md` - Update developer guidance
- Crate READMEs - Point users to main `perfgate` crate

Add `xtask arch` check for module-layer violations.

### Phase 8: First Release (0.16.0)
- Publish with new surface
- Deprecation shims live for one release
- Document migration path in `CHANGELOG.md`

### Phase 9: Second Release (0.17.0 or later)
- Remove deprecation shims
- Finalize cleanup

---

## Definition of Done

The refactoring is complete when:

1. `policy/public_crates.txt` lists exactly: `perfgate`, `perfgate-cli`, `perfgate-types`, `perfgate-client`, `perfgate-server`
2. `cargo publish --dry-run -p perfgate` does not require publishing internal microcrates
3. `perfgate-cli` depends on public packages, not every internal seam crate
4. All microcrates are either deleted, `publish = false`, or deprecated shims
5. `docs/ARCHITECTURE.md` describes public packages + internal modules
6. `xtask arch` prevents module-layer violations
7. Users are taught to use:
   ```rust
   use perfgate::prelude::*;
   use perfgate_types::*;
   use perfgate_client::*;
   ```
   Not:
   ```rust
   use perfgate_budget::*;
   use perfgate_stats::*;
   use perfgate_render::*;
   ```
8. Examples and tutorials updated
9. All tests pass; mutation test targets maintained
10. CHANGELOG documents migration path for 0.16.0

---

## Key Principles

- **One facade crate**: Users import `perfgate`, not 20 different microcrates.
- **SRP preserved at module level**: Same boundaries, but inside `perfgate` as modules.
- **No breaking changes in 0.16.0**: Deprecation shims allow gradual migration.
- **Feature-gated optional surfaces**: Render, export, sensor, github, ingest are all optional.
- **Types always self-contained**: `perfgate-types` never depends on other perfgate crates.
- **Domain stays I/O-free**: `perfgate::domain` has no runtime dependencies.
- **Strict module-layer checks**: Enforce via CI to prevent architectural decay.

---

## Timeline

Target completion: **0.16.0 release** (Q3 2026 or later).

Phases 1-7 can proceed in parallel with other 0.16.0 work, but PRs should be reviewed in order to avoid merge conflicts.

---

## References

- [docs/ARCHITECTURE.md](ARCHITECTURE.md) - Current architecture (will be updated)
- `policy/public_crates.txt` - Policy enforcement
- `policy/absorbed_crates.txt` - Absorption map
- [docs/CRATE_SEAMS.md](CRATE_SEAMS.md) - Detailed seam analysis
