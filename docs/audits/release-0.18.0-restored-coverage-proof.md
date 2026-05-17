# v0.18.0 Restored Coverage Proof

Date: 2026-05-17

Branch: `test/restore-post-srp-coverage-hardening`

Purpose: restore the post-SRP coverage hardening tranche onto current `main`
after a completion audit found the earlier
[`v0.18.0 Final Proof After Coverage Hardening`](release-0.18.0-final-proof-after-coverage-hardening.md)
referenced #473, #474, and #475 even though their merge commits were not present
in the branch under proof. This proof reruns the release-candidate gates from
the corrected tree. It does not publish crates, create tags, create a GitHub
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
- [`v0.18.0 Final Proof After Coverage Hardening`](release-0.18.0-final-proof-after-coverage-hardening.md)

## Correction

The earlier final proof after coverage hardening was merged from a branch that
included #476 and the generated badge refresh, but current `main` did not contain
the #473, #474, or #475 merge commits it cited. GitHub reported those PRs merged,
but their merge commits were absent from the checked-out `main` history and the
expected test bodies were not present in the working tree.

This restored proof ports the original test-only coverage commits onto current
`main`:

| Restored commit | Original PR scope |
| --- | --- |
| `0fdad9d` | #473 `cli_parsing` and `repair_context` helper coverage |
| `2104916` | #474 `check_guidance` and `artifact_explain` helper coverage |
| `343fc0a` | #474 Windows guidance artifact path normalization |
| `be993e5` | #475 in-memory server store baseline, verdict, decision, and audit coverage |

The already-present #476 `decision_suggest` readiness helper coverage and the
generated badge refresh remain on the corrected release-candidate tree.

The nightly baseline/trend refresh remains separate release-adjacent generated
data. It is not included in this proof as a product or release-semantics gate.

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Version under test | `0.18.0` |
| Target dir for targeted coverage proof | `C:\perfgate-target-restored-coverage` |
| Target dir for full release proof | `C:\perfgate-target-restored-final-proof` |
| Cargo incremental | disabled with `CARGO_INCREMENTAL=0` for full proof |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |
| Publication state | Pre-publish only; no crates were uploaded |

## Targeted Coverage Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 test -p perfgate-cli cli_parsing::tests` | Pass | 42 CLI parsing tests passed. |
| `cargo +1.95.0 test -p perfgate-cli repair_context::tests` | Pass | 8 repair context tests passed. |
| `cargo +1.95.0 test -p perfgate-cli check_guidance::tests` | Pass | 33 check guidance tests passed. |
| `cargo +1.95.0 test -p perfgate-cli artifact_explain::tests` | Pass | 17 artifact explanation tests passed. |
| `cargo +1.95.0 test -p perfgate-cli decision_suggest::tests` | Pass | 6 decision suggest tests passed. |
| `cargo +1.95.0 test -p perfgate-server storage::memory::tests` | Pass | Server memory store storage tests passed. |

## Command Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 fmt --all -- --check` | Pass | Formatting check completed without changes. |
| `cargo +1.95.0 check --workspace --all-targets --all-features --locked` | Pass | Workspace check completed under the restored final-proof target directory. |
| `cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings` | Pass | Workspace Clippy completed with warnings denied. |
| `cargo +1.95.0 test --workspace --all-targets --all-features --locked` | Pass | Full workspace test suite passed under the restored final-proof target directory. |
| `cargo +1.95.0 run -p xtask -- public-surface --strict` | Pass | Public-surface policy accounts for the five publishable packages. |
| `cargo +1.95.0 run -p xtask -- arch` | Pass | Architecture dependency rules hold. |
| `cargo +1.95.0 run -p xtask -- schema-compat` | Pass | Historical schema fixtures deserialize with current types. |
| `cargo +1.95.0 run -p xtask -- action-check` | Pass | GitHub Action install, release asset, and failure diagnostic wiring are aligned. |
| `cargo +1.95.0 run -p xtask -- docs-source-check` | Pass | Source-of-truth metadata, IDs, links, and active goal are valid. |
| `cargo +1.95.0 run -p xtask -- product-claims-check` | Pass | Product claim proof map is valid. |
| `cargo +1.95.0 run -p xtask -- docs-check` | Pass | Documentation drift check passed. |
| `cargo +1.95.0 run -p xtask -- doc-test` | Pass | CLI examples and structured snippets passed. |
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
