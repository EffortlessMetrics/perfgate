# perfgate 0.19.0 Evidence Maturity and Adoption Intelligence Plan

Status: active
Owner: perfgate maintainers
Created: 2026-05-18
Milestone: 0.19.0
Current PR: benchmark recipe catalog
Linked proposal: [`PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence`](../../docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md)
Linked specs: [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../../docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md), [`PERFGATE-SPEC-0010-agent-repair-context-contract`](../../docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../../docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked policy: policy ledgers remain referenced by specs and status docs; no policy row changes in this plan PR
Support/status impact: product claims should be added or promoted after behavior and proof land, with proof freshness tiers where appropriate
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check
Blocks: evidence maturity implementation PRs
Blocked by:
Rollback: revert this plan and `.codex/goals/active.toml`; proposal and spec remain valid source-of-truth artifacts

## Goal

Make perfgate useful after week one. The release and first-use lanes made the
tool public, installable, credible, and guided. This lane makes repeated team
use safer by classifying evidence maturity:

```text
what to benchmark
whether the baseline is mature
whether the signal is noisy
whether the result should block
whether paired mode is needed
whether the change is a simple regression or tradeoff
what the reviewer should run locally
what an agent should fix first
when optional ledger history is worth recording
```

This plan sequences implementation for
[`PERFGATE-SPEC-0009-evidence-maturity-contract`](../../docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md).

## Activation Boundary

The 0.18 release cutover is complete and archived. `.codex/goals/active.toml`
now tracks this 0.19 evidence maturity lane.

This plan does not publish crates, move tags, change action aliases, expand the
public crate surface, or alter receipt schemas by default. Any schema or public
surface change requires an accepted spec and explicit proof.

## Operating Rules

- Keep one semantic artifact or narrow product delta per PR.
- Preserve the five public crates.
- Preserve CLI command names, receipt schemas, GitHub Action behavior, and
  release aliases unless an accepted spec says otherwise.
- Keep local receipts as the correctness contract.
- Keep server ledger mode optional team history.
- Keep benchmark selection reviewable, not magical.
- Do not silently promote baselines, loosen thresholds, or write policy.
- Treat maturity output as advisory unless a later accepted policy promotes it.
- Product claims must wait for behavior and proof.
- Proof freshness must not overstate old canaries.

## PR Sequence

| PR | Work item | Status | Files / surface |
|----|-----------|--------|-----------------|
| 498 | Evidence maturity proposal | merged | `docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md` |
| 499 | Evidence maturity contract spec | merged | `docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md` |
| 500 | Evidence maturity implementation plan | merged | `plans/0.19.0/evidence-maturity-adoption-intelligence.md`, `.codex/goals/active.toml` |
| 502 | Agent repair-context contract spec | merged | `docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md` |
| 503 | Benchmark recipe catalog | current | `perfgate init`, recipe metadata, CLI tests |
| 504 | Benchmark recipe guidance | pending | docs for recipes and anti-patterns |
| 505 | Baseline maturity doctor | pending | `perfgate baseline doctor`, CLI tests |
| 506 | Signal maturity doctor | pending | `perfgate doctor signal`, CLI tests |
| 507 | Calibration patch output | pending | `perfgate calibrate --emit-patch`, CLI tests |
| 508 | Decision example pack | pending | examples/fixtures and optional `decision examples` |
| 509 | Decision suggestion reasons | pending | `perfgate decision suggest`, CLI tests |
| 510 | Canary freshness matrix | pending | `docs/status/CANARY_MATRIX.md` |
| 511 | Server backup/restore smoke | pending | server/CLI tests |
| 512 | Server retention and migration policy | pending | server docs/status |
| 513 | Agent repair-context fixtures | pending | repair-context tests/fixtures |
| 514 | Proof freshness tiers and claims | pending | `docs/status/PRODUCT_CLAIMS.md`, support docs |
| 515 | Evidence maturity closeout | pending | handoff and goal archive |

## Work item: implementation-plan

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: agent-repair-context-contract, benchmark-recipe-catalog
Blocked by:

### Goal

Create the implementation sequence and active goal manifest for the 0.19
evidence maturity lane.

### Acceptance

- Plan links proposal, spec, ADRs, policy boundary, and proof commands.
- `.codex/goals/active.toml` points at this lane.
- No product behavior changes land in this PR.
- Product claims remain unchanged until proof exists.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert this plan and active goal manifest. Proposal and spec remain valid.

## Work item: agent-repair-context-contract

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked specs: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md; docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md
Blocks: agent-repair-context-fixtures
Blocked by: implementation-plan

### Goal

Define the agent-operable repair-context contract separately from the general
evidence maturity spec.

### Production delta

Add:

```text
docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md
```

The contract should cover:

```text
failure_class
artifact_paths
local_reproduction_command
baseline_promotion_guard
decision_suggestion
do_not_guidance
changed_files_summary
host_runtime_context
server_upload_status
```

### Non-goals

- Do not change repair-context schema in this spec PR.
- Do not make agent behavior a CI gate.
- Do not let agents promote baselines or loosen thresholds without explicit
  human action.

### Acceptance

- Spec defines what agents can rely on and what remains advisory.
- Spec maps fixture expectations for missing baseline, regression, high noise,
  host mismatch, decision candidate, and server upload failure.
- Spec preserves local receipts as correctness.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the spec PR. Evidence maturity spec remains valid without agent details.

## Work item: benchmark-recipe-catalog

Status: current
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: benchmark-recipe-guidance, product-claims
Blocked by: implementation-plan

### Goal

Add reviewable benchmark recipe metadata for common repo shapes.

### Production delta

Extend `perfgate init --suggest-benches` to support recipe metadata for:

```text
rust-cli-smoke
rust-workspace-advisory
node-command
python-command
http-smoke
generic-command
```

Each recipe should include:

```text
Best for
Bad for
Expected noise
Recommended mode
Advisory vs blocking
Paired-mode hint
```

### Non-goals

- Do not auto-promote baselines.
- Do not silently mark generated recipes as blocking policy.
- Do not infer every framework.

### Acceptance

- Generated suggestions are commented, conservative, and editable.
- Tests cover recipe metadata output.
- Existing first-use profiles continue to work.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert recipe metadata wiring and tests.

## Work item: benchmark-recipe-guidance

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks:
Blocked by: benchmark-recipe-catalog

### Goal

Explain recipe selection and common benchmark anti-patterns.

### Production delta

Add or update docs covering:

- compile-heavy first-hour gates;
- network-heavy checks without isolation;
- mixed correctness/performance tests;
- tiny runtimes;
- un-warmed workloads;
- mutable external services; and
- broad commands that do not imply a review action.

### Non-goals

- Do not add new benchmark engines.
- Do not claim recipes are universally safe to gate.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
git diff --check
```

## Work item: baseline-maturity-doctor

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: signal-maturity-doctor, product-claims
Blocked by: implementation-plan

### Goal

Add advisory baseline trust classification.

### Production delta

Add:

```bash
perfgate baseline doctor --config perfgate.toml
```

Classifications:

```text
missing
new
immature
mature
stale
host_mismatched
high_noise
```

### Non-goals

- Do not promote baselines automatically.
- Do not rewrite config.
- Do not change receipt schemas by default.

### Acceptance

- Output says whether each baseline is safe to gate, advisory only, needs more
  samples, needs host-compatible refresh, or should use paired mode.
- Tests cover missing, immature, mature, stale, host mismatch, and high-noise
  where fixtures exist.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Work item: signal-maturity-doctor

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: calibration-patch-output, product-claims
Blocked by: baseline-maturity-doctor

### Goal

Report signal maturity and gate/advisory recommendation.

### Production delta

Add:

```bash
perfgate doctor signal --config perfgate.toml
```

Output should include sample count, CV/noise evidence, host stability, baseline
age, recent drift, and recommendation:

```text
safe_to_gate
advisory_only
increase_samples
use_paired_mode
refresh_baseline
check_host_mismatch
no_decision_yet
```

### Non-goals

- Do not treat noisy evidence as regression.
- Do not imply a bad workload can be fixed only by automation.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Work item: calibration-patch-output

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: product-claims
Blocked by: signal-maturity-doctor

### Goal

Make calibration advice copy-ready without writing config.

### Production delta

Add:

```bash
perfgate calibrate --config perfgate.toml --bench parser --emit-patch
```

Output should include a TOML block or patch fragment, reasons, evidence used,
and when not to apply the suggestion.

### Non-goals

- No `--write` behavior.
- No policy mutation.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features calibrate
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Work item: decision-example-pack

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: decision-suggestion-reasons
Blocked by: implementation-plan

### Goal

Teach structured-decision patterns through examples and fixtures.

### Production delta

Add docs/fixtures for:

```text
latency regression with throughput improvement
memory regression with runtime improvement
startup slower but steady-state faster
probe regression with dominant workload improvement
noise too high for a decision
```

An optional later CLI surface may expose:

```bash
perfgate decision examples
```

### Non-goals

- Do not make structured decisions mandatory for local gates.
- Do not treat noise as an accepted tradeoff.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: decision-suggestion-reasons

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: product-claims
Blocked by: decision-example-pack

### Goal

Make `decision suggest` explain why it recommends simple gate, paired mode,
structured decision, no decision yet, or optional ledger history.

### Production delta

Reason output should name metric movement, direction, threshold result, noise
result, artifacts, probe/scenario/tradeoff evidence when present, missing
evidence, and next command.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: canary-freshness-matrix

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: proof-freshness-claims
Blocked by: implementation-plan

### Goal

Make canary proof durable and freshness-aware.

### Production delta

Add:

```text
docs/status/CANARY_MATRIX.md
```

Fields:

```text
canary
repo shape
last run
proof artifact
what it proves
what it does not prove
freshness
```

### Non-goals

- Do not rerun every canary in this docs PR.
- Do not make canaries mandatory CI.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## Work item: server-backup-restore-smoke

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: server-retention-migration-policy, product-claims
Blocked by: implementation-plan

### Goal

Make optional ledger operations more production-boring.

### Production delta

Add backup/restore smoke coverage for a supported store path. Prove export,
restore into a fresh store, latest/history/audit equivalence, key rotation
compatibility where applicable, and prune dry-run preservation.

### Non-goals

- Do not make server mode required.
- Do not change local decision correctness.
- Do not broaden auth scopes beyond the smoke need.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-server --all-features
cargo +1.95.0 test -p perfgate-cli --all-features server
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: server-retention-migration-policy

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: product-claims
Blocked by: server-backup-restore-smoke

### Goal

Document retention and migration expectations for optional ledger mode.

### Production delta

Cover retention windows, export/restore expectations, migration compatibility,
audit records, prune safety, and recovery behavior.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
git diff --check
```

## Work item: agent-repair-context-fixtures

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked specs: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md; docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md
Blocks: product-claims
Blocked by: agent-repair-context-contract

### Goal

Back the agent repair-context contract with fixtures.

### Production delta

Cover:

```text
missing baseline
regression
high noise
host mismatch
decision candidate
server upload failure
```

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: proof-freshness-claims

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md
Blocks: final-closeout
Blocked by: canary-freshness-matrix, benchmark-recipe-catalog, baseline-maturity-doctor, signal-maturity-doctor, calibration-patch-output, decision-suggestion-reasons, server-backup-restore-smoke, agent-repair-context-fixtures

### Goal

Map evidence maturity promises to support tiers and proof freshness without
overstating canaries or advisory surfaces.

### Production delta

Update product claims only for implemented and proven behavior. Add freshness
language:

```text
current
recent
stale
superseded
unproven
```

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Work item: final-closeout

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked specs: docs/specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md; docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md
Blocks:
Blocked by: proof-freshness-claims

### Goal

Close the evidence maturity lane with durable proof and non-inferences.

### Acceptance

- Handoff records covered maturity states.
- It records which outputs remain advisory.
- It records canary freshness and remaining unproven surfaces.
- It archives `.codex/goals/active.toml`.
- It names the next recommended lane.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the closeout handoff and goal archive.
