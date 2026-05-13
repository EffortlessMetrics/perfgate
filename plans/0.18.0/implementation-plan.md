# perfgate 0.18.0 Spec-driven Governance Implementation Plan

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Current PR: complete
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked specs: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md, docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md, docs/specs/PERFGATE-SPEC-0003-performance-decision-contract.md, docs/specs/PERFGATE-SPEC-0004-user-devex-paved-road.md, docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Linked ADRs: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md, docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Linked policy: policy/public_crates.txt, policy/absorbed_crates.txt, policy/clippy-*.toml, policy/no-panic-*.toml, policy/*-allowlist.toml
Support/status impact: docs/status/SUPPORT_TIERS.md, docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test
Blocks: none
Blocked by: none
Rollback: revert the closeout handoff, plan status update, and archived goal manifest; no product behavior changes

## Goal

Make perfgate's product claims, architecture decisions, policy ledgers, release
proof, and Codex execution state traceable through proposals, specs, ADRs,
plans, status docs, policy ledgers, and machine-readable active goals.

This plan sequences the work. It does not redefine behavior already owned by
specs and ADRs.

## Operating Rules

- Keep one semantic documentation artifact per PR unless the work item is
  explicitly a paired artifact such as the plan plus active goal.
- Specs link to policy ledgers instead of copying policy rows.
- Release-proof docs link to `docs/RELEASE_READINESS.md` and audit records
  instead of duplicating the whole release matrix.
- Status docs map product claims to proof commands.
- `.codex/goals/active.toml` owns the current Codex execution state.
- `.perfgate/` remains reserved for product-generated user artifacts.
- Do not infer publish, tag, or GitHub release approval from green checks.

## PR Sequence

| PR | Work item | Status | Files |
|----|-----------|--------|-------|
| 1 | Source-of-truth scaffold | merged | `docs/README.md`, taxonomy READMEs, `plans/README.md`, `.codex/goals/README.md` |
| 2 | Spec-driven governance proposal | merged | `docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md` |
| 3 | Source-of-truth stack spec | merged | `docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md` |
| 4 | Product claim proof map | merged | `docs/status/SUPPORT_TIERS.md`, `docs/status/PRODUCT_CLAIMS.md` |
| 5 | Package surface boundary spec | merged | `docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md` |
| 6 | Public crates ADR | merged | `docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md` |
| 7 | Performance decision contract spec | merged | `docs/specs/PERFGATE-SPEC-0003-performance-decision-contract.md` |
| 8 | Receipts-first ADR | merged | `docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md` |
| 9 | User DevEx paved-road spec | merged | `docs/specs/PERFGATE-SPEC-0004-user-devex-paved-road.md` |
| 10 | Release-proof contract spec | merged | `docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md` |
| 11 | Implementation plan and active goal | merged | `plans/0.18.0/implementation-plan.md`, `.codex/goals/active.toml` |
| 12 | Source-of-truth docs checker | merged | `xtask`, checker tests, docs |
| 13 | Product claim proof checker | merged | `xtask`, checker tests, docs |
| 14 | Final closeout | merged | `docs/handoffs/2026-05-13-spec-driven-governance-closeout.md`, `plans/0.18.0/implementation-plan.md`, `.codex/goals/archive/perfgate-0-18-spec-driven-governance.toml` |

## Work item: source-of-truth-scaffold

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md
Linked ADR:
Blocks: source-of-truth-stack, implementation-plan
Blocked by:

### Goal

Create the artifact homes and templates for proposals, specs, ADRs, plans,
active goals, status docs, and handoffs.

### Acceptance

- Taxonomy READMEs exist.
- No product, policy, schema, workflow, Cargo, or Rust code files changed.
- Docs gates passed.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the scaffold commit. No data migration or product behavior is involved.

## Work item: product-claim-proof-map

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md
Linked ADR:
Blocks: product-claims-check
Blocked by:

### Goal

Add support tiers and map current user-facing claims to proof commands, linked
tests, policy gates, artifacts, and review cadence.

### Acceptance

- `docs/status/SUPPORT_TIERS.md` defines the tier vocabulary.
- `docs/status/PRODUCT_CLAIMS.md` records claim IDs, tiers, surfaces, proof,
  tests or policy gates, artifacts, and review targets.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the status docs commit. README/product claims should not rely on the
claim map until the rollback is resolved.

## Work item: package-surface-boundary

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md
Linked ADR: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Blocks: decrating follow-up, source docs checker
Blocked by:

### Goal

Pin the rule that public crates are contracts, modules are architecture
boundaries, and there is no durable unpublished production crate category.

### Acceptance

- The five public crates remain linked to `policy/public_crates.txt`.
- Absorbed/private dispositions remain linked to `policy/absorbed_crates.txt`.
- No crate or policy changes land in the spec/ADR PRs.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

### Rollback

Revert the spec/ADR docs commits. Do not change policy files as part of the
rollback unless a separate package-surface PR requires it.

## Work item: performance-decision-contract

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0003-performance-decision-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks: optional-server-ledger ADR
Blocked by:

### Goal

Define the receipts-first decision workflow and protect the local artifact
contract, portable bundles, action reproduction command, and optional server
ledger boundary.

### Acceptance

- Required artifacts and user-facing answers are specified.
- Server history is optional and cannot become a correctness prerequisite.
- Status claims link decisions, bundles, server ledger, and action reproduction
  to proof commands.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
```

### Rollback

Revert the spec/ADR docs commits. Product behavior remains unchanged.

## Work item: user-devex-paved-road

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0004-user-devex-paved-road.md
Linked ADR:
Blocks: product-claims-check
Blocked by:

### Goal

Protect the beginner path from install through local baseline promotion.

### Acceptance

- The first-run command sequence is specified.
- Failure UX expectations are documented.
- The local path does not require a server.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features first_run
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- action-check
```

### Rollback

Revert the spec commit. Product behavior remains unchanged.

## Work item: release-proof-contract

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Blocks: release closeout and future release lanes
Blocked by:

### Goal

Define release readiness as a proof matrix, not a version bump.

### Acceptance

- The spec links to release readiness and audit records.
- Publish order is explicit.
- Publish, tag, and GitHub release approval remain outside green-check
  inference.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- publish-check --package-list
```

Per-package release-candidate dry-runs are listed in
`docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md`.

### Rollback

Revert the spec commit. Release-readiness docs remain the existing source of
release proof.

## Work item: implementation-plan-and-active-goal

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md
Linked ADR:
Blocks: docs-source-check, product-claims-check, final closeout
Blocked by:

### Goal

Make the campaign executable by Codex from repo artifacts alone.

### Acceptance

- This implementation plan sequences PR-sized work.
- `.codex/goals/active.toml` parses as TOML.
- The active goal names current and remaining work, allowed files, forbidden
  files, proof commands, and completion criteria.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert this plan and `.codex/goals/active.toml`. Already merged specs and ADRs
remain valid.

## Work item: docs-source-check

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md
Linked ADR:
Blocks: final closeout
Blocked by: implementation-plan-and-active-goal

### Goal

Add the first source-of-truth checker without over-enforcing graph completeness.

### Acceptance

- Required metadata headers exist.
- Proposal/spec/ADR IDs are unique.
- Linked files exist when marked current.
- Specs use known status values.
- Plans link to at least one proposal or spec.
- `.codex/goals/active.toml` parses as TOML.

### Proof commands

```text
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
```

### Rollback

Revert the checker and its tests. The docs stack remains usable manually.

## Work item: product-claims-check

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md
Linked ADR:
Blocks: final closeout
Blocked by: product-claim-proof-map

### Goal

Make `docs/status/PRODUCT_CLAIMS.md` useful as a checked proof map.

### Acceptance

Every claim has:

- claim id;
- tier;
- surface;
- proof commands;
- linked tests or policy gates; and
- `review_after`.

### Proof commands

```text
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- docs-check
```

### Rollback

Revert the checker and tests. The status map remains the human-readable source.

## Work item: final-closeout

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md
Linked ADR:
Blocks:
Blocked by: docs-source-check, product-claims-check

### Goal

Close the lane with evidence that a future maintainer or Codex session can
answer the lane questions from repo artifacts alone.

### Acceptance

- All planned artifacts are merged or explicitly deferred.
- Source-of-truth and product-claim checks pass.
- `.codex/goals/active.toml` is archived or updated for the next active lane.
- A handoff records what changed, proof commands, and remaining work.

### Proof commands

```text
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
```

### Rollback

Revert the closeout artifact only. Do not undo merged source-of-truth artifacts
unless a separate rollback plan calls for it.
