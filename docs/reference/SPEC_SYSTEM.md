# perfgate source-of-truth system

perfgate uses a linked source-of-truth stack so humans and agents can find the
right kind of truth without scraping chat history or treating stale notes as
current state.

## Stack

```text
Roadmap
  -> Proposal
    -> Spec
      -> ADR
        -> Implementation plan
          -> Active goal
            -> PR
              -> Proof
```

## Artifact roles

| Artifact | Owns | Does not own |
| --- | --- | --- |
| Roadmap | Release direction, milestone framing, and lane selection | PR queue or proof receipts |
| Proposal | Why a lane exists, users, alternatives, risks, and success criteria | Behavior contracts or task lists |
| Spec | Required behavior, acceptance, evidence, proof, and claim boundaries | PR sequencing or product rationale |
| ADR | Durable architecture or operating decisions | Current task queues or metric state |
| Plan | Work-item order, production deltas, proof commands, rollback, and blockers | Product motivation or durable decisions |
| Active goal | Machine-readable current lane, work item, pointers, commands, and boundaries | Generated status or new behavior |
| Support tiers | Public claim support, proof pointers, limitations, and promotion requirements | Feature design |
| Policy ledgers | Exceptions, governed surfaces, owners, review dates, and coverage | Broad architecture rationale |
| Handoffs | Closeout context, remaining work, and operator notes | Behavior contracts |

## perfgate locations

| Question | Source of truth |
| --- | --- |
| Why are we doing this? | `docs/proposals/` |
| What must be true? | `docs/specs/` |
| What architecture decision did we make? | `docs/adr/` plus the historical `docs/adrs/` archive |
| What PR lands next? | `plans/<milestone>/implementation-plan.md` or a linked work-item plan |
| What is the agent actively executing? | `.codex/goals/active.toml` |
| What proves product claims? | `docs/status/SUPPORT_TIERS.md`, `docs/status/PRODUCT_CLAIMS.md`, receipts, and CI proof |
| What exceptions exist? | `policy/*.toml` and `policy/*.txt` ledgers |

`.codex/goals/` is the active agent-control surface for this repository.
`.perfgate/` is reserved for product-generated user artifacts and must not be
used for Codex goal state.

## Rules

1. Keep one kind of truth per artifact.
2. Prefer one semantic artifact per PR unless the plan explicitly says
   otherwise.
3. Specs define behavior; plans define sequencing.
4. Proposals explain why; ADRs record decisions.
5. Active goals tell agents what to do now.
6. Generated status is updated by tools, not by hand.
7. Public claims require support-tier or product-claim proof.
8. Policy exceptions require owner, reason, coverage, and review date.
9. Proof commands must be run before claiming completion, or the unavailable
   proof must be recorded explicitly.

## Required metadata

Every proposal, spec, ADR, and plan should start with a short metadata block.
Use `n/a` or `none` when a field does not apply. Subdirectory READMEs define
artifact-specific fields; source-of-truth artifacts should still declare these
ideas somewhere in the header:

```text
Status:
Owner:
Created:
Linked proposal:
Linked specs:
Linked ADRs:
Linked plan:
Linked issues:
Linked PRs:
Support/status impact:
Policy impact:
Proof commands:
```

## Agent boot order

Agents should begin with this sequence before changing files:

1. Read `AGENTS.md`, `CLAUDE.md`, or other repo agent instructions.
2. Read this file.
3. Read `.codex/goals/active.toml`.
4. Read the linked implementation plan.
5. Read the linked proposal only for why.
6. Read the linked spec for acceptance and proof.
7. Read linked ADRs for constraints.
8. Inspect `git status --short`.
9. Pick exactly one ready work item.
10. Implement only that work item.
11. Run the listed proof commands and `git diff --check`.
12. Update only the plan, status, policy, or receipts required by the work item.
13. Open or update one focused PR.

If no ready work item is identifiable, stop and write a handoff instead of
inventing scope.

## Stop conditions

Stop and report instead of guessing when:

- the active goal is missing, stale, or contradictory;
- linked files do not exist;
- generated status is dirty;
- proof commands cannot run;
- unrelated staged files exist;
- requested work conflicts with an ADR;
- the requested change would broaden public claims without support-tier proof;
- a policy exception is needed but no policy ledger owner, reason, coverage,
  and review date are available.

## Active goal lifecycle

Use exactly one active goal manifest:

```text
.codex/goals/active.toml
```

For a paused lane, set `status = "paused"` and include a reason. When replacing
a lane, archive the old manifest under `.codex/goals/archive/` before creating a
new active manifest.

Active goals are TOML pointers and bounded execution state. They must not become
long-form plans, generated status tables, or behavior specs.

## Closeout format

A completed lane should have a closeout or handoff that records:

- what shipped;
- proof commands and receipts;
- PRs and CI runs;
- generated status, support-tier, and policy updates;
- what did not ship;
- deferred work;
- claim boundaries; and
- the next lane recommendation.

Closeout prevents the next agent from rediscovering old work.

## Common failure modes

### Spec becomes a task list

Move PR order to `plans/<milestone>/implementation-plan.md` and keep the spec to
behavior, examples, proof, and claim boundaries.

### Plan becomes product rationale

Move why to `docs/proposals/` and keep the plan to work items, dependencies,
proof, and rollback.

### Active goal becomes prose

Keep `.codex/goals/active.toml` machine-readable and link out to prose docs.

### Agent hand-edits generated status

Run the generator or checker named in the plan. If it cannot run, record that as
unavailable proof.

### Support claims drift

Require support/status impact in source-of-truth artifacts and map public claims
to `docs/status/SUPPORT_TIERS.md` or `docs/status/PRODUCT_CLAIMS.md`.

### Policy exceptions become silent debt

Every exception needs an owner, reason, `covered_by` or equivalent coverage,
`review_after`, and optional expiry.

### Mega PR

Split by semantic artifact or by one implementation work item per PR.

## What good looks like

A new contributor or agent can arrive cold and answer from files alone:

```text
What are we doing?
Why?
What must be true?
What decision constrains it?
What PR lands next?
What command proves it?
What may we claim?
What must we not claim?
```

If the repo answers those questions without chat history, the method is working.
