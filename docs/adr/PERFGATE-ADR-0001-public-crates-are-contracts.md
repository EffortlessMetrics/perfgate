# PERFGATE-ADR-0001: Public crates are contracts

Status: accepted
Date: 2026-05-13
Owner: perfgate maintainers
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked specs: docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md

## Decision

perfgate keeps a small public crate surface and uses owner modules inside
owning crates for architecture boundaries. Internal seams should not become
public crates unless they carry a durable external contract.

The durable public crates are:

- `perfgate`
- `perfgate-cli`
- `perfgate-types`
- `perfgate-client`
- `perfgate-server`

Workspace packages outside that list must be private dev/test crates or
temporary compatibility wrappers scheduled for owner-module absorption. There
is no durable unpublished production crate category.

## Context

perfgate's earlier modularization used many crates to make boundaries visible.
That helped isolate concerns, but it also made the public package surface look
larger than the contract perfgate intends to support long term.

The current architecture keeps the useful boundaries but moves them behind
owning crates and modules when they do not need independent external contracts.
The public-surface policy now distinguishes package contracts from internal
architecture seams.

This decision aligns:

- release proof, which publishes only the reviewed public crates;
- policy ledgers, which classify public and absorbed package surfaces;
- architecture docs, which explain module and seam ownership; and
- Codex work planning, which should not create new public crates as a default
  way to express architecture.

## Consequences

- `perfgate-types` remains separate for schemas, config, receipts, and API
  contracts.
- `perfgate-client` and `perfgate-server` remain separate for baseline-service
  client/server contract seams.
- `perfgate` remains the embeddable facade and owner for internal domain,
  runtime, presentation, and integration modules.
- `perfgate-cli` remains the installable command-line surface.
- Compatibility wrappers are temporary unless explicitly classified as
  dev/test/private by policy.
- New public crates require policy updates and ADR justification.
- Internal architecture work should prefer owner modules over new workspace
  crates.
- `cargo +1.95.0 run -p xtask -- public-surface --strict` and
  `cargo +1.95.0 run -p xtask -- arch` remain the executable proof for this
  boundary.

## Alternatives considered

### Keep every architecture seam as a public crate

Rejected. It makes internal boundaries visible but turns implementation seams
into external contracts, increasing release and compatibility burden.

### Keep unpublished production crates as a durable category

Rejected. It creates a third state that is neither a public contract nor a
clearly private/test-only package. That makes release proof and policy review
harder to reason about.

### Collapse all code into one crate without owner modules

Rejected. perfgate still needs strong domain, runtime, presentation, app, and
integration boundaries. The decision is about package surface, not removing
architecture.

### Add public crates whenever Codex needs a narrow work area

Rejected. PR scope should be enforced through plans, allowed files, modules,
and tests. New crates are a public-surface decision, not an agent convenience.

## Follow-up specs / plans

- `docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md`
- `plans/0.18.0/package-surface-boundary.md`
- `policy/public_crates.txt`
- `policy/absorbed_crates.txt`
- `docs/CRATE_SEAMS.md`

The follow-on plan should sequence wrapper absorption separately from this ADR.
