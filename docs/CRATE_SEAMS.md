# Perfgate Crate Seams and Public Surface

This document is the canonical 0.16 crate-surface contract. It keeps the
clean-architecture seams that made the workspace useful, but stops treating
every seam as a public package.

## Governing Rule

> Crates are public contracts. Folders and modules are architectural boundaries.

The target is a small public package surface backed by enforceable internal
seams. The product should be easy to depend on without exposing every internal
refactoring decision as a crates.io package.

## Target Public Crates

By the end of the 0.16 public-surface collapse, only these packages should be
publishable:

| Crate | Role |
|-------|------|
| `perfgate` | Main embeddable facade for local performance-gating workflows |
| `perfgate-cli` | Installs the `perfgate` binary |
| `perfgate-types` | Stable receipts, schemas, config, and wire contracts |
| `perfgate-client` | Baseline service client |
| `perfgate-server` | Baseline service binary/library |

All other workspace packages must have one explicit disposition:

- absorbed into one of the public crates as an internal module,
- deleted from the workspace,
- marked `publish = false`,
- or kept temporarily as a deprecated compatibility shim.

## Current Landed Moves

PR #223 started the real collapse and is the current implementation truth:

| Former crate | Current owner | Status |
|--------------|---------------|--------|
| `perfgate-validation` | `perfgate_types::validation` | crate deleted |
| `perfgate-auth` | `perfgate_api::auth` | crate deleted |
| `perfgate-summary` | `perfgate_render::summary` | crate deleted |
| `perfgate-stats` | `perfgate_domain::stats` | crate deleted |
| `perfgate-paired` | `perfgate_domain::paired` | compatibility wrapper remains |
| `perfgate-error` | `perfgate_types::error` | compatibility wrapper remains |
| `perfgate-fake` | private workspace crate | marked `publish = false` |

Those paths are intentionally more conservative than the final facade shape.
Future PRs may re-export or move pieces again, but they must do so with the
policy files and docs updated in the same change.

## Remaining Absorption Map

`policy/absorbed_crates.txt` is the machine-readable disposition list. The
high-level target is:

| Current package | Target owner |
|-----------------|--------------|
| `perfgate-config` | `perfgate_types::config` |
| `perfgate-api` | `perfgate_types::baseline_service` or shared client/server contract |
| `perfgate-domain` | `perfgate::domain` |
| `perfgate-budget` | `perfgate::core::budget` |
| `perfgate-significance` | `perfgate::core::significance` |
| `perfgate-host-detect` | `perfgate::domain::host` |
| `perfgate-scaling` | `perfgate::domain::scaling` |
| `perfgate-sha256` | `perfgate::core::fingerprint` |
| `perfgate-render` | `perfgate::presentation::render` |
| `perfgate-export` | `perfgate::presentation::export` |
| `perfgate-sensor` | `perfgate::presentation::sensor` |
| `perfgate-adapters` | `perfgate::runtime` |
| `perfgate-profile` | `perfgate::runtime::profile` |
| `perfgate-app` | `perfgate::app` |
| `perfgate-github` | `perfgate::integrations::github` |
| `perfgate-ingest` | `perfgate::integrations::ingest` |
| `perfgate-fake` | private workspace crate |
| `perfgate-selfbench` | private workspace crate |

## Dependency Direction Rules

These rules preserve the SRP architecture after crate collapse:

```text
types must not depend on runtime, app, server, client, or cli
core/domain must not depend on filesystem, process execution, server, client, or cli
presentation must not depend on process execution, server, client, or cli
runtime must not depend on cli, server, or client
client must not depend on server
server must not depend on cli
cli is the outermost adapter
```

The current enforcement starts with Cargo metadata layer rules and source scans
for the pure and presentation crates. As crates collapse into modules, extend
`xtask arch` with module-level source rules.

## Enforcement

Run:

```bash
cargo run -p xtask -- arch
cargo run -p xtask -- public-surface
```

`xtask arch` fails when lower-level packages can reach forbidden higher-level
packages through non-dev workspace dependencies, or when pure crates import
filesystem/process APIs in production source.

`xtask public-surface` fails if a publishable workspace package is neither listed in
`policy/public_crates.txt` nor assigned a disposition in
`policy/absorbed_crates.txt`. Entries marked `[compatibility wrapper]` must also
stay out of non-dev workspace dependency graphs; internal crates should import
the owner path directly and leave the wrapper for external compatibility.

Run:

```bash
cargo run -p xtask -- public-surface --strict
```

Strict mode is the release-end gate. It fails while any absorbed package is
still publishable.

## Migration Order

1. Land first seam absorption and compatibility wrappers.
2. Establish enforceable public-surface policy.
3. Collapse contract-adjacent crates.
4. Collapse pure core and domain policy crates.
5. Collapse presentation, runtime, app, and integration crates.
6. Convert remaining published packages into deprecated shims or mark them
   private.
7. Extend `xtask arch` for source/module dependency rules as crates collapse.
8. Update README, architecture docs, examples, and changelog for 0.16.

## Per-PR Checklist

- Code moved into the target owner module.
- Workspace imports updated.
- Compatibility shim kept only when intentionally preserving a published path.
- Old package removed from `[workspace].members` or marked `publish = false`.
- `policy/absorbed_crates.txt` status updated.
- Docs and examples reference the current owner path.
- `cargo run -p xtask -- arch` passes.
- `cargo run -p xtask -- public-surface` passes.
- `cargo run -p xtask -- ci` passes.

## References

- [REFACTORING_0_16.md](REFACTORING_0_16.md)
- [MIGRATION_0_16.md](MIGRATION_0_16.md)
- [ARCHITECTURE.md](ARCHITECTURE.md)
- [policy/public_crates.txt](../policy/public_crates.txt)
- [policy/absorbed_crates.txt](../policy/absorbed_crates.txt)
