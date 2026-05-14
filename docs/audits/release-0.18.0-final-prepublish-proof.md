# v0.18.0 Final Pre-Publish Proof

Date: 2026-05-14

Branch: `release/0-18-final-prepublish-proof`

Commit: `3eef1b0371cf049b8b124c95cc14f7bba91382b1`

Purpose: refresh the full pre-publish proof from current `main` after the
premature deferral closeout was superseded. This proof does not publish crates,
create tags, create a GitHub release, move action aliases, or prove public
install from public artifacts.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Version under test | `0.18.0` |
| Target dir for heavy Cargo proof | `C:\perfgate-target-final-prepublish` |
| Cargo incremental | disabled with `CARGO_INCREMENTAL=0` |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |
| Publication state | Pre-publish only; no crates were uploaded |

The first full-workspace test attempt used the default target directory on
`D:` and failed because the local drive was full (`os error 112`). The proof was
rerun with `CARGO_TARGET_DIR=C:\perfgate-target-final-prepublish` and passed.
That default-target failure was an environment capacity issue, not a product
test failure.

## Command Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 fmt --all -- --check` | Pass | Formatting check completed without changes. |
| `cargo +1.95.0 check --workspace --all-targets --all-features --locked` | Pass | Workspace check completed under the pre-publish target directory. |
| `cargo +1.95.0 test --workspace --all-targets --all-features --locked` | Pass | Full workspace test suite passed under the pre-publish target directory. |
| `cargo +1.95.0 run -p xtask -- docs-check` | Pass | Documentation drift check passed. |
| `cargo +1.95.0 run -p xtask -- doc-test` | Pass | Checked 70 CLI examples and 36 structured snippets. |
| `cargo +1.95.0 run -p xtask -- docs-source-check` | Pass | Source-of-truth metadata, IDs, links, and active goal are valid. |
| `cargo +1.95.0 run -p xtask -- product-claims-check` | Pass | Product claim proof map is valid. |
| `cargo +1.95.0 run -p xtask -- public-surface --strict` | Pass | Public-surface policy accounts for the five publishable packages. |
| `cargo +1.95.0 run -p xtask -- arch` | Pass | Architecture dependency rules hold. |
| `cargo +1.95.0 run -p xtask -- action-check` | Pass | GitHub Action install, release asset, and failure diagnostic wiring are aligned. |
| `cargo +1.95.0 run -p xtask -- schema-compat` | Pass | 18 historical schema fixtures deserialize with current types. |
| `cargo +1.95.0 run -p xtask -- publish-check --package-list` | Pass | Package list resolves to the five public crates. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types` | Pass | Packaged 21 files, verified `perfgate-types v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate` | Pass | Packaged 87 files, verified `perfgate v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client` | Pass | Packaged 12 files, verified `perfgate-client v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server` | Pass | Packaged 40 files, verified `perfgate-server v0.18.0`, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli` | Pass | Packaged 74 files, verified `perfgate-cli v0.18.0`, and aborted upload because of dry run. |
| `git diff --check` | Pass | Checked after this audit and release-state updates were added. |

## Publish Order

If release-operator approval is granted, publish in this order:

```text
perfgate-types
perfgate
perfgate-client
perfgate-server
perfgate-cli
```

## Non-Inferences

- This does not publish `0.18.0` to crates.io.
- This does not create `v0.18.0`.
- This does not create a GitHub release or release assets.
- This does not move `v0.18` or `v0`.
- This does not prove public install from crates.io, `cargo-binstall`, or
  GitHub release assets.
- This does not close the release cutover lane.

## Release Boundary

The next irreversible release step remains blocked on explicit release-operator
approval. The active lane is now waiting at release-operator-gated publication:
crates.io upload, exact release tag, GitHub release/assets, intentional action
alias movement, public install smoke, and publication closeout.
