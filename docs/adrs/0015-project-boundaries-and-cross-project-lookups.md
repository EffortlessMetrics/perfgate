# ADR 0015: Project Boundaries and Cross-Project Lookups

## Status
Accepted

## Context

Perfgate's architecture defines the product as a selective build-truth sensor for
CI performance budgets. It is designed to answer a narrow question: did a change
regress end-to-end performance beyond an explicit budget. The optional baseline
service exists to store and query baselines, not to redefine the product as a
general performance analytics platform.

ADR 0011 established the baseline server as a multi-tenant system with explicit
project isolation:

- baselines and verdicts are namespaced by project
- non-admin credentials are restricted to their assigned project
- the `*` project scope is reserved for admin-style access

PR #212 introduced a small ergonomic feature on top of that model:
`perfgate compare` can now fetch a server baseline from another project via
`--baseline-project` while preserving the existing `@server:<benchmark>` selector.

That shipped slice is intentionally narrower than "cross-project comparison" as a
larger product capability. Without an explicit decision, it would be easy for
future small changes to blur the line between project as a hard namespace
boundary and project as a soft query parameter.

## Decision

`project` remains a hard isolation and ownership boundary in perfgate.

This means:

- baseline storage, verdict storage, auth scope, and primary dashboard views
  remain project-scoped
- `perfgate compare --baseline-project <project>` is a lookup override for
  baseline resolution only; it does not change the caller's default project or
  introduce implicit cross-project server APIs
- the shipped `@server:` plus `--baseline-project` flow is an ergonomic client
  feature, not a federation model
- any future feature that compares, aggregates, or queries across multiple
  projects on the server must define a dedicated API, auth model, and audit
  semantics before implementation
- roadmap priority remains signal credibility first: flakiness history, weighted
  fleet aggregation, and service hardening come before broader cross-project
  federation or plugin expansion

## Consequences

### Positive

- Preserves the product's design center as a CI verdict sensor rather than a
  general-purpose performance platform.
- Keeps ADR 0011's multi-tenancy model coherent after the compare-time lookup
  improvement shipped in PR #212.
- Allows useful cross-project baseline lookup in the CLI without silently
  changing server trust boundaries.
- Forces future federation work to be designed deliberately instead of emerging
  through a series of unrelated small patches.

### Negative

- Full cross-project comparison remains future work.
- Users who want server-side multi-project queries still need a separately
  designed API and auth model.
- The roadmap must distinguish between shipped lookup ergonomics and unshipped
  federation capabilities to avoid ambiguity.

## References

- [Architecture](../ARCHITECTURE.md)
- [Roadmap](../ROADMAP.md)
- [ADR 0011: Authentication and Multi-tenancy](0011-authentication-and-multi-tenancy.md)
- PR #212: `feat: support cross-project baseline lookups in compare`
