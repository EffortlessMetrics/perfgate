# The spec/proposal system, fully explained

The system is a **repo source-of-truth stack**. Its central rule is:

> **Do not make every document do every job.**

Each artifact owns one kind of truth: **why**, **what**, **what decision**,
**how**, **what now**, **what proves it**, and **what changed**.

## Stack at a glance

```text
Roadmap
  -> Proposal / PRD
    -> Specs
      -> ADRs where needed
        -> Implementation plan
          -> Active goal manifest
            -> Issues / PRs
              -> Proof commands
              -> CI lanes
              -> support-tier updates
              -> policy receipts
                -> Closeout / handoff
```

## Minimal mental model

```text
Proposal = why.
Spec = what.
ADR = durable decision.
Plan = how.
Active goal = what Codex is doing now.
Support tiers = what users may believe.
Policy ledgers = what exceptions and proof obligations exist.
CI = what proved it.
Closeout = what happened.
```

The system works when those layers are **linked, validated, and not
duplicating each other**.

## Core operating principles

1. **One artifact, one kind of truth.**
2. **Specs are contracts, not queues.**
3. **Plans are PR-sized.**
4. **Claims must be proof-mapped.**
5. **Policy exceptions are ledgers, not vibes.**
6. **Agent state must be machine-readable.**
7. **Do not encode fake repo rules.**
8. **Verify named commands, crates, lints, APIs, and workflows before use.**

## Repository layout target

```text
docs/
  proposals/
  specs/
  adr/
  status/
  handoffs/
plans/
.codex/goals/
policy/
```

## Link graph expectations

- Roadmap links to proposals.
- Proposals link to specs, ADRs, and plans.
- Specs link to proposals and proof commands.
- Plans link to proposal/spec/ADR IDs and PR-sized work items.
- Active goal manifests link to the currently active plan items.
- PRs link back to the governing proposal/spec/plan artifacts.
- Closeouts capture what landed, what proved it, and what remains.

## Duplication guardrail

Keep each truth in exactly one primary location:

- Support tiers in `docs/status/SUPPORT_TIERS.md`.
- CI lane policy in `policy/ci-lane-whitelist.toml`.
- Package classification in `policy/package-boundary.toml`.
- Active agent execution in `.codex/goals/active.toml`.
- PR ordering in `plans/<milestone>/implementation-plan.md`.

Specs should reference these owners rather than duplicating their tables.

## Rollout sequence (minimal)

1. Define stack docs and templates.
2. Add `policy/doc-artifacts.toml`.
3. Add `cargo xtask check-doc-artifacts`.
4. Add `.codex/goals/active.toml`.
5. Add `cargo xtask check-goals`.
6. Add first proposal and first spec.
7. Add support-tier mapping.
8. Add package/CI/policy ledgers.
9. Wire CI checks (advisory first, then selected blockers).

## Why this exists

This system provides **repo-operational memory** so humans and agents can
resolve intent, contract, proof, and current execution state directly from the
repository instead of stale chat context.
