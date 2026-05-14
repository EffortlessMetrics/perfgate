# v0.18.0 Publish Readiness Proof

Date: 2026-05-14

Branch: `release/0-18-publish-dry-run`

Purpose: validate that the v0.18.0 release candidate packages and verifies for
the five public crates before any irreversible publication step. This proof
does not publish crates, create tags, create a GitHub release, or move action
aliases.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Version under test | `0.18.0` |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |
| Publication state | Dry-run only; no crates were uploaded |

## Publish Dry-Run Matrix

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 run -p xtask -- publish-check --package-list` | Pass | Static packaging checks passed and listed the five publishable crates. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types` | Pass | Packaged 21 files, verified `perfgate-types v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate` | Pass | Packaged 87 files, verified `perfgate v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client` | Pass | Packaged 12 files, verified `perfgate-client v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server` | Pass | Packaged 40 files, verified `perfgate-server v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli` | Pass | Packaged 74 files, verified `perfgate-cli v0.18.0`, and aborted upload because of dry run. |

## Publish Order

If release-operator approval is granted later, publish in this order:

```text
perfgate-types
perfgate
perfgate-client
perfgate-server
perfgate-cli
```

This proof only validates packaging and verification. It is not approval to run
the publish commands.

## Known Gaps

- Crates were not published.
- No `v0.18.0` tag was created.
- No `v0.18` or `v0` action alias was moved.
- No GitHub release was created.
- Public install smoke remains blocked until public artifacts exist.
- Release archive smoke and public docs cutover remain separate follow-up work.

## Release Boundary

The next irreversible release step remains blocked on explicit release-operator
approval. Until that approval exists, the correct follow-up work is release
artifact smoke and public documentation cutover that continue to distinguish
ready-to-release proof from public release state.
