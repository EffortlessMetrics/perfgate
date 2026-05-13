# PERFGATE-SPEC-0003: Performance decision contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: performance-decision-contract.v1
Product surface: CLI, GitHub Action, receipts, optional server decision ledger
CI surface: perfgate-cli decision tests, action-check, schema-compat
Schema impact: decision index, decision bundle, decision record, scenario, tradeoff, probe receipts
Action impact: decision-enabled action path and local reproduction output
Server impact: optional decision upload, history, export, prune, and debt ledger
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions, PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional
Linked plan: plans/0.18.0/implementation-plan.md
Linked policy: decision policy in perfgate.toml; server authorization policy; release proof gates
Support/status impact: PG-CLAIM-0001, PG-CLAIM-0002, PG-CLAIM-0003, and PG-CLAIM-0007 in docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 test -p perfgate-cli --all-features decision; cargo +1.95.0 run -p xtask -- action-check; cargo +1.95.0 run -p xtask -- schema-compat

## Problem

perfgate's advanced value is no longer just "detect a slower command." It now
supports structured performance decisions that combine compare receipts, probe
evidence, workload scenarios, tradeoff policy, review requirements, portable
bundles, and optional server history.

That workflow needs a stable product contract. Without one, future changes can
silently break artifact shape, action reproduction, local-first behavior, or
the distinction between correctness receipts and optional server ledger state.

## Behavior

Performance decisions are receipts-first and local-first. The primary contract
is the local artifact set; the server ledger is optional team-scale history.

The complete decision workflow is:

1. `perfgate check`
2. `perfgate ingest probes`
3. `perfgate probe compare`
4. `perfgate scenario evaluate`
5. `perfgate tradeoff evaluate`
6. `perfgate decision evaluate`
7. `perfgate decision bundle`
8. optional `perfgate decision upload`
9. optional `perfgate decision history`, `latest`, `export`, `prune`, and `debt`

The workflow MAY be run in smaller pieces, but `decision evaluate` MUST be able
to render a review-ready local decision from configured compare, probe,
scenario, and tradeoff evidence without requiring the server.

## Required artifacts

The decision workflow writes or consumes these artifacts, under the configured
artifact root unless explicitly overridden:

- `run.json`
- `compare.json`
- `probes.json`
- `probe-compare.json`
- `scenario.json`
- `tradeoff.json`
- `decision.md`
- `decision.index.json`
- `decision-bundle.json`

`decision-bundle.json` is optional in the sense that it is created by
`decision bundle`, not by every `decision evaluate` run. When created, it MUST
be portable enough for release, audit, issue, or agent handoff attachment
without requiring server access.

## Required user-facing answer

`decision evaluate` and the action decision path MUST help a reviewer answer:

- what got slower;
- what got faster;
- which workload matters;
- which policy accepted or rejected the tradeoff;
- whether evidence was trustworthy enough;
- whether review is required; and
- how to reproduce locally.

The GitHub Action decision path MUST surface the local reproduction command
when decision evaluation runs.

## Optional server ledger

Server decision storage extends the local receipts model. It MUST consume
decision receipts rather than invent a separate decision model.

The server ledger MAY provide:

- upload of accepted or rejected decision records;
- latest and history lookup;
- JSONL or JSON export;
- prune with explicit dry-run or force behavior;
- debt summaries for accepted tradeoffs; and
- audit events for ledger mutations.

Local decision correctness MUST NOT depend on the server being available.

## Non-goals

- This spec does not make the server mandatory.
- This spec does not turn perfgate into a profiler.
- This spec does not define a new receipt schema version.
- This spec does not change action inputs.
- This spec does not change review-required policy values.
- This spec does not require every project to use probe evidence.

## Required evidence

Performance-decision changes MUST run the narrow product proof:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
```

Documentation-only changes to this spec SHOULD also run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Acceptance examples

| Example | Result |
|---------|--------|
| `decision evaluate` writes `decision.md` and `decision.index.json` from local configured evidence. | Pass |
| `decision bundle` exports `perfgate.decision_bundle.v1` from `decision.index.json`. | Pass |
| The GitHub Action decision path prints the local `perfgate decision evaluate --config perfgate.toml` reproduction command. | Pass |
| Server upload stores a decision record generated from receipt evidence. | Pass |
| Server history is required before local `decision evaluate` can decide a tradeoff. | Fail |
| A decision report says review is not required while policy evidence marks it required. | Fail |
| A bundle omits the index needed to locate scenario, tradeoff, decision, and compare evidence. | Fail |

## Test mapping

The current proof is mapped to:

- [`cli_structured_decision_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_structured_decision_e2e_tests.rs)
- [`cli_performance_decision_example_tests.rs`](../../crates/perfgate-cli/tests/cli_performance_decision_example_tests.rs)
- [`cli_release_decision_proof_tests.rs`](../../crates/perfgate-cli/tests/cli_release_decision_proof_tests.rs)
- [`cli_tradeoff_tests.rs`](../../crates/perfgate-cli/tests/cli_tradeoff_tests.rs)
- [`cli_probe_tests.rs`](../../crates/perfgate-cli/tests/cli_probe_tests.rs)
- [`cli_scenario_tests.rs`](../../crates/perfgate-cli/tests/cli_scenario_tests.rs)
- [`cli_server_tests.rs`](../../crates/perfgate-cli/tests/cli_server_tests.rs)
- [`cli_mock_server_tests.rs`](../../crates/perfgate-cli/tests/cli_mock_server_tests.rs)
- `cargo +1.95.0 run -p xtask -- action-check`
- `cargo +1.95.0 run -p xtask -- schema-compat`

## Implementation mapping

Current behavior is documented or implemented across:

- [`docs/PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md)
- [`docs/RELEASE_READINESS.md`](../RELEASE_READINESS.md)
- [`examples/performance-decision`](../../examples/performance-decision)
- `crates/perfgate-cli`
- `crates/perfgate-types`
- `crates/perfgate-server`
- `crates/perfgate-client`
- the composite GitHub Action checked by `xtask action-check`

The source-of-truth status claims are:

- `PG-CLAIM-0001`: reviewable performance decisions
- `PG-CLAIM-0002`: portable local-first decision bundles
- `PG-CLAIM-0003`: optional server decision ledger
- `PG-CLAIM-0007`: action local reproduction for decisions

## CI proof

Changes to decision behavior, action decision wiring, or decision schemas MUST
name the relevant proof commands in the PR body and run the narrow gates before
release readiness:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
```

Release candidates SHOULD also link to the structured decision proof row in
[`docs/RELEASE_READINESS.md`](../RELEASE_READINESS.md).

## Promotion rule

This spec is accepted when merged as a docs-only behavior contract. It is
implemented when:

- `PERFGATE-ADR-0002-receipts-first-performance-decisions` records the
  receipts-first architecture decision;
- `PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional` records the
  local-first server boundary;
- the status proof map tracks decision, bundle, server ledger, and action
  reproduction claims;
- the 0.18.0 implementation plan identifies any remaining decision-contract
  follow-up; and
- the decision proof commands above pass.
