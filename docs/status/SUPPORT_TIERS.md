# Support Tiers

perfgate uses support tiers to keep product claims tied to proof. A claim's
tier is not marketing language; it describes how much compatibility, test
coverage, and operational confidence the repo currently promises.

## Tier Definitions

| Tier | Meaning | Required evidence |
|------|---------|-------------------|
| stable | Public contract with release-grade proof and compatibility expectations. Breaking changes need release notes, migration guidance, and spec updates. | Spec or policy owner, CI proof, release-readiness proof when release-facing, and a review cadence. |
| supported | Intended user path with maintained tests, docs, and proof commands. Behavior may evolve, but changes must update docs and tests. | Linked docs, tests or policy gates, proof commands, and review cadence. |
| advisory | Visible and useful, but not a hard gate or compatibility promise. Failures should inform review rather than silently block users. | Documentation of scope and at least one proof or fixture showing the signal shape. |
| experimental | Available for trial; behavior may change without migration guarantees. | Clear non-goals, owner, and follow-up path before promotion. |
| deprecated | Still present, but users should move away from it. | Replacement path, deprecation note, and removal or review target. |

## Promotion Rules

A claim can move to a stronger tier only when the source-of-truth stack owns the
necessary evidence:

- The behavior is described by a spec, policy ledger, release-readiness proof,
  or existing product doc.
- Proof commands are listed in `docs/status/PRODUCT_CLAIMS.md`.
- Linked tests, fixtures, policy gates, or CI lanes cover the claim.
- The claim has a `review_after` value so support does not become stale by
  default.

## Demotion Rules

A claim should move to a weaker tier when:

- its proof commands no longer pass;
- the linked behavior changes without matching docs or tests;
- the support surface becomes advisory by policy;
- the claim depends on unreleased or externally blocked work; or
- the owner cannot identify current proof during release readiness.

## Review Cadence

Use concrete review targets:

- `before-0.18.0-release`
- `next-msrv-change`
- `next-public-surface-change`
- `next-decision-contract-change`
- `next-release-candidate`

Date-based reviews are allowed when a calendar deadline matters, but release or
policy milestones are preferred when they are the real support boundary.

## Ownership Boundaries

- Specs define behavior and proof contracts.
- Policy ledgers own concrete exceptions and governed surfaces.
- Release-readiness docs own release proof matrices and audit links.
- Product claims map user-facing promises to tiers and evidence.
- Handoffs record temporary status and remaining work.
