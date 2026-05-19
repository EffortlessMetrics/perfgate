# PERFGATE-SPEC-0012: Agent policy-change guardrails

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-19
Milestone: 0.20.0
Behavior version: agent-policy-change-guardrails.v1
Product surface: policy doctor, policy patch output, review packets, GitHub Action summaries, repair context, decision suggestions, agent review workflows
CI surface: docs-source-check, product-claims-check, doc-test, focused policy/check fixtures, action-check if summary behavior changes, schema-compat if receipt shape changes
Schema impact: no receipt schema change in this spec; future policy-specific receipt fields require schema-compat and migration proof
Action impact: no action input, alias, or exit-code behavior change by default; Action summaries may point agents to review-required policy posture
Server impact: server ledger remains optional team history and must not be required by agents for local correctness or policy promotion
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked specs: PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract, PERFGATE-SPEC-0010-agent-repair-context-contract
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions, PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional
Linked plan: plans/0.20.0/policy-ergonomics-team-rollout.md
Linked policy: policy ledgers remain source of truth for governed exceptions, public surface, workflow policy, generated files, and release proof
Support/status impact: product claims should promote agent policy-change support only after fixture-backed proof covers review-required and forbidden cases
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

perfgate now reports evidence maturity and policy posture. Agents can read
repair context, review packets, policy doctor output, Action summaries, and
policy patch suggestions. That makes them useful for routine review prep:
rerunning a failed check, finding the right artifact, summarizing noisy signal,
or opening a policy patch for review.

The same access can weaken the evidence contract if agents treat policy as an
automation target instead of a review surface:

```text
loosen thresholds to make CI green
promote a baseline because setup is missing
make a mature advisory benchmark blocking without approval
accept a tradeoff because a bundle exists
require server ledger mode because upload was configured
quarantine or retire a gate without reviewer intent
```

This spec defines what agents may do, what requires explicit human review, and
what remains forbidden by default when perfgate policy is involved.

## Behavior

Agents MUST treat policy ergonomics output as review guidance, not authority.
The safe operating model is:

```text
inspect evidence
reproduce locally
summarize posture
suggest next command
propose reviewable patch
wait for human approval before changing policy
```

Agents MAY prepare review artifacts and patches. They MUST NOT make policy
weaker, broader, or more mandatory without explicit human approval. They MUST
NOT convert advisory maturity output into blocking behavior by default.

This spec extends the advisory-to-blocking promotion contract. It does not
replace local receipts, decision bundles, product claims, or policy ledgers as
sources of truth.

## Agent capability classes

Agent actions fall into three classes.

| Class | Meaning | Required handling |
|-------|---------|-------------------|
| `allowed` | Useful inspection, reproduction, explanation, or non-mutating proposal work. | Agent may perform after ordinary user authorization for repository work. |
| `review_required` | Policy-affecting work that may be correct but changes enforcement, trust posture, or team history. | Agent may draft or propose, but human review must approve before merge or execution. |
| `forbidden_by_default` | Unsafe shortcut or authority escalation that weakens evidence or changes correctness boundaries. | Agent must not perform unless a later accepted spec and explicit user approval narrow the prohibition. |

User-facing output MAY use friendlier wording, but it MUST preserve these
meanings.

## Allowed agent actions

Agents MAY:

- run local perfgate commands requested by the user or named by receipts;
- rerun checks, doctors, calibration, decision suggestion, or report commands;
- inspect `run.json`, `compare.json`, `report.json`, `comment.md`,
  `repair_context.json`, review packets, decision artifacts, and policy doctor
  output;
- summarize benchmark posture, baseline maturity, signal maturity, proof
  freshness, and decision readiness;
- suggest paired mode when noise or host sensitivity makes ordinary comparison
  weak;
- propose a non-mutating config or policy patch with reasons;
- open a PR that labels policy changes as review-required;
- point reviewers to local reproduction commands and artifacts;
- mark missing optional evidence as unknown; and
- report optional server ledger status separately from local receipt
  correctness.

Allowed actions MUST preserve workload intent and existing review boundaries.
If an agent cannot identify the benchmark, baseline, host, or artifact context,
it SHOULD stop at a summary and ask for review instead of inventing missing
evidence.

## Review-required actions

Agents MAY draft or recommend these actions, but MUST require explicit human
review before they are applied, merged, or executed:

- promote a baseline;
- make a benchmark blocking or move it to `required_gate`;
- move a benchmark from `advisory` to `gate_candidate`;
- loosen fail, warn, noise, repeat, or paired-mode policy;
- tighten policy in a way that will block developers;
- accept a tradeoff or mark a structured decision as approved;
- change a policy profile or apply a different repo-shape profile;
- quarantine, unquarantine, retire, or restore a gate;
- change benchmark command intent or workload scope;
- change proof freshness status used by product claims;
- change hosted Action behavior or required-baseline mode;
- change release, tag, alias, or public install guidance;
- require server ledger mode; or
- change ledger retention, prune, backup, restore, or key-rotation policy.

Review-required output SHOULD include:

```text
current state
proposed state
evidence used
artifact paths
local reproduction command
policy patch preview
reason for human review
what this does not prove
rollback or demotion path
```

## Forbidden-by-default actions

Agents MUST NOT:

- promote a baseline solely because one is missing;
- loosen thresholds to make a regression pass;
- delete artifacts, baselines, decision bundles, repair context, or audit
  evidence to make a check green;
- treat high-noise evidence as a confirmed regression or confirmed improvement;
- compare host-mismatched evidence as if hosts were compatible;
- make all mature benchmarks blocking by default;
- make server ledger upload availability part of local correctness by default;
- accept a tradeoff without decision evidence and reviewer approval;
- rewrite benchmark commands in a way that changes workload intent without
  review;
- change public crate surface, receipt schemas, release aliases, or GitHub
  Action inputs as a policy repair shortcut; or
- infer absent probes, scenarios, tradeoff policy, canaries, or proof freshness
  records.

If the user explicitly asks for a forbidden-by-default action, the agent SHOULD
surface the risk and require a narrow, explicit instruction before proceeding.
Some actions may still need a separate accepted spec before implementation.

## Policy-change workflow

For policy-affecting work, agents SHOULD follow this sequence:

1. Inspect the receipts and review packet.
2. Run or cite the local reproduction command.
3. Run `perfgate policy doctor --config perfgate.toml` for posture.
4. If a policy change looks appropriate, run or propose
   `perfgate policy emit-patch --config perfgate.toml --bench <bench> --to <state>`.
5. Summarize why the change is allowed, review-required, or blocked.
6. Open or update a PR that makes the review boundary visible.
7. Wait for human review before applying review-required policy.

Agents SHOULD prefer demotion or quarantine suggestions when evidence becomes
untrustworthy. Demotion still requires review when it changes enforcement or
team policy.

## Scenario guardrails

### Missing baseline

Missing baseline means setup is incomplete. Agents MAY run the first check,
inspect run/report artifacts, and propose a reviewed promotion command. Agents
MUST NOT promote the baseline blindly or loosen thresholds to hide the missing
setup.

### Noisy signal

High noise means evidence is not trustworthy enough for a simple gate. Agents
MAY recommend more samples, advisory posture, paired mode, or calibration
review. Agents MUST NOT call noisy evidence a confirmed regression or promote
it to blocking policy by default.

### Mature promotion candidate

A mature advisory benchmark may become a `gate_candidate`. Agents MAY emit a
reviewable patch and reason section. Agents MUST NOT move it to
`required_gate` without reviewer approval.

### Regression

Agents MAY reproduce locally, inspect compare/report artifacts, and propose a
fix or decision path. Agents MUST NOT update the baseline, loosen thresholds,
or retire the gate merely to make CI green.

### Tradeoff candidate

Agents MAY point to decision suggestions, scenario/tradeoff evidence, and
decision-bundle commands. Agents MUST NOT accept bounded regressions, approve a
decision, or record team history without review.

### Stale proof

Stale proof can inform investigation but cannot support promotion alone.
Agents MAY recommend refreshing proof or rerunning a canary. Agents MUST NOT
cite stale proof as current support for blocking policy or product claims.

### Optional server ledger

Agents MAY inspect ledger readiness, export, restore, prune dry-run, audit, and
upload status. Agents MUST keep local receipts as correctness unless explicit
team policy says otherwise. Requiring ledger mode is review-required and not a
default repair.

## Required agent-facing surfaces

Policy-related agent surfaces SHOULD carry enough information for safe routing:

| Surface | Required agent meaning |
|---------|------------------------|
| repair context | failure class, artifact paths, reproduction or inspection command, and do-not guidance |
| policy doctor | current posture, recommended posture, missing requirements, and next command |
| policy patch output | current/proposed state, reasons, evidence, non-inferences, and rollback guidance |
| review packet | verdict, maturity, signal, calibration, decision, proof freshness, artifacts, and do-not guidance |
| Action summary | advisory versus blocking posture, local reproduction, review-required state, and artifacts |
| decision suggestion | simple gate, paired mode, structured decision, or no decision yet with reasons |
| product claims | support tier and proof freshness, without stale proof overpromotion |

No single surface must duplicate every field. Agents SHOULD combine linked
receipts and summaries, and MUST report unknown when evidence is absent.

## Proof freshness

Agents MUST respect proof freshness tiers when proposing policy changes:

- `current` proof may support current policy recommendations.
- `recent` proof may support bounded recommendations with explicit limits.
- `stale` proof may trigger investigation or refresh but MUST NOT support
  promotion alone.
- `superseded` proof MUST point to the newer evidence.
- `unproven` gaps MUST remain visible.

Agents SHOULD not close freshness gaps by editing product claims. They should
rerun proof, update the canary matrix, or mark the claim appropriately.

## Non-goals

- Do not make agents policy authorities.
- Do not add a dashboard or agent service.
- Do not add another benchmark engine.
- Do not expand public crates.
- Do not require server ledger mode.
- Do not auto-promote baselines.
- Do not auto-loosen thresholds.
- Do not auto-accept tradeoffs.
- Do not write policy by default.
- Do not change receipt schemas, CLI command names, GitHub Action inputs,
  release tags, release aliases, or public install guidance in this spec.

## Required evidence

Documentation-only changes to this spec SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Fixture or behavior changes SHOULD run focused proof:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

Action summary changes SHOULD also run:

```bash
cargo +1.95.0 run -p xtask -- action-check
```

## Acceptance examples

| Example | Result |
|---------|--------|
| Agent reruns `perfgate check` and links `repair_context.json` plus the local reproduction command. | Pass |
| Agent summarizes a noisy signal and proposes paired mode without changing config. | Pass |
| Agent emits a `gate_candidate` patch and labels it review-required. | Pass |
| Agent opens a PR that makes a mature advisory benchmark blocking only after the patch is visible for review. | Pass |
| Agent recommends refreshing stale proof before promoting a policy claim. | Pass |
| Agent reports optional ledger upload failure without invalidating local receipts. | Pass |
| Agent promotes a missing baseline to make CI green. | Fail |
| Agent loosens thresholds because a regression failed. | Fail |
| Agent turns all mature benchmarks into required gates by default. | Fail |
| Agent accepts a tradeoff without decision evidence and reviewer approval. | Fail |
| Agent requires server ledger mode because ledger upload is configured. | Fail |
| Agent cites stale canary proof as current support for a required gate. | Fail |

## Fixture requirements

The implementation lane MUST add policy guardrail fixtures for:

```text
missing baseline
noisy signal
mature promotion candidate
regression
tradeoff candidate
stale proof
```

Each fixture SHOULD assert:

- the agent-facing surface identifies allowed versus review-required action;
- unsafe shortcuts are named in do-not guidance;
- local receipts remain the correctness contract;
- policy patches are non-mutating unless explicitly reviewed;
- proof freshness is preserved when relevant;
- optional ledger status does not become local correctness; and
- agents can determine the safe next action without raw benchmark logs.

If a fixture needs new fields, it SHOULD first prove the behavior through
existing artifacts and then propose any schema addition separately.

## Test mapping

Current or planned proof maps to:

- CLI policy tests for promotion doctor and patch output boundaries;
- CLI check fixtures for missing baseline, noisy signal, and regression;
- CLI decision fixtures for tradeoff candidates;
- product-claims-check for freshness discipline;
- action-check fixtures for Action summary posture and review-required copy;
- repair-context fixtures for failure class and do-not guidance; and
- schema-compat if `repair_context` or another receipt shape changes.

## Implementation mapping

The agent policy-change guardrails are owned by:

- `docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md`
  for rollout state and promotion vocabulary;
- this spec for agent policy authority boundaries;
- `docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md` for
  failure-to-repair artifact contracts;
- `crates/perfgate-cli` policy, check, decision, and repair-context modules for
  behavior;
- `action.yml` and `xtask action-check` for hosted summary guardrails;
- `docs/status/PROOF_FRESHNESS.md` and `docs/status/PRODUCT_CLAIMS.md` for
  freshness and support mapping; and
- `docs/status/CANARY_MATRIX.md` for external proof context.

## Promotion rule

This spec is accepted when merged as the agent policy-change guardrail
contract. It is implemented when:

- policy guardrail fixtures cover missing baseline, noisy signal, mature
  promotion candidate, regression, tradeoff candidate, and stale proof;
- agent-facing output clearly marks allowed, review-required, and
  forbidden-by-default policy actions;
- product claims cite only fixture-backed policy guardrail support; and
- the 0.20 closeout records what agents may do, what requires review, what
  remains forbidden, and what remains unproven.
