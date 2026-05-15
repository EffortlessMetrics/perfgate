# First-Use Intelligence Closeout

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-15
Milestone: 0.19.0
Linked proposal: [`PERFGATE-PROP-0005-first-use-intelligence`](../proposals/PERFGATE-PROP-0005-first-use-intelligence.md)
Linked specs: [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked plan: [`first-use-intelligence.md`](../../plans/0.19.0/first-use-intelligence.md)
Linked policy:
Support/status impact: [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md) now maps first-use UX promises to tests, action checks, artifacts, and hosted canary proof.
Proof commands: docs-check, doc-test, docs-source-check, product-claims-check, action-check, targeted CLI and xtask tests

## What Changed

This lane moved perfgate's first-use surface from "documented" to guided:
commands now tell users what state they are in, what the result means, what
artifact proves it, what command comes next, and what not to do.

The lane added or hardened:

- `perfgate doctor` adoption-state output and next commands.
- `perfgate init --suggest-benches` templates for conservative first
  benchmarks.
- `perfgate explain artifacts` for receipt directory explanation.
- shared first-use failure classes and repair guidance.
- advisory `perfgate calibrate` threshold/noise suggestions.
- mandatory Action local reproduction and artifact summary coverage.
- `perfgate decision suggest` readiness guidance.
- `perfgate probes init` starter templates.
- `perfgate ledger doctor` optional server-ledger readiness checks.
- hosted external Action canary evidence against `EffortlessSteven/droid-action`.
- product-claim entries for the first-use UX surface.

## Covered States

The lane covers first-use states called out by the UX contract:

- no config;
- configured with no benchmarks;
- benchmarks without baselines;
- ready local gate;
- generated CI workflow;
- noisy signal guidance through calibration suggestions;
- decision readiness;
- optional ledger readiness.

## Covered Failure Classes

The repair-guidance work covers:

- setup missing config;
- setup missing benchmark;
- setup command failure;
- missing baseline;
- performance regression;
- high noise;
- unsupported metric;
- host mismatch;
- review required;
- server upload failure.

Each class is shaped around status, meaning, artifacts, next command, and
guardrail wording.

## Hosted Canary

The hosted canary used external PR
`https://github.com/EffortlessSteven/droid-action/pull/7` with a generated
perfgate workflow and a committed baseline. The first hosted run proved the pass
path and artifact upload. The forced-failure run proved a copyable local
reproduction command and uploaded failure artifacts.

The first forced-failure run exposed a Bash summary bug around optional decision
state and Markdown fences. The fix landed in perfgate `main` as
`978f1c211b2910c53918b522c01bdc8078381c33`, and the external canary rerun used
that action commit. The rerun intentionally failed the performance gate, printed
the local reproduction command, uploaded `perfgate-artifacts-25941883937-2`,
and no longer emitted the shell errors.

Durable audit:
[`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md).

## Product Claims

The lane added scoped claims for:

- adoption-state doctor and benchmark suggestions;
- artifact explanation and repair classes;
- advisory calibration and decision-readiness suggestions;
- probe starter templates;
- optional ledger readiness doctor;
- hosted external Action canary evidence.

The hosted canary claim is advisory. It proves one external repository, one
hosted runner shape, and one forced-failure workflow. It does not prove every
hosted runner or every repository shape.

## Proof Commands

Docs/status proof:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Action proof:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 test -p xtask action_check
```

Representative CLI proof included targeted doctor, init, explain, check,
calibrate, decision, probe, ledger, and server tests recorded in the individual
PR bodies and product-claim entries.

## Active Goal Handling

No `.codex/goals/active.toml` archive was performed for this lane. The active
goal still belongs to the operator-gated 0.18 release cutover, and that release
lane remains blocked only at explicit publication/tag/release/alias/public-smoke
steps.

## What Not To Infer

- This lane did not publish `0.18.0`.
- This lane did not create or move release tags or action aliases.
- This lane did not make server-ledger mode required for correctness.
- This lane did not prove every hosted external CI runner.
- This lane did not add new performance primitives.
- This lane did not replace the release cutover active goal.

## Remaining Follow-Up

The next useful work is release-operator execution for `0.18.0` when approval
exists, followed by public install smoke and publication closeout. Additional
first-use work should now come from real user friction, not from more scaffold.
