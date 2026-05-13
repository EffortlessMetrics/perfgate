# PERFGATE-SPEC-0005: Release proof contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: release-proof-contract.v1
Product surface: release readiness, public crates, GitHub Action, schemas, documentation, publish order
CI surface: docs-check, doc-test, action-check, public-surface, arch, schema-compat, publish-check
Schema impact: release proof must include schema compatibility when receipt shapes are release-facing
Action impact: release proof must include GitHub Action install and local reproduction wiring
Server impact: release proof must include server schema compatibility when server receipts or API fixtures are release-facing
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked ADRs: PERFGATE-ADR-0001-public-crates-are-contracts, PERFGATE-ADR-0002-receipts-first-performance-decisions
Linked plan: plans/0.18.0/implementation-plan.md
Linked policy: policy/public_crates.txt, policy/absorbed_crates.txt, policy/no-panic-*.toml, policy/clippy-*.toml, policy/*-allowlist.toml
Support/status impact: PG-CLAIM-0005 and PG-CLAIM-0008 in docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- action-check; cargo +1.95.0 run -p xtask -- public-surface --strict; cargo +1.95.0 run -p xtask -- arch; cargo +1.95.0 run -p xtask -- schema-compat; cargo +1.95.0 run -p xtask -- publish-check --package-list

## Problem

A release is not ready because versions changed. It is ready when the proof
matrix passes and the repo can show which commands validated the public surface,
schemas, action wiring, policy gates, docs, and publish order.

perfgate already has release-readiness docs and a 0.17.0 publish-readiness audit.
This spec defines the durable release-proof contract so future releases link to
those records instead of scattering release truth across README prose, PR
bodies, and chat history.

## Behavior

Release readiness MUST be evidence-based. A release candidate MUST identify:

- the release version and committed SHA under proof;
- the public crates to publish;
- the dependency order for publish dry-runs and publish operations;
- documentation and docs-example proof;
- GitHub Action proof;
- public-surface and architecture proof;
- schema compatibility proof;
- policy-ledger proof relevant to the release;
- install smoke or release-asset smoke requirements; and
- explicit boundaries for work not performed by a PR.

Release docs SHOULD link to proof records rather than copying every command
result into multiple places.

## Required proof

The release proof matrix is owned by [`docs/RELEASE_READINESS.md`](../RELEASE_READINESS.md).
Release-specific audit records live under [`docs/audits/`](../audits/), including
[`release-0.17.0-publish-readiness.md`](../audits/release-0.17.0-publish-readiness.md).

The release proof contract includes these gates:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- publish-check --package-list
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli
```

These commands SHOULD be run without `--allow-dirty` for release proof. A PR
that validates readiness but does not publish MUST state that no crates, tags,
or GitHub release assets were created.

## Publish order

The current release-order proof is:

1. `perfgate-types`
2. `perfgate`
3. `perfgate-client`
4. `perfgate-server`
5. `perfgate-cli`

Cargo verifies packaged dependencies against crates.io during downstream
publish dry-runs, so same-release workspace dependencies must be available in
that order for real publish operations. This spec does not grant permission to
publish; publish, tag, and GitHub release steps require explicit release
approval.

## Non-goals

- This spec does not publish crates.
- This spec does not create tags.
- This spec does not create GitHub releases.
- This spec does not duplicate the full `docs/RELEASE_READINESS.md` matrix.
- This spec does not replace release-specific audit files.
- This spec does not change package versions or the Rust MSRV.
- This spec does not infer publish approval from green checks.

## Acceptance examples

| Example | Result |
|---------|--------|
| A release-prep PR links to `docs/RELEASE_READINESS.md`, names the committed SHA under proof, and records all proof commands run. | Pass |
| A publish-readiness PR validates the matrix and explicitly states no crates, tags, or GitHub release assets were created. | Pass |
| A version bump PR claims release readiness without publish dry-run proof. | Fail |
| A spec copies the entire release-readiness table instead of linking to it. | Fail |
| A release operator publishes downstream crates before same-release dependencies are available. | Fail |
| A green release-proof PR is treated as implicit approval to publish. | Fail |

## Test mapping

Release proof currently maps to:

- [`docs/RELEASE_READINESS.md`](../RELEASE_READINESS.md)
- [`docs/audits/release-0.17.0-publish-readiness.md`](../audits/release-0.17.0-publish-readiness.md)
- [`docs/audits/rust-1.95-compatibility.md`](../audits/rust-1.95-compatibility.md)
- [`docs/status/PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md)
- [`policy/public_crates.txt`](../../policy/public_crates.txt)
- [`policy/absorbed_crates.txt`](../../policy/absorbed_crates.txt)
- `cargo +1.95.0 run -p xtask -- publish-check --package-list`
- per-package publish dry-run commands for the five public crates

## Implementation mapping

Release proof is implemented or documented across:

- `xtask` release and policy commands;
- workspace package metadata;
- `rust-toolchain.toml`;
- `.github/workflows/release.yml`;
- [`docs/RELEASE_READINESS.md`](../RELEASE_READINESS.md);
- [`docs/audits/`](../audits/);
- [`docs/status/PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md); and
- policy ledgers under [`policy/`](../../policy/).

## CI proof

Release-facing PRs MUST explain which release gates were run and which gates
were intentionally deferred. Hosted CI is useful evidence, but it is not by
itself enough to prove publish readiness unless the release-specific commands
above are covered.

Docs-only changes to this spec SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Promotion rule

This spec is accepted when merged as a docs-only release-proof contract. It is
implemented when:

- release-readiness docs link to status and audit proof rather than duplicating
  the same matrix in multiple places;
- the 0.18.0 implementation plan names release-proof work and proof commands;
- `.codex/goals/active.toml` points release work at this spec when release
  proof is the active work item; and
- future release candidates can answer from repo artifacts what was proven,
  what was not done, and what explicit approval is still required.
