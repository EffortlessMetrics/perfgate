# PERFGATE-SPEC-0002: Package surface boundary

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: package-surface-boundary.v1
Product surface: public crates, workspace package classification, release proof
CI surface: public-surface --strict, arch
Schema impact: none
Action impact: none
Server impact: none
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked ADRs: PERFGATE-ADR-0001-public-crates-are-contracts
Linked plan: plans/0.18.0/package-surface-boundary.md
Linked policy: policy/public_crates.txt, policy/absorbed_crates.txt
Support/status impact: PG-CLAIM-0004 in docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- public-surface --strict; cargo +1.95.0 run -p xtask -- arch

## Problem

perfgate previously used many workspace crates as architecture boundaries. The
current release lane keeps the architecture seams, but it no longer treats every
seam as a durable public package. Without an explicit package-surface contract,
future work could accidentally recreate a durable unpublished production crate
category or expand the publishable API surface without policy review.

This spec pins the package classification rule that release readiness already
uses: public crates are contracts, folders and modules are architecture
boundaries, and workspace packages must be classified by policy.

## Behavior

perfgate MUST keep a small, reviewed public crate surface. The current public
crates are:

- `perfgate`
- `perfgate-cli`
- `perfgate-types`
- `perfgate-client`
- `perfgate-server`

Those crate names are repeated here because the five-crate list is the behavior
under test. The policy file remains the concrete machine-readable source:
[`policy/public_crates.txt`](../../policy/public_crates.txt).

Every workspace package MUST be classified as exactly one of:

- published public crate;
- private dev/test crate; or
- temporary compatibility wrapper scheduled for owner-module absorption.

There is no durable unpublished production crate category. If production code
needs an architecture boundary, it SHOULD use an owner module inside the
appropriate public or internal owning crate unless a new public crate is
justified by policy and ADR review.

The absorbed/private disposition is governed by
[`policy/absorbed_crates.txt`](../../policy/absorbed_crates.txt). Human-facing
context lives in [`docs/CRATE_SEAMS.md`](../CRATE_SEAMS.md) and
[`docs/ARCHITECTURE.md`](../ARCHITECTURE.md).

## Required classifications

The classification contract is:

| Classification | Meaning | Source |
|----------------|---------|--------|
| Published public crate | Durable external package contract. | `policy/public_crates.txt` |
| Private dev/test crate | Workspace package used for fixtures, tests, or internal development only. | Workspace metadata and policy review |
| Temporary compatibility wrapper | Transitional package kept only while callers move to owner modules. | `policy/absorbed_crates.txt` |

New workspace packages MUST update the relevant policy or explicitly document
why they are outside this production package contract.

## Non-goals

- This spec does not absorb or delete crates.
- This spec does not publish crates.
- This spec does not change `Cargo.toml`.
- This spec does not change `policy/public_crates.txt`.
- This spec does not change `policy/absorbed_crates.txt`.
- This spec does not replace `docs/CRATE_SEAMS.md`; it defines the contract
  that document explains.

## Required evidence

Package-surface changes MUST run:

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

Documentation-only changes to this spec SHOULD also run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Acceptance examples

| Example | Result |
|---------|--------|
| A new owner module is added inside `perfgate::domain` for internal policy logic. | Pass |
| A temporary compatibility wrapper remains listed in `policy/absorbed_crates.txt` while callers migrate. | Pass |
| A new publishable workspace package is added without updating `policy/public_crates.txt` and an ADR. | Fail |
| A production crate is marked `publish = false` and treated as a durable architecture boundary without policy classification. | Fail |
| `perfgate-types` remains separate for stable schemas, config, receipts, and API contracts. | Pass |
| `perfgate-client` and `perfgate-server` remain separate for baseline-service contract seams. | Pass |

## Test mapping

The package surface is covered by:

- `cargo +1.95.0 run -p xtask -- public-surface --strict`
- `cargo +1.95.0 run -p xtask -- arch`
- [`policy/public_crates.txt`](../../policy/public_crates.txt)
- [`policy/absorbed_crates.txt`](../../policy/absorbed_crates.txt)
- [`docs/CRATE_SEAMS.md`](../CRATE_SEAMS.md)

The status proof map tracks the user-facing claim as `PG-CLAIM-0004`.

## Implementation mapping

The current implementation is spread across:

- workspace package metadata in `Cargo.toml` and crate manifests;
- `policy/public_crates.txt`;
- `policy/absorbed_crates.txt`;
- `xtask` public-surface validation;
- `xtask` architecture validation;
- `docs/CRATE_SEAMS.md`; and
- `docs/RELEASE_READINESS.md`.

This spec SHOULD spawn a decrating or absorption plan when a compatibility
wrapper is ready to move behind an owner module. That plan must not be bundled
with this spec-only PR.

## CI proof

Release-facing changes that affect package classification MUST pass:

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

Release proof SHOULD also include publish preflight commands from
[`docs/RELEASE_READINESS.md`](../RELEASE_READINESS.md) rather than copying the
full release matrix here.

## Promotion rule

This spec is accepted when merged with no package or policy changes. It is
implemented when:

- `PERFGATE-ADR-0001-public-crates-are-contracts` records the durable
  architecture decision;
- `plans/0.18.0/package-surface-boundary.md` identifies the follow-on PR
  sequence for any remaining wrapper absorption;
- `PG-CLAIM-0004` links the public surface claim to proof commands; and
- `public-surface --strict` and `arch` remain release gates.
