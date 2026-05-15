# PERFGATE-PROP-0005: First-use intelligence and review ergonomics

Status: proposed
Owner: perfgate maintainers
Created: 2026-05-15
Target milestone: 0.19.0
Linked specs: PERFGATE-SPEC-0008-first-use-ux-contract (planned)
Linked ADRs: none
Linked plan: first-use-intelligence implementation plan (planned)
Support/status impact: docs/status/PRODUCT_CLAIMS.md should add or update claims only after first-use UX behavior is implemented and proven
Policy impact: no policy rows by default; policy ledgers remain source of truth for governed exceptions, public surface, and release gates

## Problem

perfgate has the core product spine and release proof surfaces: first-hour docs,
adoption levels, structured decisions, probe guidance, action failure examples,
external canaries, server-ledger operations proof, final 0.18 pre-publish proof,
and an operator publish packet. The remaining first-use gap is not more proof
scaffolding.

The product still asks cold users to make hard performance-engineering choices
too early:

```text
what should I measure?
is this setup or a regression?
which artifact should I inspect?
is this benchmark too noisy to gate?
should I promote a baseline or fix the code?
when do probes and structured decisions become worth it?
do I need server mode yet?
```

perfgate should teach those choices through the workflow. It should make the
next correct step obvious while preserving the receipts-first model: local
measurements, comparisons, action summaries, decision bundles, and optional
ledger history all point to reviewable evidence.

## Users and surfaces

- Cold CLI users need `doctor`, `init`, `check`, and `baseline promote` to say
  what state the repo is in and what command comes next.
- Repository maintainers need benchmark suggestions that are reviewable, not
  silently auto-detected truth.
- GitHub Action users need every failure summary to make local reproduction and
  artifact inspection impossible to miss.
- Reviewers need CLI and action output that distinguishes missing baselines,
  real regressions, noisy evidence, unsupported metrics, and review-required
  decisions.
- Advanced users need a path from simple gates to calibrated gates, paired
  checks, probes, tradeoff policy, decision bundles, and optional ledger
  history without making beginners learn that entire system first.
- Team operators need server-ledger readiness checks that say when server mode
  is configured and when it is unnecessary.
- Maintainers and agents need specs, product claims, tests, examples, and
  handoffs that define the UX contract without duplicating policy ledgers or
  release matrices.

## Success criteria

- `doctor` reports an adoption state and at least one next command for common
  states such as no config, configured without benches, benches without
  baselines, local-ready, CI-ready, noisy signal, decision candidate, and
  ledger configured.
- `init` can generate reviewable benchmark suggestions for common repo shapes
  without silently treating guesses as committed truth.
- CLI failure output consistently separates setup failures, missing baselines,
  performance regressions, high noise, unsupported metrics, review-required
  decisions, and server upload failures.
- Failure output uses a stable shape: status, meaning, artifacts, next command,
  and what not to do.
- Users can explain an artifact directory from the CLI without reading the
  architecture docs.
- Calibration suggestions remain advisory unless a user explicitly asks the
  tool to write config changes.
- GitHub Action summaries always include verdict, affected benches, artifact
  paths, and a local reproduction command.
- Structured-decision guidance appears when there is a real tradeoff reason,
  not as required first-hour ceremony.
- Probe starters produce stable examples and JSONL/scenario/tradeoff templates
  before adding broad language-specific instrumentation APIs.
- Server ledger UX keeps the server visibly optional; local receipts remain the
  correctness contract.
- Product claims are updated only when behavior, examples, tests, or canaries
  prove the claim.
- The lane closes with a handoff that records covered first-use states, covered
  failure classes, proof commands, product-claim changes, and non-inferences.

## Proposed shape

Add a first-use UX lane that turns perfgate's existing evidence surfaces into a
guided path:

```text
install
doctor
init
choose benchmark
run
understand output
promote baseline
wire CI
debug first failure
tighten signal
add probes only when needed
make structured decisions only for real tradeoffs
use optional ledger history only when the team needs it
```

The lane should keep suggestions explicit and reviewable. For example, benchmark
suggestions should be emitted as commented config candidates with plain language
about why they are fast, heavy, advisory, or calibration-sensitive. The tool
should not silently commit benchmark policy on the user's behalf.

The CLI should converge on a consistent explanation shape:

```text
Status
Meaning
Artifacts
Next
Do not
```

That same shape should inform action summaries and docs examples so users learn
one review pattern across local and CI surfaces.

The first behavior spec should define the UX contract before implementation:

- every setup state has a next command;
- every failure class has a meaning and next action;
- every artifact directory can be explained;
- every CI failure has a local reproduction command;
- calibration suggestions are advisory unless explicitly written;
- structured decisions are suggested only when useful;
- server mode remains optional.

## Alternatives considered

### Add more documentation only

Rejected. The docs are now extensive and useful, but this lane is about the
tool teaching users while they use it. The right next command should appear in
`doctor`, `init`, `check`, action summaries, artifact explanations, and
calibration output, not only in a guide.

### Auto-detect and auto-commit benchmark policy

Rejected. Benchmark choice is product judgment. perfgate can suggest candidates,
but users should review benchmark commands, thresholds, and required/advisory
status before committing them.

### Make structured decisions the default first-hour path

Rejected. Structured decisions are perfgate's distinctive value, but they should
appear when there is a tradeoff, noisy evidence, probe data, or review need.
Level 1 local gates must remain complete and valuable by themselves.

### Turn probes into profiling

Rejected. Probes should remain tradeoff lenses. They explain where work moved
inside a reviewable performance decision; they should not make perfgate compete
with profilers or APM tools.

### Make server ledger mode part of correctness

Rejected. Server mode is optional team history. Local receipts, action
summaries, and decision bundles remain the correctness contract.

## Specs to create or update

- `PERFGATE-SPEC-0008-first-use-ux-contract` should define adoption states,
  failure classes, artifact explanation behavior, advisory calibration rules,
  CI reproduction requirements, decision-readiness rules, probe starter
  expectations, and ledger-readiness boundaries.
- Update `PERFGATE-SPEC-0007-guided-adoption-contract` only if the adoption
  ladder itself changes.
- Update `PERFGATE-SPEC-0003-performance-decision-contract` only if decision
  receipts, bundles, or decision-readiness behavior changes.
- Update `PERFGATE-SPEC-0005-release-proof-contract` only if public release
  proof starts to require first-use UX gates.

## Architecture decisions needed

No new ADR is required at lane start. The lane depends on existing decisions:

- receipts-first performance decisions;
- public crates as contracts and modules as architecture boundaries;
- local receipts first and server ledger optional.

Add an ADR only if implementation changes durable architecture, such as moving
benchmark suggestion policy into a new public contract surface or changing
local/server correctness boundaries.

## Evidence plan

Docs and proposal/spec PRs should run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Behavior PRs should add focused tests for the changed surface before broadening
validation:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
```

Cross-cutting or release-impacting changes should also run:

```bash
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- product-claims-check
```

The hosted external PR canary should produce an audit note that records the
external repository, generated workflow, baseline setup, first action result,
uploaded artifacts, local reproduction command, and what remains unproven.

## Risks

- Benchmark suggestions can create false confidence if they look authoritative
  instead of reviewable.
- Over-eager detection can generate slow compile-heavy gates that frustrate
  first-hour users.
- More CLI output can become noise if it is not consistently structured.
- Calibration suggestions can be mistaken for statistical guarantees unless
  they stay advisory and explain the evidence used.
- Structured-decision suggestions can overwhelm beginners if they appear before
  a tradeoff exists.
- Server-readiness checks can imply server mode is required unless the output
  clearly preserves local receipt correctness.
- Product claims can drift if claims are added before behavior and proof exist.

## Non-goals

- Do not add new performance primitives by default.
- Do not reopen wrapper absorption, source-of-truth governance, guided adoption,
  external canaries, or 0.18 release cutover.
- Do not change the five public crates.
- Do not make benchmark suggestions mutate committed config without explicit
  user action.
- Do not make calibration write config in the first advisory implementation.
- Do not make structured decisions or server ledger mode mandatory.
- Do not duplicate policy ledgers or release matrices in UX specs or docs.

## Exit criteria

This proposal is complete when:

- the first-use UX contract spec exists and is accepted;
- implementation plans sequence the work into PR-sized changes;
- `doctor` reports adoption state and next command;
- `init` can emit reviewable benchmark suggestions;
- CLI failure output uses the stable explanation shape for covered failure
  classes;
- artifact explanation is available from the CLI;
- calibration suggestions are advisory and proven;
- action summaries always expose local reproduction;
- decision readiness and probe templates guide users without forcing ceremony;
- ledger readiness says whether server mode is configured and remains optional;
- product claims are updated with proof-backed support tiers;
- a hosted external PR canary proves the CI path outside the perfgate repo;
- the lane closes with a handoff that records covered states, gaps, and
  non-inferences.
