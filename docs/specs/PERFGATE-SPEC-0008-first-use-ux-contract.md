# PERFGATE-SPEC-0008: First-use UX contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-15
Milestone: 0.19.0
Behavior version: first-use-ux-contract.v1
Product surface: doctor, init, check, baseline promotion, artifact explanation, calibration, action summaries, decision guidance, probe starters, optional ledger readiness
CI surface: docs-source-check, product-claims-check, doc-test, action-check, CLI focused tests, hosted external action canary
Schema impact: no new receipt schema version by default; behavior may read existing run, compare, report, repair context, decision, probe, scenario, and tradeoff receipts
Action impact: every action summary path must expose artifact paths and a local reproduction command
Server impact: ledger readiness is optional team history; local receipts remain the correctness contract
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions
Linked plan: first-use-intelligence implementation plan (planned)
Linked policy: policy ledgers remain source of truth for governed exceptions and public surface
Support/status impact: product claims should be added or promoted only after behavior has proof
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

perfgate already has the primitives for reviewable performance decisions:
local gates, baselines, action summaries, probe evidence, structured decisions,
decision bundles, release proof, and an optional server ledger. The remaining
first-use gap is that users still need to infer the right next step from docs,
artifacts, and command output.

The UX contract must make perfgate teach the workflow while preserving the
receipts-first model. A cold user should not need to understand the full
architecture to answer:

```text
what happened?
what does it mean?
what artifact proves it?
what should I run next?
what should I avoid doing?
```

This spec defines the required behavior for first-use intelligence and review
ergonomics so CLI output, GitHub Action summaries, docs, examples, tests,
product claims, and agents share one contract.

## Behavior

perfgate MUST make the next useful action visible across first-use and review
surfaces. The standard explanation shape is:

```text
Status
Meaning
Artifacts
Next
Do not
```

Surfaces MAY adapt labels for compact output, but they MUST preserve the same
information when reporting setup state, failures, calibration suggestions,
artifact explanations, decision readiness, or ledger readiness.

## Adoption states

`perfgate doctor` MUST classify common adoption states and print a next command.

| State | Meaning | Required next action |
|-------|---------|----------------------|
| `no_config` | no `perfgate.toml` is available | initialize the repo |
| `configured_no_benches` | config exists but no runnable benches are configured | add or review benchmark suggestions |
| `benches_no_baselines` | benches exist but durable baselines are missing | run check, inspect artifacts, then promote reviewed baselines |
| `ready_local` | config and baselines are ready for local checks | run check with `--require-baseline` |
| `ready_ci` | generated CI workflow and baselines are present | use the generated action ref and local reproduction command |
| `noisy_signal` | recent evidence is too noisy for confident gating | calibrate, increase repeats, use advisory mode, or consider paired mode |
| `decision_candidate` | evidence suggests a tradeoff question | evaluate whether probes, scenarios, and decision receipts are useful |
| `ledger_configured` | server ledger settings are present | verify ledger readiness while preserving local receipt correctness |

The classifier MUST NOT make server mode, structured decisions, or probes a
prerequisite for `ready_local`.

## Benchmark suggestions

`perfgate init` MAY generate benchmark suggestions for repo profiles such as:

- `rust-cli`;
- `rust-workspace`;
- `node`; and
- `generic-command`.

Suggestions MUST be reviewable. They MUST NOT be silently treated as durable
policy. Suggested config SHOULD explain whether the candidate is a fast
first-hour check, a heavier advisory check, or a benchmark that should remain
non-blocking until calibrated.

Generated suggestions SHOULD prefer low setup cost for first-hour adoption.
Compile-heavy or environment-sensitive commands SHOULD be marked advisory or
non-required until the user reviews and calibrates them.

## Failure taxonomy

CLI and action output MUST classify covered failures before presenting repair
guidance.

| Class | Meaning | Required guidance |
|-------|---------|-------------------|
| `setup_missing_config` | no usable config was found | initialize or pass `--config` |
| `setup_missing_bench` | no runnable benchmark is configured or selected | add/review a benchmark command |
| `setup_command_failed` | the benchmark command did not complete | inspect command stderr/stdout and fix command setup |
| `missing_baseline` | setup is incomplete; this is not a regression | inspect first run and promote only if trusted |
| `performance_regression` | current evidence exceeded configured budget | inspect comparison artifacts and reproduce locally |
| `high_noise` | evidence is too noisy for confident judgment | calibrate, repeat, keep advisory, or use paired mode |
| `unsupported_metric` | a requested metric is unavailable on this platform | use supported metrics or platform-specific guidance |
| `host_mismatch` | baseline and current evidence came from incompatible host classes | rerun on matching hosts or refresh baseline intentionally |
| `review_required` | policy requires human review despite generated receipts | inspect decision evidence and reviewer guidance |
| `server_upload_failed` | optional ledger upload failed | distinguish upload failure from local decision correctness |

Each class MUST provide status, meaning, artifact references when available, a
next command or inspection target, and one "do not" guardrail where useful.

## Artifact explanation

perfgate MUST provide a CLI path to explain artifact directories, starting with:

```bash
perfgate explain artifacts --out-dir artifacts/perfgate
```

The explanation MUST identify known receipt and review files when present:

- `run.json`;
- `compare.json`;
- `report.json`;
- `comment.md`;
- `repair_context.json`;
- `decision.md`;
- `decision.index.json`;
- `decision-bundle.json`;
- `probe-compare.json`;
- `scenario.json`; and
- `tradeoff.json`.

For each known artifact, output SHOULD include the artifact role and the next
useful command, such as rerunning a check, reading a comment summary, bundling a
decision, or inspecting repair context. Unknown files MAY be listed without
semantic claims.

## Calibration suggestions

Calibration MUST be advisory by default. A first implementation MUST NOT write
config changes unless a later spec or explicit flag defines that behavior.

The advisory command shape is:

```bash
perfgate calibrate --config perfgate.toml --bench parser
```

Calibration output SHOULD include:

- sample count;
- coefficient of variation or equivalent noise evidence;
- suggested fail threshold;
- suggested warn threshold;
- suggested noise threshold and noise policy;
- host class or fingerprint context when available;
- repeat guidance; and
- paired-mode guidance when evidence is unstable.

Calibration MUST NOT imply that statistical suggestions are guarantees. Output
SHOULD name the receipts or samples used for the recommendation.

## GitHub Action summaries

Every GitHub Action summary path MUST include a local reproduction block.

At minimum, action summaries MUST expose:

- verdict;
- failed or warned benches when applicable;
- artifact paths or uploaded artifact names;
- local reproduction command;
- baseline promotion hint for setup/missing-baseline states;
- decision evaluation command when decision mode is enabled; and
- noise, unsupported metric, host mismatch, or review-required guidance when
  those classes apply.

Action summaries MUST distinguish setup failures from performance regressions.
They MUST NOT imply that optional server upload failures invalidate local
receipts unless the user's policy explicitly made upload blocking.

## Decision readiness

Structured decisions MUST be a graduation path, not first-hour ceremony.

The advisory command shape is:

```bash
perfgate decision suggest --config perfgate.toml
```

Decision readiness output SHOULD say one of:

- a simple local gate is enough;
- paired mode is more appropriate;
- structured decisions may help;
- structured decisions are not ready because required probe, scenario, or
  tradeoff evidence is missing;
- decision evidence is ready to bundle; or
- optional ledger upload may help the team track accepted debt.

The command MUST preserve local receipt correctness and MUST NOT require server
mode before decision receipts can be evaluated or bundled.

## Probe starters

Probe starter templates SHOULD generate stable examples and receipt wiring
without turning probes into profiling.

The starter command shape is:

```bash
perfgate probes init --template parser
perfgate probes init --template batch
perfgate probes init --template cli
perfgate probes init --template server
```

Starters SHOULD include:

- stable probe naming examples;
- JSONL example events;
- scenario and tradeoff template snippets when useful;
- expected artifact paths; and
- next commands for ingest, compare, decision evaluate, and bundle.

Starters MUST NOT require server mode. Broad language-specific instrumentation
APIs are out of scope unless a later proposal and spec add them.

## Ledger readiness

Server ledger mode MUST remain optional team history.

The readiness command shape is:

```bash
perfgate ledger doctor
```

Ledger readiness SHOULD report:

- local receipts readiness;
- server URL configuration;
- API key presence and validity when checkable;
- project configuration;
- upload mode;
- history reachability;
- export availability;
- prune dry-run availability; and
- a reminder that local receipts remain the correctness contract.

For unconfigured users, ledger readiness SHOULD say that server mode is not
needed yet.

## Non-goals

- Do not add new performance primitives by default.
- Do not change the five public crates.
- Do not reopen wrapper absorption, guided adoption, external trust canaries,
  or 0.18 release cutover.
- Do not make server ledger mode mandatory.
- Do not require probes or structured decisions for local checks.
- Do not make calibration write config in the first advisory implementation.
- Do not duplicate policy ledger rows or release matrices.
- Do not claim hosted external action proof until a hosted canary exists.

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
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- action-check
```

Cross-cutting or receipt-affecting changes SHOULD also run:

```bash
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- product-claims-check
```

Hosted action canary proof SHOULD record the external repository, generated
workflow, baseline setup, hosted action result, uploaded artifacts, local
reproduction command, user confusion, and changes made.

## Acceptance examples

| Example | Result |
|---------|--------|
| `doctor` reports `benches_no_baselines`, explains that this is setup, and prints check/promote commands. | Pass |
| `init --suggest-benches` emits commented benchmark candidates that a user must review before committing. | Pass |
| A missing-baseline CLI result says not to loosen thresholds to fix setup. | Pass |
| `explain artifacts` identifies `repair_context.json` as reproduction and repair guidance. | Pass |
| `calibrate` prints threshold suggestions and says which evidence it used without writing config. | Pass |
| Every action summary includes one copyable local reproduction command. | Pass |
| `decision suggest` says a simple gate is enough when no tradeoff evidence exists. | Pass |
| `ledger doctor` tells an unconfigured local user that server mode is not needed yet. | Pass |
| A CLI failure says only `failed` without a meaning or next command. | Fail |
| Benchmark suggestions auto-promote a required baseline without user review. | Fail |
| Calibration silently edits `perfgate.toml`. | Fail |
| Structured-decision guidance appears as mandatory setup for Level 1 local gates. | Fail |
| A server upload failure is reported as invalidating local decision receipts by default. | Fail |

## Test mapping

Current or planned proof maps to:

- CLI first-run and baseline tests for adoption-state transitions;
- CLI init tests for benchmark suggestions;
- CLI check tests for failure taxonomy and repair copy;
- artifact explanation tests for receipt recognition;
- calibration tests for advisory threshold and noise suggestions;
- `xtask action-check` fixtures for action summary reproduction blocks;
- decision tests for `decision suggest`;
- probe tests and examples for starter templates;
- server/CLI tests for `ledger doctor`;
- hosted external action canary audit notes; and
- `product-claims-check` for support-tier mapping once claims are added.

## Implementation mapping

The first-use UX contract is owned by:

- `crates/perfgate-cli` for command parsing and output;
- `perfgate::app` modules for reusable command behavior when applicable;
- composite action summary generation and `xtask action-check`;
- `docs/` guides and examples for user-facing explanation;
- `examples/` for deterministic templates and outcome examples;
- `docs/status/PRODUCT_CLAIMS.md` for proof-backed support claims;
- `.codex/goals/active.toml` and `plans/0.19.0/first-use-intelligence.md`
  once the implementation plan exists; and
- `docs/handoffs/` for lane closeout.

Policy files remain the source of truth for governed exceptions, public
surface, no-panic state, and file policy. This spec may link to those ledgers
but MUST NOT copy their rows.

## CI proof

First-use UX changes MUST select proof commands by affected surface:

| Surface | Proof |
|---------|-------|
| Proposal/spec/plan/status docs | `docs-check`, `doc-test`, `docs-source-check`, `product-claims-check`, `git diff --check` |
| Doctor/adoption state | focused CLI doctor tests |
| Init suggestions | focused CLI init tests |
| Failure taxonomy | focused CLI check tests and doc-test examples |
| Artifact explanation | focused CLI explain tests |
| Calibration | focused CLI calibrate tests |
| Action summaries | `cargo +1.95.0 run -p xtask -- action-check` |
| Decision readiness | focused CLI decision tests |
| Probe starters | focused CLI probe tests and examples |
| Ledger readiness | focused CLI/server tests |
| Receipt/schema impact | `cargo +1.95.0 run -p xtask -- schema-compat` |

## Promotion rule

This spec is accepted when merged as the first-use UX behavior contract. It is
implemented when:

- the first-use implementation plan and active goal manifest exist;
- adoption-state doctor behavior is implemented and tested;
- benchmark suggestions are implemented and tested;
- covered failure classes use the stable explanation shape;
- artifact explanation is implemented and tested;
- advisory calibration is implemented and tested;
- action summaries require local reproduction blocks;
- decision readiness suggestions are implemented and tested;
- probe starter templates are implemented or explicitly deferred with proof;
- ledger readiness preserves optional server semantics;
- product claims map first-use UX promises to proof; and
- the closeout handoff records covered states, covered failure classes, proof
  commands, non-inferences, and remaining work.
