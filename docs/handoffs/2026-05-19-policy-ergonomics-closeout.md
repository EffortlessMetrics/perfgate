# Policy Ergonomics and Team Rollout Closeout

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-19
Milestone: 0.20.0
Linked proposal: [`PERFGATE-PROP-0007-policy-ergonomics-team-rollout`](../proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md)
Linked specs: [`PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract`](../specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md), [`PERFGATE-SPEC-0012-agent-policy-change-guardrails`](../specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked plan: [`policy-ergonomics-team-rollout.md`](../../plans/0.20.0/policy-ergonomics-team-rollout.md)
Support/status impact: [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md), [`CANARY_MATRIX.md`](../status/CANARY_MATRIX.md), and [`PROOF_FRESHNESS.md`](../status/PROOF_FRESHNESS.md)
Proof commands: docs-check, doc-test, docs-source-check, product-claims-check, git diff --check

## Summary

This lane moved perfgate from evidence maturity into team policy rollout.
0.19 taught teams whether evidence was trustworthy; 0.20 teaches teams how to
promote that evidence deliberately without creating brittle gates.

The implemented path is:

```text
choose a policy profile
start advisory
inspect baseline and signal maturity
run policy doctor
emit a reviewable non-mutating patch
review the packet
surface policy posture in Action summaries
keep agents inside review-required guardrails
cite only fresh enough proof
```

The lane preserved the core product boundaries:

- local receipts remain the correctness contract;
- server ledger mode remains optional team history;
- policy profiles are suggestions, not behavior-changing defaults;
- maturity output remains advisory until team policy promotes it;
- policy patches are emitted for review and are not written automatically;
- agents are not policy authorities; and
- no public crates, receipt schemas, release aliases, GitHub Action inputs, or
  benchmark engines changed as part of the closeout.

## What Changed

Implemented surfaces:

- policy profile metadata for common repo shapes:
  `rust-cli-standard`, `rust-workspace-advisory`,
  `node-command-advisory`, `python-command-advisory`,
  `http-local-smoke`, `generic-command-advisory`, `agent-heavy-repo`, and
  `server-ledger-optional`;
- [`POLICY_ROLLOUT.md`](../POLICY_ROLLOUT.md), explaining advisory-to-blocking
  rollout, posture selection, promotion requirements, quarantine/retirement,
  reviewer rules, and server-ledger non-requirements;
- `perfgate policy doctor --config perfgate.toml` for advisory promotion
  readiness;
- `perfgate policy emit-patch --config perfgate.toml --bench <bench> --to <posture>`
  for reviewable non-mutating policy patches;
- `perfgate policy review-packet --config perfgate.toml --bench <bench>` for a
  compact reviewer and agent packet;
- GitHub Action posture summary examples and `action-check` fixture coverage;
- [`PERFGATE-SPEC-0012`](../specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md)
  for agent allowed/review-required/forbidden policy behavior;
- fixture-backed agent guardrail coverage for missing baseline, noisy signal,
  mature promotion candidate, regression, tradeoff candidate, and stale proof;
- product-claims proof freshness validation, including the rule that
  `stable`/`supported` claims cannot rely on `stale`, `superseded`, or
  `unproven` proof freshness;
- a post-0.20 canary rerun plan in [`CANARY_MATRIX.md`](../status/CANARY_MATRIX.md);
  and
- a real external non-Rust policy rollout canary:
  [`2026-05-19-policy-rollout-canary-droid-action.md`](../audits/2026-05-19-policy-rollout-canary-droid-action.md).

## What Teams Can Now Do

Teams can now answer:

- which profile matches this repo shape;
- whether a benchmark should stay smoke, advisory, gate-candidate, required,
  quarantined, or retired;
- why a promotion is or is not ready;
- what exact non-mutating config patch would be reviewed;
- what a reviewer needs to inspect before approving policy;
- what local command reproduces the evidence;
- why a noisy benchmark must stay advisory or use paired mode;
- what agents may inspect or propose; and
- which proof is current enough to cite.

## What Stayed Advisory

These surfaces remain advisory unless a team explicitly promotes policy:

- benchmark recipe and policy profile suggestions;
- baseline and signal maturity classifications;
- calibration patch output;
- `policy doctor` recommendations;
- `policy emit-patch` output;
- review packets;
- Action posture summaries; and
- agent guardrail recommendations.

No command in this lane silently promotes baselines, loosens thresholds, makes
a mature benchmark blocking, requires server ledger mode, or accepts a
tradeoff.

## Agent Boundaries

Agents may:

- inspect receipts and review packets;
- rerun local reproduction commands;
- summarize posture, maturity, signal, and proof freshness;
- recommend paired mode or more samples; and
- propose a reviewable policy patch.

Agents require explicit review to:

- promote baselines;
- make a gate blocking;
- loosen thresholds;
- accept tradeoffs;
- change policy profiles;
- quarantine or retire gates; or
- require server ledger mode.

Agents are forbidden by default from treating missing baselines as regressions,
noisy evidence as confirmed policy, stale proof as current support, or optional
ledger upload as local correctness.

## Proof Records

Durable lane artifacts:

- [`PERFGATE-PROP-0007-policy-ergonomics-team-rollout`](../proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md)
- [`PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract`](../specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md)
- [`PERFGATE-SPEC-0012-agent-policy-change-guardrails`](../specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md)
- [`policy-ergonomics-team-rollout.md`](../../plans/0.20.0/policy-ergonomics-team-rollout.md)
- [`POLICY_ROLLOUT.md`](../POLICY_ROLLOUT.md)
- [`CANARY_MATRIX.md`](../status/CANARY_MATRIX.md)
- [`PROOF_FRESHNESS.md`](../status/PROOF_FRESHNESS.md)
- [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md)
- [`2026-05-19-policy-rollout-canary-droid-action.md`](../audits/2026-05-19-policy-rollout-canary-droid-action.md)

Representative proof from the lane included focused CLI policy tests, help
snapshot tests, check guidance tests, `action-check`, schema compatibility,
product-claims validation, docs-source validation, and the external local
policy rollout canary.

Closeout proof:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## What Not To Infer

- This lane did not add a dashboard.
- This lane did not add a benchmark engine.
- This lane did not expand public crates.
- This lane did not change receipt schemas.
- This lane did not change release aliases.
- This lane did not change GitHub Action inputs.
- This lane did not make server ledger mode required.
- This lane did not auto-promote baselines.
- This lane did not auto-loosen thresholds.
- This lane did not make all mature benchmarks blocking.
- The policy rollout canary did not run hosted external Action policy posture.
- The policy rollout canary did not prove public 0.20 install behavior.
- The policy rollout canary did not prove probe-backed policy rollout.

## Remaining Work

Good follow-up lanes should start from real rollout pressure:

- run a hosted external policy posture canary from a real PR;
- prove a mature `gate_candidate` promotion in an external repo;
- run a probe-backed policy rollout canary with stable probe IDs;
- add team rollout templates only after profile usage shows repeated patterns;
- deepen production server-ledger operations proof separately; and
- keep proof freshness current when product claims are promoted.

## Active Goal Handling

This closeout archives `.codex/goals/active.toml` as
`.codex/goals/archive/perfgate-policy-ergonomics-team-rollout.toml` with status
`completed`.
