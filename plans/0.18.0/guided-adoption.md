# perfgate 0.18.0 Guided Adoption Implementation Plan

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Current PR: complete
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked specs: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADRs: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md; local server-ledger optionality ADR planned if the lane changes that boundary
Linked policy: policy ledgers remain referenced by specs and status docs; no policy row changes in this plan PR
Support/status impact: guided adoption product claims planned for docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check
Blocks: none
Blocked by: none
Rollback: revert the closeout handoff, plan status update, and archived goal manifest; already merged proposal/spec/docs remain valid

## Goal

Make perfgate easy for cold users and teams to adopt in stages: install,
doctor, init, check, promote, wire CI, understand failures, adopt structured
decisions, add probes, and optionally operate a decision ledger.

This plan sequences the work. Behavior is owned by
[`PERFGATE-SPEC-0007-guided-adoption-contract`](../../docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md).

## Operating Rules

- Keep one semantic artifact or narrow product delta per PR.
- Local receipts remain the primary correctness contract.
- Server ledger mode remains optional team infrastructure.
- Probes explain tradeoffs; they are not the basic local gate and they are not
  a profiling surface.
- Product claims must link to proof commands, specs, docs, tests, examples, or
  policy gates.
- Do not duplicate release-readiness matrices or policy ledgers in this plan.
- Do not reduce the five public crates in this lane.

## PR Sequence

| PR | Work item | Status | Files |
|----|-----------|--------|-------|
| 372 | First-hour adoption guide | merged | `docs/FIRST_HOUR.md`, README/docs links |
| 373 | Adoption levels | merged | `docs/ADOPTION_LEVELS.md`, README/docs links |
| 374 | Guided adoption proposal | merged | `docs/proposals/PERFGATE-PROP-0002-guided-adoption.md` |
| 375 | Guided adoption contract spec | merged | `docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md` |
| 376 | Guided adoption plan and active goal | merged | `plans/0.18.0/guided-adoption.md`, `.codex/goals/active.toml` |
| 377 | Decision outcome gallery | merged | `docs/examples/decision-outcomes.md`, `examples/performance-decision/outcomes/` |
| 378 | First-hour adoption smoke fixture | merged | CLI tests and fixtures |
| 379 | Probe instrumentation quickstart | merged | `docs/PROBE_QUICKSTART.md`, README/docs/example links |
| 380 | Probe-to-decision proof | merged | CLI tests or deterministic fixtures |
| 381 | GitHub Action failure UX | merged | action output, tests, docs |
| 382 | Server ledger operations runbook | merged | server docs/runbook |
| 383 | Guided adoption product claims | merged | `docs/status/PRODUCT_CLAIMS.md` |
| 384 | Claim/spec freshness checker | merged | `xtask` checker and tests |
| 385 | Policy ledger contract spec | merged | `docs/specs/PERFGATE-SPEC-0006-policy-ledger-contracts.md` |
| 386 | Wrapper-crate cleanup plan | merged | `plans/0.18.0/wrapper-crate-cleanup.md` |
| final | Guided adoption closeout | current | handoff and archived active goal |

## Work item: first-hour-guide

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR:
Blocks: first-hour-smoke-proof, guided-adoption-claims
Blocked by:

### Goal

Teach cold users the install, init, local check, baseline promotion, commit, CI,
and failure reproduction path.

### Acceptance

- Users can identify generated files, durable files, and temporary artifacts.
- The local reproduction command is explicit.
- Server mode and structured decisions remain optional.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the guide and links. Product behavior remains unchanged.

## Work item: adoption-levels

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR:
Blocks: decision-outcome-gallery, server-ledger-runbook
Blocked by:

### Goal

Teach the progressive ladder: local gate, GitHub Action gate, structured
decision, and server ledger.

### Acceptance

- Each level names users, commands, config, artifacts, failure examples, and
  next steps.
- The server remains optional.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the guide and links. Product behavior remains unchanged.

## Work item: guided-adoption-plan-and-active-goal

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR:
Blocks: remaining guided adoption PRs
Blocked by:

### Goal

Make the adoption lane executable by humans and Codex from repo files.

### Acceptance

- This plan sequences PR-sized work.
- `.codex/goals/active.toml` parses as TOML.
- The active goal names current work, allowed files, forbidden files, proof
  commands, and completion criteria.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert this plan and `.codex/goals/active.toml`.

## Work item: decision-outcome-gallery

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks: guided-adoption-claims
Blocked by: guided-adoption-plan-and-active-goal

### Goal

Give users pattern recognition for decision outcomes.

### Production delta

Add examples for pass, fail, warn with accepted tradeoff, review-required,
missing evidence, and high noise.

### Non-goals

- Do not change decision behavior.
- Do not add server requirements.

### Acceptance

- Each example names scenario, input receipts, decision excerpt, action summary
  excerpt when relevant, reviewer action, and local reproduction command.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the examples and links.

## Work item: first-hour-smoke-proof

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR:
Blocks: guided-adoption-claims
Blocked by: guided-adoption-plan-and-active-goal

### Goal

Prove the first-hour path from a generated cold project.

### Production delta

Add or extend CLI tests for doctor, init, check, baseline status, baseline
promote, and `--require-baseline`.

### Non-goals

- Do not perform crates.io install in ordinary CI.
- Do not require server mode.

### Acceptance

- Generated project contains `perfgate.toml`, `.github/workflows/perfgate.yml`,
  `baselines/`, and `.perfgate/README.md`.
- A check can run, a baseline can be promoted, and a require-baseline check can
  run afterward.
- Failure copy includes a local reproduction command where applicable.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features first_run
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the fixture/test changes.

## Work item: probe-quickstart

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks: probe-to-decision-proof, guided-adoption-claims
Blocked by: guided-adoption-plan-and-active-goal

### Goal

Teach probes as tradeoff lenses, not profiling.

### Production delta

Add a probe quickstart covering minimal JSONL, Rust helper, tracing/Criterion
adapter paths, ingest, compare, and decision wiring.

### Non-goals

- Do not make probes required for basic checks.
- Do not add a new public crate.

### Acceptance

- A user can add one probe without server setup.
- A user can compare probe evidence.
- A user can attach probe evidence to `decision evaluate`.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the quickstart and links.

## Work item: probe-to-decision-proof

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks: guided-adoption-claims
Blocked by: probe-quickstart

### Goal

Turn the probe quickstart path into executable proof.

### Production delta

Add or extend tests so probe emission or JSONL, ingest, probe compare, decision
evaluate, and decision bundle are covered together.

### Non-goals

- Do not require a server.
- Do not change receipt schema versions.

### Acceptance

- The proof starts from probe evidence and ends with decision artifacts.
- The bundle path is covered.

### Proof commands

```bash
cargo +1.95.0 test -p perfgate-cli --all-features probe
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

### Rollback

Revert the tests/fixtures.

## Work item: action-failure-copy

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR:
Blocks: guided-adoption-claims
Blocked by: guided-adoption-plan-and-active-goal

### Goal

Make CI failures explain themselves.

### Production delta

Improve action summary or logs so failures show verdict, failed metric or setup
condition, artifacts, local reproduction command, baseline bootstrap hint,
decision command when enabled, review-required behavior, and uploaded artifact
list.

### Non-goals

- Do not make server upload required.
- Do not change branch protection semantics beyond existing action inputs.

### Acceptance

- Action-check covers the updated summary/failure copy.
- Local reproduction is visible for standard and decision-enabled gates.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert action output changes and fixtures.

## Work item: server-ledger-runbook

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR: local server-ledger optionality ADR planned if the lane changes that boundary
Blocks: guided-adoption-claims
Blocked by: guided-adoption-plan-and-active-goal

### Goal

Make optional team ledger mode operationally understandable.

### Production delta

Add a runbook covering local SQLite mode, team server mode, API key lifecycle,
decision upload, history/latest, export, prune, debt, audit, backups, CI upload
failure behavior, and dashboard expectations.

### Non-goals

- Do not make the server mandatory.
- Do not change auth behavior unless a separate implementation PR requires it.

### Acceptance

- A team can decide whether to use server mode.
- A team can run ledger mode without making it required for local correctness.
- Export and prune are explained safely.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the runbook and links.

## Work item: guided-adoption-claims

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR:
Blocks: claim-spec-freshness-check, final closeout
Blocked by: first-hour-smoke-proof, probe-to-decision-proof

### Goal

Map guided adoption claims to support tiers and proof.

### Production delta

Add claims for first-hour local adoption, staged adoption levels, probe-backed
tradeoff explanation, and optional team decision-ledger operations.

### Non-goals

- Do not mark unproven surfaces stable.
- Do not copy specs or test bodies into the claim map.

### Acceptance

- Each claim has ID, tier, surface, linked docs/specs/tests or gates, proof
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

Revert claim-map entries and any links that rely on them.

## Work item: claim-spec-freshness-check

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR:
Blocks: final closeout
Blocked by: guided-adoption-claims

### Goal

Prevent stale planned spec links once concrete spec files exist.

### Production delta

Extend `product-claims-check` or a nearby xtask docs checker so a
`PERFGATE-SPEC-XXXX ... planned` reference fails if
`docs/specs/PERFGATE-SPEC-XXXX-*.md` exists.

### Non-goals

- Do not build a full semantic graph checker.
- Do not require every claim to have every possible spec.

### Acceptance

- The narrow stale planned-spec rule is tested.
- Existing claims pass.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 test -p xtask --all-features
cargo +1.95.0 clippy -p xtask --all-targets --all-features -- -D warnings
git diff --check
```

### Rollback

Revert the checker and tests.

## Work item: policy-ledger-contract-spec

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0006-policy-ledger-contracts.md
Linked ADR:
Blocks: final closeout
Blocked by:

### Goal

Close the remaining named spec-governance follow-up for policy ledgers.

### Production delta

Add `docs/specs/PERFGATE-SPEC-0006-policy-ledger-contracts.md`.

### Non-goals

- Do not copy policy rows into the spec.
- Do not change policy allowlists in the spec PR.

### Acceptance

- The spec states that policy ledgers own concrete exceptions.
- Specs link to ledgers instead of copying rows.
- Allowlists and generated baselines have owner/reason/review expectations
  where applicable.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the spec.

## Work item: wrapper-crate-cleanup-plan

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked spec: docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md
Linked ADR: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Blocks: wrapper absorption batches
Blocked by:

### Goal

Plan remaining compatibility wrapper absorption without moving crates in the
planning PR.

### Production delta

Document remaining wrappers, owner modules, dev/test-only classifications,
delete candidates, dependency cleanup, batch order, and proof commands.

### Non-goals

- Do not move, delete, or reclassify crates in the planning PR.
- Do not reduce the five public crates.

### Acceptance

- Every remaining wrapper has a disposition.
- Each implementation batch has proof commands.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the plan.

## Work item: final-closeout

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked spec: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Blocks:
Blocked by:

### Goal

Close the guided adoption lane with proof and archived agent state.

### Acceptance

- Proposal, spec, plan, product claims, docs, proof PRs, and runbook are merged
  or explicitly deferred.
- `.codex/goals/active.toml` is archived.
- A handoff records changed surfaces, proof commands, user paths now supported,
  and remaining deferred work.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

### Rollback

Revert the closeout handoff and archived goal manifest only.
