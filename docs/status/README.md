# Status Docs

Status docs map product claims to proof. They keep README, release, action, CLI,
and server claims honest by tying each claim to a support tier, evidence, and
review cadence.

This directory is the home for:

```text
SUPPORT_TIERS.md
PRODUCT_CLAIMS.md
PLATFORM_SUPPORT.md
CANARY_MATRIX.md
PROOF_FRESHNESS.md
```

Those files are intentionally introduced after this scaffold so the claim map
can land as its own reviewable PR.

## Support Tiers

Use these tiers unless a later spec changes the vocabulary:

| Tier | Meaning |
|------|---------|
| stable | Public contract with release-grade proof and compatibility expectations |
| supported | Intended user path with tests, docs, and maintained behavior |
| advisory | Visible and useful, but not a hard gate or compatibility promise |
| experimental | Available for trial; behavior may change without migration guarantees |
| deprecated | Still present, but users should move away from it |

## Product Claim Row

Each claim should record:

```text
claim id
claim text
tier
surface
proof commands
linked tests or policy gates
artifacts
review_after
linked specs
linked release/readiness notes
```

## Boundaries

- README and release docs may summarize claims, but the proof map owns the
  current support tier.
- Specs define the behavior contract behind a claim.
- Policy files own machine-readable gates and exceptions.
- Handoffs record temporary status and remaining work.
