# Repo source-of-truth system

perfgate uses a linked source-of-truth stack so humans and agents can answer
what truth lives where without reading chat history or treating stale prose as
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
|----------|------|--------------|
| Roadmap | Release direction, milestone framing, and lane selection | Detailed PR queue or proof receipts |
| Proposal | Why a lane exists, users, surfaces, alternatives, risks, and success criteria | Behavior contracts or detailed PR order |
| Spec | Required behavior, acceptance examples, proof, CI mapping, and claim boundaries | Product motivation or PR sequencing |
| ADR | Durable architecture and operating decisions | Temporary task lists or current metrics |
| Plan | PR order, work items, dependencies, proof commands, rollback, and handoff status | Product rationale or durable architecture decisions |
| Active goal | Current machine-readable agent objective, active work items, proof commands, and claim boundaries | Generated status, policy exceptions, or new behavior |
| Support tiers | Public claim support, limitations, proof, and next promotion requirements | Feature design or task sequencing |
| Policy ledgers | Exceptions, governed surfaces, owners, reasons, coverage, and review dates | Broad architecture narratives |

## Repository-specific locations

| Truth | Source of truth |
|-------|-----------------|
| Why are we doing this? | `docs/proposals/` |
| What must be true? | `docs/specs/` |
| What durable decision constrains the work? | `docs/adr/` and the historical `docs/adrs/` archive |
| What PR lands next? | `plans/<milestone>/implementation-plan.md` or a lane-specific plan |
| What is Codex actively executing? | `.codex/goals/active.toml` |
| What proves public claims? | `docs/status/SUPPORT_TIERS.md`, `docs/status/PRODUCT_CLAIMS.md`, receipts, and CI |
| What exceptions exist? | `policy/*.toml` and checked policy inventories |

perfgate intentionally uses `.codex/goals/` for agent execution state because
`.perfgate/` is reserved for product-generated user artifacts from
`perfgate init` and related workflows.

## Rules

1. One kind of truth per artifact.
2. One semantic artifact per PR unless the linked plan explicitly says
   otherwise.
3. Proposals explain why; specs define behavior; ADRs record durable decisions.
4. Plans sequence implementation and proof; they do not redefine specs.
5. Active goals tell agents what to do now; they do not introduce new behavior.
6. Generated status is updated by tools, not by hand.
7. Public claims require a support-tier row or an equivalent proof pointer.
8. Policy exceptions require owner, reason, coverage, creation date, and review
   date.
9. Runtime/code PRs must link to the spec and plan item they implement.
10. No claim is complete without a proof command or an explicit unavailable-proof
    note.

## Required metadata

Proposal, spec, ADR, and plan headers are defined in their directory READMEs:

- `docs/proposals/README.md`
- `docs/specs/README.md`
- `docs/adr/README.md`
- `plans/README.md`

Use `n/a` when a field is required but does not apply. Do not remove headers to
avoid making an impact statement.

## Agent workflow

Agents must:

1. Read `AGENTS.md` or tool-specific repo instructions.
2. Read this file.
3. Read `.codex/goals/active.toml`.
4. Read the linked implementation plan.
5. Read the linked proposal only for motivation.
6. Read the linked spec for acceptance and proof.
7. Read linked ADRs for constraints.
8. Inspect `git status --short` before changing files.
9. Pick exactly one ready work item.
10. Implement only that work item.
11. Run the proof commands listed by the plan or active goal.
12. Update status, policy ledgers, receipts, or handoffs only when the selected
    work item requires it.
13. Commit one focused change and open a PR.

If no ready work item exists, the correct result is a handoff or explicit
blocked report, not invented scope.

## Stop conditions

Stop and report instead of guessing when:

- the active goal is missing, stale, or contradictory;
- linked proposal, spec, ADR, plan, status, or policy files do not exist;
- the requested work conflicts with an ADR;
- the branch contains unrelated staged changes;
- generated status is dirty and the generator/checker is not part of the work
  item;
- proof commands cannot run and no substitute evidence is authorized;
- a public claim lacks support-tier proof;
- a policy exception lacks owner, reason, coverage, or review date.

## Active goal lifecycle

### Activate

Create or update:

```text
.codex/goals/active.toml
```

The manifest should include the lane ID, status, owner, linked proposal, linked
specs, linked ADRs, linked plan, objective, end state, work items, proof
commands, and claim boundaries.

### Pause

Use a paused active manifest when no lane is selected:

```toml
status = "paused"
reason = "No selected implementation lane."
```

### Archive

Move completed or superseded manifests to:

```text
.codex/goals/archive/YYYY-MM-DD-<lane>.toml
```

Then create the next active manifest. Do not leave multiple active goals.

## Closeout format

At the end of a lane, write a closeout under the lane plan directory when the
plan requires it:

```text
plans/<lane>/closeout.md
```

A closeout records what shipped, proof commands, receipts, PRs, CI runs,
generated status, support-tier updates, policy updates, deferred work, claim
boundaries, and the next lane recommendation.

## Common failure modes

### Spec becomes a task list

Move PR order to `plans/<lane>/implementation-plan.md`; keep the spec focused
on behavior, examples, acceptance, proof, and claim boundaries.

### Plan becomes product rationale

Move motivation to a proposal; keep the plan focused on work items, proof,
dependencies, rollback, and handoff.

### Active goal becomes prose

Keep the manifest TOML-shaped and link out to docs. Do not copy generated
status tables into it.

### Agent hand-edits generated status

Run the generator or checker named in the plan. If the command is unavailable,
record the unavailable proof honestly.

### Support claims drift

Require support/status impact headers and update `docs/status/SUPPORT_TIERS.md`
or `docs/status/PRODUCT_CLAIMS.md` when claims change.

### Policy exceptions become silent debt

Every exception must have an owner, reason, `covered_by`, `created`, and
`review_after`; temporary exceptions should also have an expiry or removal plan.

### Mega PR

Split into one semantic artifact or one implementation work item per PR unless
the active plan explicitly authorizes combining them.

## What good looks like

A new contributor or agent can arrive cold and answer these questions from repo
files alone:

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

When the repo answers those questions without chat history, the source-of-truth
system is working.
