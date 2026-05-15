# perfgate 0.19.0 First-use Intelligence Implementation Plan

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-15
Milestone: 0.19.0
Current PR: first-use intelligence plan
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked specs: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADRs: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Linked policy: policy ledgers remain referenced by specs and status docs; no policy row changes in this plan PR
Support/status impact: product claims should be added or promoted after behavior and hosted canary proof land
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check
Blocks: first-use UX implementation PRs
Blocked by: `.codex/goals/active.toml` currently tracks the operator-gated 0.18 release cutover and must not be archived before publication closeout
Rollback: revert this plan; proposal and spec remain valid source-of-truth artifacts

## Goal

Make perfgate guide users from first install to trustworthy performance review
without making them learn the architecture first.

The UX contract is:

```text
what happened
what it means
what proves it
what to run next
what not to do
```

This plan sequences the implementation for
[`PERFGATE-SPEC-0008-first-use-ux-contract`](../../docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md).

## Activation Boundary

The 0.18 release cutover goal is still active and blocked at explicit
release-operator publication, tag, release, alias, public-smoke, and closeout
steps. This plan does not archive or overwrite that goal.

When the release goal is closed, or when maintainers explicitly choose to switch
active Codex state to 0.19 UX work, `.codex/goals/active.toml` should point at
this plan and use the work items below. Until then, this plan is ready but the
machine-readable active goal remains the release cutover.

## Operating Rules

- Keep one semantic artifact or narrow product delta per PR.
- Preserve local receipts as the correctness contract.
- Keep server ledger mode optional team history.
- Keep structured decisions as a graduation path, not first-hour ceremony.
- Keep benchmark suggestions explicit and reviewable.
- Do not silently mutate committed benchmark policy.
- Do not write calibration changes in the first advisory implementation.
- Do not change the five public crates.
- Do not reopen wrapper absorption, guided adoption, external canaries, or
  release cutover.
- Product claims must wait for behavior and proof.

## PR Sequence

| PR | Work item | Status | Files / surface |
|----|-----------|--------|-----------------|
| 425 | First-use intelligence proposal | merged | `docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md` |
| 426 | First-use UX contract spec | merged | `docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md` |
| 427 | First-use implementation plan | merged | `plans/0.19.0/first-use-intelligence.md` |
| 428 | Adoption-state doctor | merged | `crates/perfgate-cli`, `perfgate::app`, CLI tests |
| 429 | Benchmark suggestion templates | merged | `init` behavior, templates, CLI tests |
| 430 | Artifact explanation command | merged | `perfgate explain artifacts`, CLI tests |
| 431 | Failure taxonomy and repair copy | merged | shared CLI/action wording helpers, tests |
| 432 | Calibration suggestions | merged | `perfgate calibrate`, CLI tests |
| 433 | Mandatory action reproduction block | merged | action summaries, `xtask action-check` fixtures |
| 434 | Decision readiness suggestions | merged | `perfgate decision suggest`, CLI tests |
| 435 | Probe starter templates | merged | `perfgate probes init`, examples, CLI tests |
| 436 | Ledger readiness doctor | merged | `perfgate ledger doctor`, CLI/server tests |
| 437 | Hosted external action canary | merged | audit note and hosted finding |
| 437a | Action step-summary shell fix | merged | `action.yml`, action proof |
| 438 | Product claim updates | merged | `docs/status/PRODUCT_CLAIMS.md` |
| 439 | First-use intelligence closeout | current | handoff; active goal remains 0.18 release cutover |

## Work item: adoption-state-doctor

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: benchmark-suggestions, failure-taxonomy
Blocked by:

### Goal

Make `perfgate doctor` report adoption state and a concrete next command.

### Production delta

Add an internal adoption-state classifier covering:

```text
no_config
configured_no_benches
benches_no_baselines
ready_local
ready_ci
noisy_signal
decision_candidate
ledger_configured
```

### Non-goals

- Do not require probes, structured decisions, or server ledger for
  `ready_local`.
- Do not rewrite unrelated doctor checks.

### Acceptance

- No-config, zero-bench, bench-without-baseline, baseline-ready, and
  workflow-generated states have focused tests.
- Output includes state, meaning, next command, and useful guardrail wording.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
git diff --check
```

### Rollback

Revert the doctor classifier, output changes, and tests.

## Work item: benchmark-suggestions

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: first-use product claims
Blocked by:

### Goal

Make benchmark authoring guided instead of blank-page work.

### Production delta

Add reviewable suggestions for:

```text
rust-cli
rust-workspace
node
generic-command
```

Suggestions should be commented and conservative. Heavy or compile-sensitive
commands should remain advisory or non-required until calibrated.

### Non-goals

- Do not auto-commit or silently choose benchmark policy.
- Do not infer every language or framework.

### Acceptance

- Suggested benches include user-facing explanation.
- Suggestions are easy to edit before baseline promotion.
- Tests cover profile-specific output.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert suggestion templates, init wiring, docs, and tests.

## Work item: artifact-explanation

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks: failure-taxonomy
Blocked by:

### Goal

Let users inspect receipt directories without memorizing the architecture.

### Production delta

Add:

```bash
perfgate explain artifacts --out-dir artifacts/perfgate
```

The command should recognize run, compare, report, comment, repair context,
decision, bundle, probe, scenario, and tradeoff artifacts.

### Non-goals

- Do not validate every schema in this command unless a later spec requires it.
- Do not hide unknown files.

### Acceptance

- Known artifacts receive role descriptions.
- Output includes next commands where useful.
- Tests cover common local gate and decision artifact layouts.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features explain
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the command, parser wiring, docs, and tests.

## Work item: failure-taxonomy

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: action-reproduction-block, first-use product claims
Blocked by: adoption-state-doctor

### Goal

Standardize setup, signal, regression, platform, review, and server-upload
failure guidance.

### Production delta

Add shared wording or helpers for:

```text
setup_missing_config
setup_missing_bench
setup_command_failed
missing_baseline
performance_regression
high_noise
unsupported_metric
host_mismatch
review_required
server_upload_failed
```

### Non-goals

- Do not change verdict semantics unless tests prove the intended behavior.
- Do not make optional server upload blocking by default.

### Acceptance

- Each class has status, meaning, artifacts, next command, and do-not guidance.
- CLI and action wording use the same concepts.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert shared wording/helpers and fixture updates.

## Work item: calibration-suggestions

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: first-use product claims
Blocked by:

### Goal

Suggest thresholds and noise policy from receipts without writing config.

### Production delta

Add advisory command:

```bash
perfgate calibrate --config perfgate.toml --bench parser
```

### Non-goals

- No `--write` behavior in this first implementation.
- Do not present suggestions as statistical guarantees.

### Acceptance

- Output includes samples, noise evidence, suggested fail/warn/noise
  thresholds, host context, repeat guidance, and paired-mode guidance when
  unstable.
- The command names evidence used for the suggestion.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features calibrate
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert command wiring, docs, and tests.

## Work item: action-reproduction-block

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: hosted-action-canary, first-use product claims
Blocked by: failure-taxonomy

### Goal

Make local reproduction mandatory in every action summary path.

### Production delta

Extend action summary generation and `xtask action-check` fixtures so every
summary includes verdict, artifacts, local reproduction, setup guidance,
decision guidance when enabled, and signal/platform guidance when relevant.

### Non-goals

- Do not move action aliases.
- Do not make server upload required.

### Acceptance

- `action-check` fails if a summary path omits local reproduction.
- Golden examples cover setup, regression, noise/review, and decision-enabled
  paths.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert action summary changes and fixture expectations.

## Work item: decision-readiness

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks: probe-starter-templates
Blocked by:

### Goal

Make structured decisions pull-based.

### Production delta

Add:

```bash
perfgate decision suggest --config perfgate.toml
```

The command should say whether a simple gate is enough, paired mode is more
appropriate, structured decisions may help, required evidence is missing,
decision evidence is ready to bundle, or optional ledger upload may help.

### Non-goals

- Do not require server mode.
- Do not make structured decisions part of first-hour setup.

### Acceptance

- Tests cover not-ready, simple-gate, paired-mode, decision-candidate, and
  bundle-ready states where fixtures exist.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

### Rollback

Revert command wiring, docs, and tests.

## Work item: probe-starter-templates

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks: first-use product claims
Blocked by: decision-readiness

### Goal

Generate stable probe examples without turning probes into profiling.

### Production delta

Add:

```bash
perfgate probes init --template parser
perfgate probes init --template batch
perfgate probes init --template cli
perfgate probes init --template server
```

Templates should include naming examples, JSONL events, scenario/tradeoff
snippets where useful, artifact paths, and next commands.

### Non-goals

- Do not add broad runtime instrumentation APIs for every language.
- Do not require probes for local checks.

### Acceptance

- Templates are deterministic and tested.
- Output keeps probes framed as tradeoff lenses.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features probe
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert template files, command wiring, docs, and tests.

## Work item: ledger-readiness-doctor

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: first-use product claims
Blocked by:

### Goal

Make optional team ledger readiness explicit.

### Production delta

Add:

```bash
perfgate ledger doctor
```

The output should report local receipt readiness, server URL, API key, project,
upload mode, history reachability, export availability, prune dry-run
availability, and optional-server semantics.

### Non-goals

- Do not make server mode required for local correctness.
- Do not change auth scopes beyond readiness checks.

### Acceptance

- Unconfigured users are told they do not need server mode yet.
- Configured users get actionable missing/ready status.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features ledger
cargo +1.95.0 test -p perfgate-cli --all-features server
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

### Rollback

Revert command wiring, docs, and tests.

## Work item: hosted-action-canary

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: first-use product claims, final closeout
Blocked by: action-reproduction-block

### Goal

Prove hosted external PR behavior, not only local external canaries.

### Production delta

Record an external hosted action canary audit covering external repo PR,
generated workflow, baseline setup, first hosted action run, artifact upload,
local reproduction copied from action output, what confused the user, and what
was fixed.

### Non-goals

- Do not overstate coverage beyond the canary repo and runner shape.
- Do not require server mode.

### Acceptance

- The canary audit distinguishes what it proves from what remains unproven.
- Any required docs/tooling fixes are separately scoped and validated.
- The hosted step-summary shell bug found by the canary is fixed before
  product-claim promotion or lane closeout.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the canary audit and any canary-only docs links.

## Work item: action-step-summary-shell-fix

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: first-use product claims, final closeout
Blocked by: hosted-action-canary

### Goal

Fix the hosted action failure-summary shell issue found by the external canary.

### Production delta

Guard optional summary variables and make Markdown code fences shell-safe in
`action.yml` so hosted failure summaries can render without Bash command
substitution or unbound-variable errors.

### Non-goals

- Do not move action aliases.
- Do not change verdict semantics.
- Do not require server mode.

### Acceptance

- The action failure summary still prints verdict, artifacts, and local
  reproduction.
- Optional decision guidance is omitted safely when decision mode is disabled.
- Hosted shell execution no longer reports `decision_repro_line: unbound
  variable` or attempts to execute Markdown fence text.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the action summary shell changes and any fixture updates.

## Work item: first-use-product-claims

Status: implemented
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR:
Blocks: final closeout
Blocked by: adoption-state-doctor, benchmark-suggestions, artifact-explanation, failure-taxonomy, calibration-suggestions, action-reproduction-block, decision-readiness, probe-starter-templates, ledger-readiness-doctor, hosted-action-canary

### Goal

Map first-use UX promises to support tiers and proof.

### Production delta

Add or update claims for adoption-state doctor, benchmark suggestions, artifact
explanation, calibration suggestions, failure taxonomy, decision readiness,
probe starters, ledger readiness, and hosted external action canary proof.

### Non-goals

- Do not mark unproven behavior supported.
- Do not claim hosted CI coverage beyond the canary evidence.

### Acceptance

- Claims include ID, tier, surface, linked docs/specs/tests or gates, proof
  commands, artifacts where relevant, and `review_after`.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert claim-map entries and dependent links.

## Work item: final-closeout

Status: current
Linked proposal: docs/proposals/PERFGATE-PROP-0005-first-use-intelligence.md
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks:
Blocked by: first-use-product-claims

### Goal

Close the first-use intelligence lane with durable proof and non-inferences.

### Acceptance

- The handoff records covered adoption states, covered failure classes,
  commands added, canary proof, product-claim changes, remaining unproven
  surfaces, and what should happen next.
- If this lane has become the active goal by then, the active goal is archived.
- If the 0.18 release goal is still active, the closeout records that no active
  goal archive was performed for this lane.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the closeout handoff and any lane-specific goal archive.
