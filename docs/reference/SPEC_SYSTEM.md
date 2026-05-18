# Repo source-of-truth system

perfgate uses a linked source-of-truth stack. The rule is simple: do not make
every document do every job. Separate why, what, durable decision, how, what now,
and what proves it.

## Stack

```text
Roadmap
  -> Proposal / PRD
    -> Spec
      -> ADR where needed
        -> Implementation plan
          -> Active goal
            -> Issue / PR
              -> Proof commands
              -> CI receipts
              -> support-tier updates
              -> policy-ledger updates
```

## Artifact roles

| Artifact | Owns | Does not own |
|---|---|---|
| Roadmap | Release direction, milestones, lane framing | PR queue, live proof receipts |
| Proposal | Why, users, affected surfaces, alternatives, success criteria | Behavior contract, PR order |
| Spec | Required behavior, acceptance examples, proof, implementation mapping | Product rationale, PR sequence |
| ADR | Durable architecture or operating decision | Task list, current metric state |
| Plan | Work items, PR order, proof commands, rollback, blockers | Product rationale, durable decisions |
| Active goal | Current machine-readable execution state | Generated status, long prose, new behavior |
| Support tiers | Public claim support, proof pointers, limitations | Feature design, task sequencing |
| Policy ledgers | Exceptions, CI/policy intent, owner, reason, coverage, review date | Broad architecture or product strategy |
| Handoffs | Closeout context, remaining work, operator notes | Behavior contracts or policy truth |

## perfgate locations

| Question | Source of truth |
|---|---|
| Why are we doing this? | `docs/proposals/` |
| What must be true? | `docs/specs/` |
| What durable decision constrains the work? | `docs/adr/` and historical `docs/adrs/` |
| What PR lands next? | `plans/<milestone>/implementation-plan.md` or a linked work-item plan |
| What is the agent actively executing? | `.codex/goals/active.toml` |
| What proves product claims? | `docs/status/SUPPORT_TIERS.md`, `docs/status/PRODUCT_CLAIMS.md`, release/audit receipts, and CI |
| What exceptions exist? | `policy/*` ledgers |
| What remains after a lane closes? | `docs/handoffs/` |

`perfgate` intentionally uses `.codex/goals/` for agent execution state.
`.perfgate/` remains available for product-generated user artifacts from
`perfgate init` and related workflows.

## Rules

1. One kind of truth per artifact.
2. One semantic artifact per PR unless the linked plan says otherwise.
3. Proposals explain why; specs define behavior; ADRs record durable decisions.
4. Plans define sequencing, proof commands, rollback, and blockers.
5. Active goals tell agents what to do now and link to the current proposal,
   specs, ADRs, plans, status docs, and proof commands.
6. Generated status is updated by tools, not by hand.
7. Public claims require support-tier or product-claim proof pointers.
8. Policy exceptions require owner, reason, coverage, and review date.
9. Runtime/code PRs must link to the spec and plan item they implement.
10. If a linked source-of-truth artifact is missing or contradictory, stop and
    report instead of inventing a lane.

## Required headers

Each artifact-specific README defines its exact metadata block:

- proposals: `docs/proposals/README.md`
- specs: `docs/specs/README.md`
- ADRs: `docs/adr/README.md`
- plans: `plans/README.md`

When adding a new proposal/spec/ADR/plan, include the artifact's required
headers and use `n/a` for fields that truly do not apply.

## Agent workflow

Agents must:

1. Read `AGENTS.md`, `CLAUDE.md`, or other repo-level instructions.
2. Read this file.
3. Read `.codex/goals/active.toml`.
4. Read the linked implementation plan or work-item plan.
5. Read the linked proposal only for why.
6. Read the linked spec for acceptance and proof.
7. Read linked ADRs for durable constraints.
8. Inspect `git status --short` before editing.
9. Pick exactly one ready work item.
10. Implement only that item.
11. Run the proof commands from the plan item plus `git diff --check`.
12. Update status, receipts, policy ledgers, or handoffs only when the work item
    requires it.
13. Commit one focused change and open/update one focused PR.

## Stop conditions

Stop and report instead of guessing when:

- the active goal is missing or stale;
- linked files do not exist;
- no ready work item can be identified;
- proof commands cannot run and no substitute evidence is documented;
- generated status differs from committed status;
- unrelated staged changes exist;
- requested work conflicts with an ADR;
- a public claim lacks support-tier or product-claim proof; or
- a policy exception lacks owner, reason, coverage, and review date.

## Validation commands

Use the narrowest proof command that matches the work item. Source-of-truth
changes commonly use:

```bash
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

Broader changes may also require product-claim, policy, public-surface, schema,
action, clippy, test, or release proof commands listed in the selected plan.

## Active goal lifecycle

The current active manifest lives at:

```text
.codex/goals/active.toml
```

Archive completed or superseded manifests under:

```text
.codex/goals/archive/<lane>.toml
```

Do not leave multiple active manifests. Do not use active goal TOML as prose,
release notes, or a behavior contract.

## Closeout format

At the end of a lane, write a handoff or closeout that records:

- what shipped;
- proof commands and receipts;
- PRs and CI runs;
- generated status, support-tier, and policy updates;
- what did not ship;
- deferred work;
- claim boundaries; and
- the next recommended lane or work item.

Use `docs/handoffs/` for closeout context unless a plan-specific closeout file
is already defined.

## Common failure modes

### Spec becomes a task list

Move PR order to `plans/<milestone>/implementation-plan.md` or the linked
work-item plan. Keep the spec focused on behavior, examples, and proof.

### Plan becomes product rationale

Move why to the proposal. Keep the plan focused on work items, dependencies,
proof commands, and rollback.

### Active goal becomes prose

Keep TOML machine-readable. Link out to docs instead of embedding long tables or
new behavior.

### Agent hand-edits generated status

Run the generator/checker named in the plan. If no generator exists, document the
manual update requirement in the work item before editing status.

### Support claims drift

Require support-tier or product-claim proof pointers before strengthening README
or product documentation claims.

### Policy exceptions become silent debt

Every exception needs an owner, reason, `covered_by`, `created`, and
`review_after` field, plus an expiry when temporary.

### Mega PR

Split by semantic artifact or implementation work item. Do not mix proposal,
spec, ADR, plan, active goal, runtime, policy, and support-tier updates unless
the selected plan item explicitly requires that bundle.

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

If the repo answers those questions without chat history, the source-of-truth
system is working.
