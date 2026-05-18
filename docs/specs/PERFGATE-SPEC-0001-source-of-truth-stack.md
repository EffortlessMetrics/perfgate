# PERFGATE-SPEC-0001: Source-of-truth stack

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: source-of-truth-stack.v1
Product surface: documentation, product claims, release claims, Codex execution state
CI surface: docs-check, doc-test, future docs-source-check
Schema impact: none
Action impact: none
Server impact: none
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked ADRs: PERFGATE-ADR-0001-public-crates-are-contracts, PERFGATE-ADR-0002-receipts-first-performance-decisions, PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional
Linked plan: plans/0.18.0/implementation-plan.md
Linked policy: policy/public_crates.txt, policy/absorbed_crates.txt, policy/no-panic-*.toml, policy/*-allowlist.toml, policy/clippy-*.toml
Support/status impact: docs/status/SUPPORT_TIERS.md and docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; git diff --check

## Problem

perfgate has multiple durable truth surfaces: architecture docs, release proof,
policy ledgers, product workflow docs, GitHub Action behavior, public crate
boundaries, and Codex execution state. Those surfaces are valuable only if
future work can find the right owner for each kind of truth.

Without an explicit source-of-truth stack, the repo can drift in predictable
ways:

- README and product docs can claim support without a proof map.
- Specs can copy policy ledgers and go stale.
- Plans can redefine behavior instead of sequencing implementation.
- ADRs can become product strategy documents.
- Active Codex work can live in chat instead of a machine-readable manifest.
- Release docs can restate proof tables instead of linking to evidence records.

This spec defines the artifact ownership contract that prevents those drifts.

## Behavior

perfgate's source-of-truth stack MUST separate why, what, durable architecture
decisions, PR sequencing, active agent state, policy ledgers, product support
claims, and closeout notes.

| Truth | Source |
|-------|--------|
| Source-of-truth rail and agent boot order | `docs/reference/SPEC_SYSTEM.md` |
| Why a lane exists | `docs/proposals/` |
| Behavior and proof contract | `docs/specs/` |
| Durable architecture decision | `docs/adr/` plus the historical `docs/adrs/` archive |
| PR sequencing | `plans/<milestone>/implementation-plan.md` and work-item plans |
| Active Codex execution state | `.codex/goals/active.toml` |
| Product claim support | `docs/status/SUPPORT_TIERS.md` and `docs/status/PRODUCT_CLAIMS.md` |
| Public crate surface | `policy/public_crates.txt` |
| Absorbed/private crate disposition | `policy/absorbed_crates.txt` |
| No-panic state | `policy/no-panic-*.toml` |
| Non-Rust and workflow file surfaces | `policy/*-allowlist.toml` |
| Clippy lint policy and debt | `policy/clippy-*.toml` |
| Release-readiness proof | `docs/RELEASE_READINESS.md` and `docs/audits/` |
| Closeout and remaining work | `docs/handoffs/` |

The stack MUST obey these duplicate-truth rules:

- Proposals explain why. They MUST NOT contain the full PR checklist.
- `docs/reference/SPEC_SYSTEM.md` defines the shared source-of-truth rail,
  artifact roles, agent boot order, stop conditions, and closeout expectations.
- Specs define behavior, evidence, non-goals, and proof. They MUST NOT copy
  policy ledger entries or release-readiness tables.
- ADRs record durable architectural decisions. They MUST NOT own product
  strategy, support tiers, or temporary implementation notes.
- Plans sequence implementation. They MUST NOT redefine behavior already owned
  by specs.
- Goal TOML records current Codex execution state. It MUST NOT define new
  product behavior.
- Policy ledgers own concrete reviewed exceptions and governed surfaces.
- Status docs own product claim support tiers and proof mapping.
- Handoffs own closeout context and remaining work, not behavior contracts.

New spec-governance work SHOULD link across the stack instead of copying
content across artifacts.

## Non-goals

- This spec does not add enforcement code.
- This spec does not migrate or rewrite the historical `docs/adrs/` archive.
- This spec does not change the current five-crate public surface.
- This spec does not add, remove, publish, or reclassify crates.
- This spec does not change policy ledger entries.
- This spec does not claim that every existing README/product claim already has
  status proof coverage.

## Required evidence

The source-of-truth scaffold and this spec are proven by documentation gates:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

When the checker exists, source-of-truth changes SHOULD also run:

```text
cargo +1.95.0 run -p xtask -- docs-source-check
```

The initial checker SHOULD enforce only narrow structural facts:

- required metadata headers exist for proposals, specs, ADRs, and plans;
- proposal, spec, and ADR IDs are unique;
- linked files exist when a link is marked current rather than planned;
- specs use known status values;
- plans link to at least one proposal or spec; and
- `.codex/goals/active.toml` parses as TOML when present.

The initial checker SHOULD NOT enforce full graph completeness, support-tier
coverage, policy-ledger semantic validation, or every README claim mapping.

## Acceptance examples

These examples define the intended pass/fail behavior of the source-of-truth
model.

| Example | Result |
|---------|--------|
| A package-surface spec links to `policy/public_crates.txt` and defines the classification rule. | Pass |
| A package-surface spec copies every current crate row from the policy ledger and treats that copy as authoritative. | Fail |
| A plan lists the PR sequence, allowed files, rollback, and proof commands while linking to a spec for behavior. | Pass |
| A plan introduces a new behavior guarantee that does not appear in a spec. | Fail |
| An ADR records that public crates are contracts and owner modules are architecture boundaries. | Pass |
| An ADR is used as the only place to track a temporary PR checklist. | Fail |
| `.codex/goals/active.toml` points to the active proposal, spec, plan, allowed files, forbidden files, proof commands, and completion criteria. | Pass |
| `.perfgate/goals/active.toml` is used for Codex state. | Fail |
| A README product claim links to `docs/status/PRODUCT_CLAIMS.md` once the claim map exists. | Pass |
| A README product claim says a workflow is supported but no status or proof map owns that claim after the status docs land. | Fail |

## Test mapping

Current proof is documentation-only:

- `cargo +1.95.0 run -p xtask -- docs-check`
- `cargo +1.95.0 run -p xtask -- doc-test`
- `git diff --check`

Future proof SHOULD add `cargo +1.95.0 run -p xtask -- docs-source-check`.
That checker should cover structure and links before it attempts semantic
completeness.

## Implementation mapping

The current scaffold is implemented by:

- `docs/README.md`
- `docs/proposals/README.md`
- `docs/specs/README.md`
- `docs/adr/README.md`
- `docs/status/README.md`
- `docs/handoffs/README.md`
- `plans/README.md`
- `plans/0.18.0/README.md`
- `.codex/goals/README.md`

Follow-on artifacts SHOULD use the naming conventions documented in those
README files.

Policy ownership remains in:

- `policy/public_crates.txt`
- `policy/absorbed_crates.txt`
- `policy/clippy-lints.toml`
- `policy/clippy-debt.toml`
- `policy/clippy-exceptions.toml`
- `policy/no-panic-allowlist.toml`
- `policy/no-panic-baseline.toml`
- `policy/non-rust-allowlist.toml`
- `policy/generated-allowlist.toml`
- `policy/executable-allowlist.toml`
- `policy/workflow-allowlist.toml`
- `policy/dependency-surface-allowlist.toml`

## CI proof

For documentation-only changes in this lane, run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

For follow-on package-surface, release-proof, policy, or product-claim specs,
the relevant spec MUST name the additional gates that prove that behavior.

## Promotion rule

This spec is accepted when merged with the source-of-truth scaffold and
proposal. It is implemented when:

- the stack directories and README templates exist;
- `PERFGATE-PROP-0001` links this lane's motivation to planned specs, ADRs,
  plans, policy boundaries, status docs, and evidence;
- the 0.18.0 implementation plan exists;
- `.codex/goals/active.toml` exists and identifies the active work item; and
- the initial docs-source checker either exists or is explicitly deferred in
  the implementation plan with proof commands and scope.
