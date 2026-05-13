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

For the 0.16 public-surface collapse, only these packages should be
publishable. This is now enforced by `cargo run -p xtask -- public-surface --strict`:

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
| `perfgate-auth` | `perfgate_types::baseline_service::auth` | crate deleted |
| `perfgate-summary` | `perfgate::presentation::render::summary` | crate deleted |
| `perfgate-stats` | `perfgate::domain::stats` | crate deleted |
| `perfgate-paired` | `perfgate::domain::paired` | crate deleted |
| `perfgate-error` | `perfgate_types::error` | crate deleted |
| `perfgate-render` | `perfgate::presentation::render` | crate deleted |
| `perfgate-export` | `perfgate::presentation::export` | crate deleted |
| `perfgate-sensor` | `perfgate::presentation::sensor` | crate deleted |
| `perfgate-fake` | private workspace crate | marked `publish = false` |
| `perfgate-api` | `perfgate_types::baseline_service`; runtime credential source in `perfgate_server::CredentialSource` | crate deleted |
| `perfgate-profile` | `perfgate::runtime::profile` | crate deleted |
| `perfgate-ingest` | `perfgate::integrations::ingest` | crate deleted |
| `perfgate-significance` | `perfgate::domain::significance` | crate deleted |
| `perfgate-sha256` | `perfgate_types::fingerprint`; facade path `perfgate::core::fingerprint` | crate deleted |
| `perfgate-host-detect` | `perfgate::domain::host` | crate deleted |
| `perfgate-budget` | `perfgate::domain::budget`; facade path `perfgate::core::budget` | crate deleted |
| `perfgate-scaling` | `perfgate::domain::scaling` | crate deleted |
| `perfgate-github` | `perfgate::integrations::github` | crate deleted |
| `perfgate-adapters` | `perfgate::runtime` | crate deleted |
| `perfgate-domain` | `perfgate::domain` | crate deleted |
| `perfgate-app` | `perfgate::app` | crate deleted |

Those paths are intentionally more conservative than the final facade shape.
Future PRs may re-export or move pieces again, but they must do so with the
policy files and docs updated in the same change.

## Remaining Private Surface

`policy/absorbed_crates.txt` is the machine-readable disposition list. The
public-surface blockers have been resolved; remaining non-public workspace
packages are private/dev packages or compatibility wrappers:

| Package | Disposition |
|---------|-------------|
| `perfgate-fake` | private workspace crate, `publish = false` |
| `perfgate-selfbench` | private workspace crate |
| `perfgate-tests` | private workspace root package |
| `xtask` | private workspace automation crate |

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

The current enforcement uses Cargo metadata layer rules plus module-level
source scans for collapsed domain and presentation seams.

## Enforcement

Run:

```bash
cargo run -p xtask -- arch
cargo run -p xtask -- public-surface
```

`xtask arch` fails when lower-level packages can reach forbidden higher-level
packages through non-dev workspace dependencies, or when pure domain and
presentation modules import forbidden filesystem/process APIs in production
source.

`xtask public-surface` fails if a publishable workspace package is neither listed in
`policy/public_crates.txt` nor assigned a disposition in
`policy/absorbed_crates.txt`. Entries marked `[compatibility wrapper]` must also
stay out of non-dev workspace dependency graphs; internal crates should import
the owner path directly and leave the wrapper for external compatibility.

Run:

```bash
cargo run -p xtask -- public-surface --strict
```

Strict mode is the release-end gate. It fails if any absorbed package is still
publishable, or if a target public package directly depends on an
absorbed/internal workspace package. It now passes on `main`.

## Completed Migration Order

1. Land first seam absorption and compatibility wrappers.
2. Establish enforceable public-surface policy.
3. Collapse contract-adjacent crates.
4. Collapse pure core and domain policy crates.
5. Collapse presentation, runtime, app, and integration crates.
6. Convert remaining published packages into deprecated shims or mark them
   private.
7. Extend `xtask arch` for any new source/module dependency rules as crates collapse.
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
- [plans/0.18.0/wrapper-crate-cleanup.md](../plans/0.18.0/wrapper-crate-cleanup.md)
- [policy/public_crates.txt](../policy/public_crates.txt)
- [policy/absorbed_crates.txt](../policy/absorbed_crates.txt)
