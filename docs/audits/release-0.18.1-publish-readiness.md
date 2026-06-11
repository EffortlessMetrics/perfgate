# v0.18.1 Publish Readiness Proof

Date: 2026-06-11

Branch: `chore/0.18.1-release-prep` (canonical publish-prep branch)
Version under test: `0.18.1`

Purpose: validate that the v0.18.1 release candidate packages and verifies for
the five public crates before any irreversible publication step. This proof does
not publish crates, create tags, create a GitHub release, or move action aliases.

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |
| Publication state | Dry-run only; no crates were uploaded |

## Publish Dry-Run Matrix

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 run -p xtask -- publish-check --package-list` | Pass | Static packaging checks passed and listed the five publishable crates. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types` | Pass | Packaged and verified `perfgate-types v0.18.1` in dry-run mode. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate` | Pass | Packaged and verified `perfgate v0.18.1` in dry-run mode. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client` | Pass | Packaged and verified `perfgate-client v0.18.1` in dry-run mode. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server` | Pass | Packaged and verified `perfgate-server v0.18.1` in dry-run mode. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli` | Pass | Packaged and verified `perfgate-cli v0.18.1` in dry-run mode. |

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
- No `v0.18.1` tag was created.
- No `v0.18` or `v0` action alias movement was attempted.
- No GitHub release was created.
- Public install smoke remains blocked until public artifacts exist.

## Known Deferred Proof

- No-panic policy check remains a controlled drift (`213`) and is carried forward
  as an explicit advisory/deferred condition in lane planning.

