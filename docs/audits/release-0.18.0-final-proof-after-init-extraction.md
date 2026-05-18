# v0.18.0 Final Proof After Init Extraction

Date: 2026-05-17

Branch: `release/0-18-final-proof-after-init-extraction`

Commit under proof: `d87c1ee1ce4800cdb280341cbc862a1689030178`

Purpose: refresh the 0.18.0 release-candidate proof after #484 extracted the
`perfgate init` execution path into `crates/perfgate-cli/src/init.rs`. The
extraction was behavior-preserving, but this audit makes the final
release-candidate proof apply to the actual current `main` commit after that
change. This proof does not publish crates, create tags, create a GitHub
release, move action aliases, prove public install, or close the active release
cutover lane.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

Linked prior proof:

- [`v0.18.0 Final Pre-Publish Proof`](release-0.18.0-final-prepublish-proof.md)
- [`v0.18.0 Restored Coverage Proof`](release-0.18.0-restored-coverage-proof.md)
- [`v0.18.0 Final Proof After Restored Coverage`](release-0.18.0-final-proof-after-restored-coverage.md)
- [`v0.18.0 Pre-Release Readiness Handoff`](../handoffs/2026-05-17-0-18-pre-release-readiness.md)

## Change Since Prior Final Proof

#484 moved the `perfgate init` command implementation and benchmark-suggestion
helpers out of `crates/perfgate-cli/src/main.rs` and into
`crates/perfgate-cli/src/init.rs`. It preserved clap argument types, command
dispatch, first-use benchmark suggestion behavior, receipt schemas, action
behavior, and release state.

Because #484 landed after the prior final proof and pre-release handoff, this
audit reruns the full release-candidate proof from the post-#484 tree.

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Version under test | `0.18.0` |
| Commit under proof | `d87c1ee1ce4800cdb280341cbc862a1689030178` |
| Target dir | default workspace `target/` |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |
| Publication state | Pre-publish only; no crates were uploaded |

## Command Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 fmt --all -- --check` | Pass | Formatting check completed without changes. |
| `cargo +1.95.0 check --workspace --all-targets --all-features --locked` | Pass | Workspace check completed after #484. |
| `cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings` | Pass | Workspace Clippy completed with warnings denied. |
| `cargo +1.95.0 test --workspace --all-targets --all-features --locked` | Pass | Full workspace test suite passed after #484. |
| `cargo +1.95.0 run -p xtask -- public-surface --strict` | Pass | Public-surface policy accounts for the five publishable packages. |
| `cargo +1.95.0 run -p xtask -- arch` | Pass | Architecture dependency rules hold. |
| `cargo +1.95.0 run -p xtask -- schema-compat` | Pass | 18 historical schema fixtures deserialize with current types. |
| `cargo +1.95.0 run -p xtask -- action-check` | Pass | GitHub Action install, release asset, and failure diagnostic wiring are aligned. |
| `cargo +1.95.0 run -p xtask -- docs-source-check` | Pass | Source-of-truth metadata, IDs, links, and active goal are valid. |
| `cargo +1.95.0 run -p xtask -- product-claims-check` | Pass | Product claim proof map is valid. |
| `cargo +1.95.0 run -p xtask -- docs-check` | Pass | Documentation drift check passed. |
| `cargo +1.95.0 run -p xtask -- doc-test` | Pass | Checked 70 CLI examples and 36 structured snippets. |
| `git diff --check` | Pass | Whitespace check passed. |

## Non-Inferences

- This does not publish `0.18.0` to crates.io.
- This does not create `v0.18.0`.
- This does not create a GitHub release or release assets.
- This does not move `v0.18` or `v0`.
- This does not prove public install from crates.io, cargo-binstall, or GitHub
  release assets.
- This does not close or archive the active release cutover goal.

## Release Boundary

The active 0.18.0 release cutover remains blocked only at explicit
release-operator boundaries: crates.io publication, exact release tag, GitHub
release/assets, intentional action alias movement, public install smoke, and
publication closeout.
