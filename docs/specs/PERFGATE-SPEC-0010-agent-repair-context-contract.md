# PERFGATE-SPEC-0010: Agent repair-context contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-18
Milestone: 0.19.0
Behavior version: agent-repair-context-contract.v1
Product surface: repair_context.json, check guidance, action summaries, decision suggestions, agent repair workflows
CI surface: docs-source-check, product-claims-check, doc-test, focused repair-context fixtures, schema-compat if receipt shape changes
Schema impact: no repair-context schema change in this spec; future schema additions require schema-compat and explicit migration proof
Action impact: action summaries may point agents to repair_context.json but must preserve local reproduction commands
Server impact: optional ledger upload status may be reported, but local receipts remain the correctness contract
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked ADRs: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Linked plan: plans/0.19.0/evidence-maturity-adoption-intelligence.md
Linked policy: policy ledgers remain source of truth for governed exceptions, public surface, workflow policy, and release proof
Support/status impact: product claims should reference this contract only after repair-context fixtures prove the covered scenarios
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

perfgate emits `repair_context.json` so humans and automation can move from a
failed or warned check to local reproduction and repair. The first-use lane made
the artifact visible; the evidence maturity lane needs a stricter contract for
agents.

Without this contract, agents may infer unsafe actions from logs:

```text
loosen thresholds to silence a failure
promote a baseline to fix missing setup
treat noisy evidence as a regression
ignore host mismatch
assume optional server upload failure invalidates local receipts
```

This spec defines what agents can rely on, what remains advisory, and which
fixture cases must back the contract before product claims promote it.

## Behavior

Agent repair context MUST preserve the same explanation shape used by first-use
UX:

```text
what happened
what it means
what artifact proves it
what command reproduces it
what not to do
```

The contract is advisory by default. It MUST guide agents toward safe local
repair and review actions, but it MUST NOT let agents automatically promote
baselines, loosen thresholds, rewrite policy, make server mode required, or
accept tradeoffs without human-reviewable receipts.

## Current receipt fields

The current `perfgate.repair_context.v1` receipt gives agents these stable
inputs:

| Field | Agent meaning |
|-------|---------------|
| `schema` | identifies the repair-context receipt version |
| `benchmark` | names the benchmark or check target |
| `verdict` | carries pass/warn/fail/skip counts and reasons |
| `status` | quick routing status for pass, warn, fail, or skip |
| `breached_metrics` | metric-level movement that crossed warn/fail/skip policy |
| `compare_receipt_path` | optional path to the comparison receipt |
| `report_path` | path to the machine-readable report |
| `profile_path` | optional profile or auxiliary artifact path |
| `git` | optional branch and commit metadata |
| `changed_files` | optional changed-file summary grouped by top-level path |
| `otel_span` | optional trace/span identifiers |
| `recommended_next_commands` | human-reviewable local commands or inspection steps |

Agents MUST tolerate missing optional fields. Absence of optional Git,
comparison, profile, span, or changed-file data is not permission to invent it.

## Required semantic contract

Repair-context producers SHOULD provide or preserve enough information for an
agent to derive these routing concepts:

| Concept | Required behavior |
|---------|-------------------|
| `failure_class` | classify setup, missing baseline, regression, high noise, host mismatch, review required, or server upload failure when evidence supports it |
| `artifact_paths` | name the receipts and review artifacts relevant to the failure |
| `local_reproduction_command` | include or point to one copyable local reproduction command where available |
| `baseline_promotion_guard` | distinguish missing-baseline setup from performance regression and warn against blind promotion |
| `decision_suggestion` | say whether a simple gate, paired mode, structured decision, or no decision yet is appropriate when evidence exists |
| `do_not_guidance` | name unsafe shortcuts such as loosening thresholds or making server upload part of local correctness |
| `changed_files_summary` | provide bounded file context when Git data is available |
| `host_runtime_context` | preserve host or runtime context when available through linked receipts |
| `server_upload_status` | report optional upload failures without invalidating local receipts by default |

The current schema does not need to expose every concept as a top-level field.
Agents may combine `repair_context.json`, `report.json`, `compare.json`,
`comment.md`, action summary text, and check guidance to get the full contract.
Future schema fields MAY make these concepts explicit, but such changes require
schema compatibility proof.

## Failure-class expectations

Repair context MUST NOT collapse all non-pass outcomes into "regression."
Covered classes have distinct safe actions:

| Class | Agent action | Guardrail |
|-------|--------------|-----------|
| missing baseline | inspect the first run and ask for reviewed promotion | do not loosen thresholds or promote blindly |
| performance regression | reproduce locally, inspect compare/report artifacts, then fix or create decision evidence | do not update baseline to hide the regression |
| high noise | rerun, increase samples, keep advisory, or use paired mode | do not call noisy evidence a confirmed regression |
| host mismatch | rerun on compatible host or intentionally refresh matching baseline | do not compare incompatible hosts as if equivalent |
| review required | inspect decision artifacts and policy reasons | do not auto-accept bounded regressions |
| server upload failed | preserve local receipt verdict and report upload failure separately | do not make optional ledger availability local correctness |
| setup command failed | inspect benchmark command output and repository setup | do not treat command setup as performance evidence |

## Agent operating rules

Agents consuming repair context MUST:

- prefer receipt paths over raw log snippets;
- preserve local reproduction commands exactly unless redaction is required;
- inspect `compare.json` or `report.json` before changing performance policy;
- treat `recommended_next_commands` as reviewable suggestions, not commands to
  run blindly;
- report missing optional fields as unknown, not failed;
- keep server ledger mode optional unless user policy explicitly says otherwise;
- ask for human approval before promotion, threshold loosening, or policy edits;
  and
- keep structured decisions pull-based, using them only when tradeoff evidence
  exists or the output recommends them.

Agents consuming repair context MUST NOT:

- promote a baseline solely because one is missing;
- loosen fail/warn/noise thresholds to make a check pass;
- rewrite benchmark commands without preserving the workload intent;
- treat throughput, latency, memory, or probe movement without metric direction;
- convert optional ledger upload failure into local gate failure by default; or
- infer absent artifacts, probes, scenarios, or tradeoff policy.

## Fixture requirements

The implementation lane MUST add or update fixtures for:

```text
missing baseline
performance regression
high noise
host mismatch
decision candidate
server upload failure
setup command failed
```

Each fixture SHOULD assert:

- the repair context is valid JSON;
- the schema remains `perfgate.repair_context.v1` unless explicitly changed;
- the relevant artifact paths are present or intentionally absent;
- at least one local reproduction or inspection command is available where the
  scenario supports it;
- unsafe "do not" guidance is present in the paired CLI/action surface;
- optional server status does not invalidate local receipts by default; and
- agents can determine the safe next action without reading benchmark logs.

If a fixture needs fields not present in `perfgate.repair_context.v1`, it SHOULD
first prove the behavior through surrounding artifacts and then propose any
schema addition separately.

## Non-goals

- Do not change the repair-context schema in this spec PR.
- Do not make agents a CI policy authority.
- Do not allow agents to auto-promote baselines or loosen thresholds.
- Do not make server ledger upload part of local correctness.
- Do not require structured decisions for ordinary local gates.
- Do not build a dashboard or agent service.
- Do not expand public crates.

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
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

Cross-surface changes that affect action summaries SHOULD also run:

```bash
cargo +1.95.0 run -p xtask -- action-check
```

## Acceptance examples

| Example | Result |
|---------|--------|
| Missing-baseline repair context points to run/report artifacts and says setup is incomplete. | Pass |
| Regression repair context includes breached metrics and compare/report paths. | Pass |
| High-noise guidance recommends rerun, more samples, advisory mode, or paired mode without calling it confirmed regression. | Pass |
| Host-mismatch guidance tells the agent to rerun on compatible hosts or refresh intentionally. | Pass |
| Decision-candidate guidance points to decision artifacts or next commands when metric movement shows a tradeoff. | Pass |
| Server upload failure is reported separately from the local verdict. | Pass |
| Changed-file context is absent because Git is unavailable, and the agent treats it as unknown. | Pass |
| An agent promotes a baseline to fix missing setup without human review. | Fail |
| An agent loosens thresholds because a regression failed CI. | Fail |
| Optional ledger upload failure makes local receipts invalid by default. | Fail |
| A missing optional artifact is treated as proof that the benchmark passed. | Fail |

## Test mapping

Current or planned proof maps to:

- existing `repair_context` helper tests for Git and changed-file parsing;
- CLI check fixtures for missing baseline, regression, noise, and host mismatch;
- CLI decision fixtures for decision-candidate guidance;
- action-check fixtures for summary-to-repair-context handoff;
- server or CLI/server tests for optional upload-failure wording; and
- schema-compat if `perfgate.repair_context.v1` changes.

## Implementation mapping

The agent repair-context contract is owned by:

- `crates/perfgate-types/src/repair_context.rs` for receipt fields and schema;
- `crates/perfgate-cli/src/repair_context.rs` for receipt construction;
- `crates/perfgate-cli/src/check_guidance.rs` for failure taxonomy and "do not"
  guidance;
- `crates/perfgate-cli/src/decision_suggest.rs` for decision readiness output;
- GitHub Action summary generation and `xtask action-check` for hosted review
  ergonomics;
- `schemas/perfgate.repair_context.v1.schema.json` for generated schema proof;
  and
- `docs/status/PRODUCT_CLAIMS.md` after fixture-backed proof exists.

## Promotion rule

This spec is accepted when merged as the agent repair-context behavior
contract. It is implemented when:

- fixture coverage exists for missing baseline, regression, high noise, host
  mismatch, decision candidate, server upload failure, and setup command
  failure;
- repair context and paired CLI/action surfaces expose safe next actions and
  guardrails for those scenarios;
- schema-compat passes if the receipt shape changes; and
- product claims map agent-operable repair context only to proven scenarios.
