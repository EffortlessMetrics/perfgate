# v0.18.0 Final Proof After Restored Coverage

Date: 2026-05-17

Branch: `release/0-18-final-proof-after-restored-coverage`

Commit under proof: `ab70b44`

Purpose: refresh the 0.18.0 release-candidate proof after #480 restored the
missing post-SRP coverage hardening commits onto `main`, #481 refreshed the
generated public badge endpoint, and #477 refreshed generated nightly baselines
and trends. This proof does not publish crates, create tags, create a GitHub
release, move action aliases, prove public install, or close the active release
cutover lane.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

Linked prior proof:

- [`v0.18.0 Final Pre-Publish Proof`](release-0.18.0-final-prepublish-proof.md)
- [`v0.18.0 Publish Packet`](release-0.18.0-publish-packet.md)
- [`Metric Direction Semantics Audit`](metric-direction-semantics.md)
- [`Decision Semantics Verification`](decision-semantics-verification.md)
- [`v0.18.0 Restored Coverage Proof`](release-0.18.0-restored-coverage-proof.md)

## Queue State Included

This proof was refreshed after the remaining generated release-adjacent queue was
resolved:

| PR | Scope |
| --- | --- |
| #480 | restored #473-#475 post-SRP coverage hardening onto current `main` |
| #481 | generated public badge endpoint refresh |
| #477 | generated nightly baselines and trend data refresh |

The badge and baseline/trend refreshes are generated operational inputs. They
are included in the checked tree for this proof, but they do not change release
semantics, publish state, action aliases, receipt schemas, or public APIs.

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Version under test | `0.18.0` |
| Target dir for full release proof | `C:\perfgate-target-0-18-final-proof-after-restored-coverage` |
| Cargo incremental | disabled with `CARGO_INCREMENTAL=0` |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |
| Publication state | Pre-publish only; no crates were uploaded |

## Command Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 fmt --all -- --check` | Pass | Formatting check completed without changes. |
| `cargo +1.95.0 check --workspace --all-targets --all-features --locked` | Pass | Workspace check completed under the final post-restore target directory. |
| `cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings` | Pass | Workspace Clippy completed with warnings denied. |
| `cargo +1.95.0 test --workspace --all-targets --all-features --locked` | Pass | Full workspace test suite passed under the final post-restore target directory. |
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
- This does not prove public install from crates.io, `cargo-binstall`, or
  GitHub release assets.
- This does not close the release cutover lane.

## Release Boundary

The active 0.18.0 release cutover remains blocked only at explicit
release-operator boundaries: crates.io publication, exact release tag, GitHub
release/assets, intentional action alias movement, public install smoke, and
publication closeout.
