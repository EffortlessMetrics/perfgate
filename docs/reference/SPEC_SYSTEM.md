# Repo source-of-truth system

perfgate uses a linked source-of-truth stack so humans and agents can find the
right owner for each kind of truth without scraping chat history or treating old
plans as current behavior.

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
|---|---|---|
| Roadmap | Release direction and milestone framing | PR queue or generated metrics |
| Proposal | Why, users, alternatives, success criteria | Behavior contract or detailed PR order |
| Spec | Behavior, acceptance, proof, non-goals | PR sequence or active queue |
| ADR | Durable decisions and consequences | Task list or current metric state |
| Plan | PR order, work items, proof commands, rollback | Product rationale or durable architecture |
| Active goal | Current machine-readable work and claim boundaries | Generated status or new product behavior |
| Support tiers | Public claim proof and limitations | Feature design |
| Policy ledgers | Exceptions and CI/policy receipts | Broad architecture |
| Handoffs | Closeout context and remaining work | Behavior contracts |

## Sources of truth

| Question | Source |
|---|---|
| Why are we doing this? | `docs/proposals/` |
| What must be true? | `docs/specs/` |
| What architecture decision constrains it? | `docs/adr/` and historical `docs/adrs/` |
| What PR lands next? | `plans/<milestone>/implementation-plan.md` or a lane plan |
| What is the agent actively executing? | `.codex/goals/active.toml` |
| What proves a public claim? | `docs/status/SUPPORT_TIERS.md`, `docs/status/PRODUCT_CLAIMS.md`, receipts, and CI |
| What exceptions exist? | `policy/*.toml` and policy text ledgers |

## Rules

1. One kind of truth per artifact.
2. One semantic artifact per PR unless the selected plan item says otherwise.
3. Specs define behavior; plans define sequencing.
4. Proposals explain why; ADRs record durable decisions.
5. Active goals tell agents what to do now.
6. Generated status is updated by tools, not by hand.
7. Public claims require support-tier or product-claim proof.
8. Policy exceptions require an owner, reason, coverage, and review date when the ledger format supports them.
9. Runtime/code PRs must link to the spec and plan item they implement.
10. Do not broaden support claims without updating the proof surface that owns those claims.

## Required headers

Proposal, spec, ADR, and plan artifacts should use the required header set
listed in their local README files:

- `docs/proposals/README.md`
- `docs/specs/README.md`
- `docs/adr/README.md`
- `plans/README.md`

Use `n/a` when a field is not applicable. Existing historical artifacts may use
legacy headers until a selected work item migrates them.

## Agent workflow

Agents must:

1. Read `AGENTS.md` or `CLAUDE.md`.
2. Read this file.
3. Read `.codex/goals/active.toml`.
4. Read the linked implementation plan or lane plan.
5. Read the linked proposal only for motivation.
6. Read the linked spec for acceptance and proof.
7. Read linked ADRs for durable constraints.
8. Inspect `git status --short` for unrelated work.
9. Pick exactly one ready work item, unless the user explicitly requested a source-of-truth rail change.
10. Implement only that item.
11. Run the proof commands named by the selected plan item, plus `git diff --check`.
12. Update receipts, status, support tiers, or policy ledgers only when the selected work item requires it.
13. Stop and report rather than inventing a lane or broadening scope.

## Stop conditions

Stop and report instead of guessing when:

- the active goal is missing or stale;
- linked files do not exist;
- the selected plan item is missing proof commands;
- proof cannot run and no substitute evidence is available;
- unrelated staged changes exist;
- generated status differs from committed status;
- requested work conflicts with an ADR;
- a public claim lacks support-tier or product-claim proof;
- the requested work requires a new proposal, spec, ADR, or lane and the user did not ask for that rail to be created.

## Active goal lifecycle

### Activate

Use exactly one active manifest:

```text
.codex/goals/active.toml
```

Set:

```toml
status = "active"
```

### Pause

Use a paused manifest only when there is deliberately no selected implementation
lane:

```toml
status = "paused"
reason = "No selected implementation lane."
```

### Archive

Move old active goals to:

```text
.codex/goals/archive/<lane>.toml
```

Then create or update the new active manifest. Do not leave multiple active
goals.

## Closeout format

At the end of a lane, write a handoff or closeout that answers:

- what shipped;
- proof commands and receipts;
- PRs and CI runs;
- generated status updates;
- support-tier or policy updates;
- what did not ship;
- deferred work;
- claim boundaries; and
- the next lane recommendation.

Use `docs/handoffs/` for closeout records unless a lane plan explicitly names a
`plans/<lane>/closeout.md` file.

## Common failure modes

### Failure: spec becomes a task list

Fix: move PR order to `plans/<lane>/implementation-plan.md` and keep the spec to
behavior, examples, non-goals, and proof.

### Failure: plan becomes product rationale

Fix: move “why” to the proposal and keep the plan focused on work items,
dependencies, proof, and rollback.

### Failure: active goal becomes prose

Fix: keep `.codex/goals/active.toml` machine-readable and link out to longer
docs.

### Failure: agent hand-edits generated status

Fix: run the named generator/checker and record unavailable proof honestly.

### Failure: support claims drift

Fix: require support/status impact in source artifacts and update the status doc
that owns the claim.

### Failure: policy exceptions become silent debt

Fix: every exception gets an owner, reason, coverage, and review date when the
ledger supports those fields.

### Failure: mega PR

Fix: one semantic artifact or one implementation work item per PR unless the
selected plan explicitly widens scope.

## What good looks like

A new contributor or agent can arrive cold and answer:

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

If the repo answers those questions without chat history, the source-of-truth
system is working.
