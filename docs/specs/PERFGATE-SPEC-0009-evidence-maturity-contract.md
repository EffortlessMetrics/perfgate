# PERFGATE-SPEC-0009: Evidence maturity contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-18
Milestone: 0.19.0
Behavior version: evidence-maturity-contract.v1
Product surface: benchmark recipes, baseline doctor, signal doctor, calibration patch output, decision examples, decision suggestions, canary matrix, server ledger operations, proof freshness
CI surface: docs-source-check, product-claims-check, doc-test, focused CLI tests, server tests, schema-compat, action-check
Schema impact: no receipt schema change by default; maturity behavior reads existing config, run, compare, report, repair context, decision, probe, scenario, tradeoff, and ledger export data
Action impact: no action input or alias change by default; action summaries may link maturity recommendations only when generated from existing receipts
Server impact: server ledger remains optional team history; backup/restore and retention proof must not make the server part of local correctness
Linked proposal: docs/proposals/PERFGATE-PROP-0006-evidence-maturity-adoption-intelligence.md
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions, PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional
Linked plan: evidence maturity and adoption intelligence implementation plan (planned)
Linked policy: policy ledgers remain source of truth for governed exceptions, public surface, generated files, workflow policy, and release proof
Support/status impact: product claims should add or promote evidence-maturity, canary-freshness, baseline-trust, server-ops, and proof-freshness claims only after behavior and proof land
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

perfgate 0.18 made the tool public and first-use guided. A team can install it,
initialize a repository, run local gates, promote baselines, wire CI, inspect
artifacts, use action reproduction, graduate into decisions, and optionally use
ledger history.

The week-two product gap is evidence maturity. Teams need perfgate to answer
whether a benchmark, baseline, and signal are good enough to trust before those
results block review. Without this contract, a weak workload can become a
polished but misleading gate.

This spec defines the maturity vocabulary and user-facing behavior required for
benchmark recipes, baseline trust, signal trust, calibration patches, decision
examples, canary freshness, optional ledger operations, and product proof age.

## Behavior

perfgate MUST preserve the receipts-first model while adding advisory maturity
classification over existing evidence.

The maturity contract is:

```text
what is being measured
how mature the baseline is
how noisy the signal is
whether the workload should gate or advise
whether paired mode is more appropriate
whether the result is a simple regression, noise, or tradeoff
what artifact proves it
what reviewer or agent action comes next
```

Maturity output MUST be advisory unless a later accepted spec defines blocking
policy. perfgate MUST NOT silently promote baselines, loosen thresholds, write
policy, or turn weak evidence into required CI behavior.

## Evidence vocabulary

The following terms are the canonical user-facing maturity vocabulary for this
lane:

| Term | Meaning |
|------|---------|
| `smoke benchmark` | fast, low-setup check that proves command wiring and rough performance shape; usually not enough to block PRs alone |
| `advisory benchmark` | useful signal that should be reported but should not block until calibrated and stable |
| `gate benchmark` | mature enough to fail or warn CI according to configured policy |
| `paired benchmark` | benchmark that should compare baseline/current under paired conditions because ordinary runs are noisy or host-sensitive |
| `scenario benchmark` | benchmark whose meaning depends on workload weighting or structured tradeoff policy |
| `mature baseline` | baseline with enough samples, recent evidence, compatible host context, and acceptable noise for its configured purpose |
| `immature baseline` | baseline that exists but lacks enough evidence to support blocking decisions |
| `stale baseline` | baseline whose age, drift, or source context makes refresh/review advisable |
| `host-mismatched baseline` | baseline collected on a host class or fingerprint incompatible with current evidence |
| `noisy signal` | evidence whose variation exceeds configured or suggested confidence bounds |
| `decision candidate` | result pattern where a simple pass/fail gate is insufficient because meaningful metrics move in different directions |
| `ledger candidate` | decision that may be worth recording as team history after local receipt correctness is established |

User-facing output MAY use shorter labels, but it MUST preserve these meanings.

## Benchmark recipes

perfgate SHOULD provide reviewable benchmark recipes for common starting shapes:

- `rust-cli-smoke`;
- `rust-workspace-advisory`;
- `node-command`;
- `python-command`;
- `http-smoke`; and
- `generic-command`.

Each recipe MUST expose these metadata fields in generated comments, docs, or a
machine-readable internal model:

```text
Best for
Bad for
Expected noise
Recommended mode
Advisory vs blocking
Paired-mode hint
```

Recipe output MUST be reviewable, not magical. It MAY suggest command snippets
and default posture, but it MUST NOT silently mark a workload as blocking, write
a baseline, or promote policy without explicit user action.

Recipe guidance SHOULD flag anti-patterns:

- compile-heavy commands as first-hour blocking gates;
- network-heavy commands without isolation;
- tests that mix correctness and performance evidence;
- tiny runtimes below timer/noise usefulness;
- un-warmed workloads where startup and steady-state are mixed accidentally;
- commands that depend on mutable external services; and
- benchmarks that are too broad to identify review action.

## Baseline maturity

perfgate SHOULD provide an advisory baseline health view:

```bash
perfgate baseline doctor --config perfgate.toml
```

The command SHOULD classify each configured benchmark baseline as one of:

- `missing`;
- `new`;
- `immature`;
- `mature`;
- `stale`;
- `host_mismatched`; or
- `high_noise`.

Baseline maturity SHOULD consider, when available:

- baseline existence;
- sample count;
- baseline age;
- last promotion context;
- host fingerprint or host class compatibility;
- configured required/advisory posture;
- coefficient of variation or equivalent noise evidence;
- recent drift from run or compare receipts; and
- whether paired mode would be safer.

The command MUST NOT promote baselines automatically. It MUST explain whether a
baseline is safe to use as a gate, should remain advisory, needs more samples,
needs host-compatible refresh, or should be rerun in paired mode.

## Signal maturity

perfgate SHOULD provide signal maturity output through either `doctor` or a
dedicated subcommand:

```bash
perfgate doctor signal --config perfgate.toml
```

Signal maturity output SHOULD report per benchmark:

- sample count;
- coefficient of variation or equivalent noise evidence;
- host stability;
- baseline age;
- recent drift;
- current required/advisory posture;
- suggested gate/advisory status; and
- paired-mode recommendation when ordinary comparisons are not trustworthy.

The recommendation set SHOULD include:

- `safe_to_gate`;
- `advisory_only`;
- `increase_samples`;
- `use_paired_mode`;
- `refresh_baseline`;
- `check_host_mismatch`; and
- `no_decision_yet`.

Signal doctor output MUST distinguish noisy evidence from performance
regression. It MUST NOT imply that more automation can compensate for an
unsuitable workload.

## Calibration patch output

Calibration suggestions MUST remain advisory by default. perfgate MAY add:

```bash
perfgate calibrate --config perfgate.toml --bench parser --emit-patch
```

`--emit-patch` SHOULD print a reviewable TOML block or patch fragment. It MUST
NOT write the config unless a later accepted spec defines explicit write
behavior.

Patch output SHOULD include:

- sample count;
- host class or fingerprint;
- coefficient of variation or equivalent noise evidence;
- suggested fail threshold;
- suggested warn threshold or factor;
- suggested noise threshold and policy;
- suggested repeat count;
- paired-mode recommendation when unstable;
- a reason section; and
- when not to apply the patch.

Example output shape:

```toml
# Suggested from 15 samples on gha-ubuntu-24.04-x86_64.
# CV: 4.2%; safe for PR gating if this workload remains isolated.
threshold = 0.10
warn_factor = 0.50
noise_threshold = 0.08
noise_policy = "warn"
repeat = 7
```

## Decision examples and explanation

perfgate SHOULD provide examples or fixtures for common structured-decision
review patterns:

- latency regression with throughput improvement;
- memory regression with runtime improvement;
- startup slower but steady-state faster;
- probe regression with dominant workload improvement; and
- noise too high for a decision.

Examples SHOULD be available through docs, fixtures, or:

```bash
perfgate decision examples
```

`perfgate decision suggest` SHOULD explain why it recommends:

- simple gate;
- paired mode;
- structured decision;
- no decision yet; or
- optional ledger history.

Reason output SHOULD name:

- meaningful metric movement;
- metric direction where sign alone could mislead;
- threshold result;
- noise result;
- relevant artifacts;
- probe/scenario/tradeoff evidence when present;
- missing evidence when not ready; and
- next command.

Decision explanation MUST NOT make structured decisions mandatory for ordinary
local gates. It MUST NOT treat noise as an accepted tradeoff.

## Canary freshness

perfgate SHOULD track external and release canaries in a durable matrix:

```text
canary
repo shape
last run
proof artifact
what it proves
what it does not prove
freshness
```

The initial matrix SHOULD cover:

- small Rust CLI;
- large Rust workspace;
- non-Rust command benchmark;
- hosted external PR;
- public release install path;
- failure summary path;
- artifact upload path; and
- optional server-ledger path.

Freshness states SHOULD include:

- `current`;
- `recent`;
- `stale`;
- `superseded`; and
- `unproven`.

The matrix MUST NOT overstate canaries. One hosted external PR proves one repo,
runner, and workflow shape. It does not prove every hosted runner or every
repository shape.

## Server ledger operations

Server ledger mode MUST remain optional team history. Local receipts remain the
correctness contract.

The lane SHOULD add operational proof for:

- backup/export;
- restore/import into a fresh store;
- latest/history equivalence after restore;
- audit record preservation;
- key rotation compatibility where applicable;
- prune dry-run preservation; and
- retention or migration policy expectations.

Server operations proof MUST distinguish local decision correctness from
ledger availability. A team MAY configure blocking upload policy, but perfgate
MUST NOT make that the default product contract.

## Proof freshness tiers

Product claims SHOULD be able to describe proof freshness without duplicating
audits:

| Tier | Meaning |
|------|---------|
| `current` | proof was run against the current lane or release boundary |
| `recent` | proof remains relevant but was not rerun in the latest slice |
| `stale` | proof may still be informative but should not support new claims alone |
| `superseded` | proof was replaced by newer evidence or an explicit closeout |
| `unproven` | no durable evidence exists yet |

Freshness tiers SHOULD be applied to external canaries, platform support,
server operations, release smoke, hosted Action proof, and other claims that can
age without code changes.

## Non-goals

- Do not add another benchmark engine.
- Do not make server ledger mode required for correctness.
- Do not build a dashboard in this lane.
- Do not expand the five public crates.
- Do not change CLI command names, receipt schemas, GitHub Action behavior, or
  release aliases without a separate accepted spec.
- Do not silently auto-promote baselines, loosen thresholds, or write policy.
- Do not require structured decisions, probes, or ledger history for local
  gates.
- Do not make external canaries mandatory CI until freshness policy is accepted
  and proven stable.
- Do not duplicate policy ledger rows or release matrices in this spec.

## Required evidence

Documentation-only changes to this spec SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Behavior changes SHOULD add focused proof for the touched surface before broad
workspace validation:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 test -p perfgate-cli --all-features calibrate
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 test -p perfgate-server --all-features
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- action-check
```

Cross-cutting implementation SHOULD also run:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

## Acceptance examples

| Example | Result |
|---------|--------|
| A `rust-cli-smoke` recipe says it is good for command wiring and rough smoke, bad for steady-state throughput, expected low-to-medium noise, and advisory until calibrated. | Pass |
| A `rust-workspace-advisory` recipe says compile-heavy workspace tests should not block PRs before calibration. | Pass |
| `baseline doctor` reports a baseline as `immature` because it has too few samples and tells the user to keep it advisory. | Pass |
| `baseline doctor` reports `host_mismatched` when current evidence is collected on a different host class. | Pass |
| `doctor signal` recommends `use_paired_mode` for high-noise evidence. | Pass |
| `calibrate --emit-patch` prints a TOML block and a reason section without editing `perfgate.toml`. | Pass |
| `decision suggest` says a structured decision may help because throughput improved above threshold while RSS regressed above threshold. | Pass |
| The canary matrix marks a hosted external PR canary as `stale` after its freshness window expires. | Pass |
| Server restore smoke verifies latest/history/audit equivalence without changing local decision correctness. | Pass |
| A recipe silently marks a generated benchmark as required and promotes its first baseline. | Fail |
| Signal doctor labels noisy evidence as safe to gate without explaining noise. | Fail |
| Calibration writes config by default. | Fail |
| A server export failure is reported as invalidating local receipts by default. | Fail |
| Product claims cite a stale canary as current proof. | Fail |

## Test mapping

Current or planned proof maps to:

- CLI init tests for recipe metadata and generated comments;
- benchmark recipe docs and anti-pattern examples;
- CLI baseline tests for baseline doctor classification;
- CLI doctor tests for signal maturity output;
- CLI calibrate tests for `--emit-patch`;
- CLI decision tests for explanation reasons and example packs;
- action-check fixtures if action summaries surface maturity guidance;
- server tests for backup/restore, retention, and migration behavior;
- canary matrix docs checked by docs-source-check and product-claims-check;
- product-claims-check for proof freshness tiers; and
- schema-compat whenever receipt or export shape changes.

## Implementation mapping

The evidence maturity contract is owned by:

- `crates/perfgate-cli/src/init.rs` for benchmark recipe suggestions;
- CLI baseline command modules for `baseline doctor`;
- CLI doctor/calibration modules for signal maturity and patch output;
- `crates/perfgate-cli/src/decision_suggest.rs` and decision examples for
  explanation output;
- server store and CLI/server tests for ledger backup/restore proof;
- `docs/status/` for canary matrix and proof freshness status;
- `docs/status/PRODUCT_CLAIMS.md` for proof-backed claims;
- `docs/specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md` for the
  separate agent repair-context behavior contract when added; and
- `plans/0.19.0/` and `.codex/goals/active.toml` for implementation sequencing
  when this lane becomes active.

Policy files remain the source of truth for governed exceptions, public
surface, no-panic state, file policy, workflow policy, and release proof. This
spec may link policy ledgers but MUST NOT copy their rows.

## CI proof

Evidence maturity changes MUST select proof commands by affected surface:

| Surface | Proof |
|---------|-------|
| Proposal/spec/plan/status docs | `docs-check`, `doc-test`, `docs-source-check`, `product-claims-check`, `git diff --check` |
| Benchmark recipes | focused CLI init tests and doc-test examples |
| Baseline doctor | focused CLI baseline tests |
| Signal doctor | focused CLI doctor tests |
| Calibration patch | focused CLI calibrate tests |
| Decision explanations/examples | focused CLI decision tests and example fixture tests |
| Canary matrix/proof freshness | docs-source-check and product-claims-check |
| Server ops smoke | focused server and CLI/server tests |
| Action maturity guidance | `cargo +1.95.0 run -p xtask -- action-check` |
| Receipt/schema impact | `cargo +1.95.0 run -p xtask -- schema-compat` |
| Public surface risk | `cargo +1.95.0 run -p xtask -- public-surface --strict` |

## Promotion rule

This spec is accepted when merged as the evidence maturity behavior contract.
It is implemented when:

- the 0.19 evidence maturity implementation plan exists;
- benchmark recipe metadata and anti-pattern guidance are implemented or
  explicitly deferred;
- baseline doctor classifies missing, new, immature, mature, stale,
  host-mismatched, and high-noise baselines;
- signal doctor reports sample count, noise, host stability, baseline age,
  drift, and recommendations;
- calibration can emit a reviewable non-mutating patch;
- decision examples and decision-suggestion reasons are available;
- canary freshness is tracked in status docs;
- server backup/restore or equivalent operational proof exists;
- proof freshness tiers are mapped to product claims; and
- the lane closeout records covered maturity states, remaining advisory
  surfaces, non-inferences, and next work.
