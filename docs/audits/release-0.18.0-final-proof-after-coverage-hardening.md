# v0.18.0 Final Proof After Coverage Hardening

Date: 2026-05-17

Branch: `release/0-18-final-proof-after-coverage-hardening`

Commit under proof: `b9488986a44294971e55370dec549c9feb4fe1a9`

Purpose: refresh the 0.18.0 release-candidate proof after the post-SRP
coverage hardening tranche landed. This proof does not publish crates, create
tags, create a GitHub release, move action aliases, prove public install, or
close the active release cutover lane.

Superseded by:
[`v0.18.0 Restored Coverage Proof`](release-0.18.0-restored-coverage-proof.md).
A completion audit after this proof merged found that the #473, #474, and #475
merge commits referenced below were not contained in the current `main` history.
The restored proof ports those test-only commits onto current `main` and reruns
the release-candidate proof from that corrected tree.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

Linked prior proof:

- [`v0.18.0 Final Pre-Publish Proof`](release-0.18.0-final-prepublish-proof.md)
- [`v0.18.0 Publish Packet`](release-0.18.0-publish-packet.md)
- [`Metric Direction Semantics Audit`](metric-direction-semantics.md)
- [`Decision Semantics Verification`](decision-semantics-verification.md)

## Coverage Tranche Included

This proof was refreshed after these follow-up PRs landed:

| PR | Scope |
| --- | --- |
| #473 | `cli_parsing` and `repair_context` helper coverage |
| #474 | `check_guidance` and `artifact_explain` helper coverage |
| #475 | in-memory server store baseline, verdict, decision, and audit coverage |
| #476 | `decision_suggest` readiness helper coverage |
| #462 | generated public badge endpoint refresh |

The nightly baseline/trend refresh remains separate release-adjacent generated
data. It is not included in this proof as a product or release-semantics gate.

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Version under test | `0.18.0` |
| Target dir for heavy Cargo proof | `C:\perfgate-target-final-post-coverage-proof` |
| Cargo incremental | disabled with `CARGO_INCREMENTAL=0` |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |
| Publication state | Pre-publish only; no crates were uploaded |

## Command Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 fmt --all -- --check` | Pass | Formatting check completed without changes. |
| `cargo +1.95.0 check --workspace --all-targets --all-features --locked` | Pass | Workspace check completed under the post-coverage target directory. |
| `cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings` | Pass | Workspace Clippy completed with warnings denied. |
| `cargo +1.95.0 test --workspace --all-targets --all-features --locked` | Pass | Full workspace test suite passed under the post-coverage target directory. |
| `cargo +1.95.0 run -p xtask -- public-surface --strict` | Pass | Public-surface policy accounts for the five publishable packages. |
| `cargo +1.95.0 run -p xtask -- arch` | Pass | Architecture dependency rules hold. |
| `cargo +1.95.0 run -p xtask -- schema-compat` | Pass | 18 historical schema fixtures deserialize with current types. |
| `cargo +1.95.0 run -p xtask -- action-check` | Pass | GitHub Action install, release asset, and failure diagnostic wiring are aligned. |
| `cargo +1.95.0 run -p xtask -- docs-source-check` | Pass | Source-of-truth metadata, IDs, links, and active goal are valid. |
| `cargo +1.95.0 run -p xtask -- product-claims-check` | Pass | Product claim proof map is valid. |
| `cargo +1.95.0 run -p xtask -- docs-check` | Pass | Documentation drift check passed. |
| `cargo +1.95.0 run -p xtask -- doc-test` | Pass | Checked 70 CLI examples and 36 structured snippets. |
| `git diff --check` | Pass | Checked before and after this audit was added. |

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
