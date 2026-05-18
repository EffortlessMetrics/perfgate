# Evidence Maturity and Adoption Intelligence Closeout

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-18
Milestone: 0.19.0
Linked proposal: [`PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence`](../proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md)
Linked specs: [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md), [`PERFGATE-SPEC-0010-agent-repair-context-contract`](../specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked plan: [`evidence-maturity-adoption-intelligence.md`](../../plans/0.19.0/evidence-maturity-adoption-intelligence.md)
Linked policy:
Support/status impact: [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md), [`CANARY_MATRIX.md`](../status/CANARY_MATRIX.md), and [`PROOF_FRESHNESS.md`](../status/PROOF_FRESHNESS.md)
Proof commands: docs-check, doc-test, docs-source-check, product-claims-check, git diff --check

## Summary

This lane moved perfgate from first-hour usability into repeated-use evidence
maturity. The tool can now help a team decide whether a benchmark is only a
smoke check, suitable as advisory evidence, mature enough to gate, noisy enough
to rerun or pair, or rich enough to justify a structured decision.

The lane preserved the product boundaries:

- local receipts remain the correctness contract;
- server ledger mode remains optional team history;
- benchmark selection stays reviewable rather than automatic;
- maturity output remains advisory unless a later accepted policy promotes it;
- no public crates, receipt schemas, CLI command names, release aliases, or
  GitHub Action inputs were changed as part of the closeout.

## What Changed

The implemented surface now includes:

- benchmark recipe metadata for `perfgate init --suggest-benches`, including
  best-for, bad-for, expected-noise, recommended-mode, blocking, and paired
  hints;
- benchmark recipe and anti-pattern guidance for durable workload selection;
- `perfgate baseline doctor` for missing, new, immature, mature, stale,
  host-mismatched, and high-noise baseline classifications;
- `perfgate doctor signal` for sample count, CV/noise, host stability,
  baseline age, drift, and advisory/gate/paired recommendations;
- `perfgate calibrate --emit-patch` for reviewable non-mutating TOML guidance;
- structured-decision example packs and richer `decision suggest` reason
  output;
- a canary freshness matrix for external, hosted, release, action, artifact,
  server, and unproven probe-backed canary shapes;
- optional server ledger backup/restore smoke plus retention and migration
  policy guidance;
- an agent repair-context contract and fixtures for agent-operable failure
  guidance; and
- proof freshness tiers mapped into product claims.

## Covered Maturity States

The lane covers these evidence states from the contract:

- smoke benchmark;
- advisory benchmark;
- gate benchmark;
- paired benchmark;
- scenario benchmark;
- mature baseline;
- immature baseline;
- stale baseline;
- noisy signal;
- host mismatch;
- decision candidate; and
- ledger candidate.

The output is intentionally guidance-first. It tells users what evidence means
and what to run next, but it does not silently change config, promote baselines,
loosen thresholds, or make ledger history part of local correctness.

## Proof Records

Durable lane artifacts:

- [`PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence`](../proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md)
- [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md)
- [`PERFGATE-SPEC-0010-agent-repair-context-contract`](../specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md)
- [`evidence-maturity-adoption-intelligence.md`](../../plans/0.19.0/evidence-maturity-adoption-intelligence.md)
- [`CANARY_MATRIX.md`](../status/CANARY_MATRIX.md)
- [`PROOF_FRESHNESS.md`](../status/PROOF_FRESHNESS.md)
- [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md)

Representative proof from the lane included targeted CLI/server tests for
recipe output, baseline doctor, signal doctor, calibration patch output,
decision examples, decision suggestion reasons, server backup/restore, and
repair-context scenarios. Product-claim and docs-source checks now keep the
evidence maturity claims linked to proof.

Closeout proof:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## What Not To Infer

- This lane did not add a new benchmark engine.
- This lane did not make server ledger mode required.
- This lane did not add a dashboard.
- This lane did not expand the five public crates.
- This lane did not make benchmark suggestions automatic policy.
- This lane did not add mutation-heavy policy.
- This lane did not make advisory maturity classifications block PRs.
- This lane did not prove a probe-backed external canary; that remains
  `unproven` in the canary matrix.

## Remaining Work

Recommended next work should start from observed repeated-use friction, not
more scaffolding. Good follow-up candidates are:

- refresh the public `0.18.0` canary set from released artifacts when adoption
  proof needs current external coverage;
- run a probe-backed external canary with stable probe IDs;
- deepen server operations proof for production database backup/restore,
  migration compatibility, and large histories;
- use repair-context fixtures to improve agent-specific workflows only where
  agents still need to infer from logs; and
- promote advisory maturity output to policy only after a separate accepted
  policy/spec change.

## Active Goal Handling

This closeout archives `.codex/goals/active.toml` as
`.codex/goals/archive/perfgate-evidence-maturity-adoption-intelligence.toml`
with status `completed`.

