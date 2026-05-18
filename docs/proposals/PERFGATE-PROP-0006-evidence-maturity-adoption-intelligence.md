# PERFGATE-PROP-0006: Evidence maturity and adoption intelligence

Status: proposed
Owner: perfgate maintainers
Created: 2026-05-18
Target milestone: 0.19.0
Linked specs: PERFGATE-SPEC-0009-evidence-maturity-contract (planned), PERFGATE-SPEC-0010-agent-repair-context-contract (planned)
Linked ADRs: none
Linked plan: evidence maturity and adoption intelligence implementation plan (planned)
Support/status impact: docs/status/PRODUCT_CLAIMS.md should add or update evidence-maturity, canary-freshness, baseline-trust, and agent repair-context claims only after behavior and proof land
Policy impact: no policy rows by default; server ledger, public surface, generated files, workflow policy, and release proof remain owned by existing policy ledgers and status docs

## Problem

perfgate 0.18 made the product public, credible, and usable. Users can install
the CLI, initialize a repository, choose a first benchmark suggestion, run a
local gate, promote a baseline, wire GitHub Actions, inspect artifacts, use
repair guidance, graduate into structured decisions, and optionally operate a
server ledger. The first-use lane made that path guided instead of
architecture-first.

The next gap appears after week one. A team can run perfgate, but still needs
help answering whether the evidence is mature enough to trust:

```text
is this benchmark a smoke check, advisory signal, or PR gate?
is this baseline mature, stale, host-mismatched, or too new?
is this signal stable enough to enforce?
should this result use paired mode instead of a normal gate?
is this a simple regression, noisy evidence, or a real tradeoff?
what should a reviewer or agent do next without guessing from logs?
which canary and proof claims are fresh enough to rely on?
```

A bad benchmark with a polished gate still creates false confidence. perfgate's
receipts-first model should now classify evidence maturity, not only produce
receipts.

## Users and surfaces

- New teams need benchmark recipes that explain what a workload is best for,
  what it is bad for, expected noise, recommended mode, advisory/blocking
  posture, and paired-mode hints.
- Reviewers need baseline and signal maturity output before treating a result
  as merge-blocking evidence.
- Maintainers need conservative, reviewable recipes and anti-pattern guidance
  so first benchmarks do not become permanent weak gates.
- CI users need failure summaries and local repair context that continue to say
  whether evidence is setup, noise, regression, or tradeoff.
- Advanced users need recognizable structured-decision examples such as
  throughput improvement with memory regression, startup regression with
  steady-state improvement, and noisy evidence where no decision should be
  accepted yet.
- Team operators need server ledger operations that feel production-boring:
  backup, restore, retention, key rotation, export/import, prune dry-run,
  migration compatibility, and larger-history behavior.
- Agents need a repair-context contract that names failure class, artifact
  paths, reproduction command, guardrails, decision options, changed files, and
  host/runtime context without inferring from free-form logs.
- Product maintainers need a canary freshness matrix and proof-age language so
  support claims stay precise as external evidence ages.

## Success criteria

- Benchmark recipes exist for common starting shapes such as Rust CLI smoke,
  Rust workspace advisory, Node command, Python command, HTTP smoke, and generic
  command benchmarks.
- Each recipe explains best use, bad fit, expected noise, recommended mode,
  whether it should block PRs, and when paired mode is likely useful.
- Baseline maturity can be reported without changing receipt schemas or
  promoting baselines automatically.
- Baseline and signal maturity distinguish missing, new, immature, mature,
  stale, host-mismatched, high-noise, advisory-only, safe-to-gate, and
  paired-recommended states.
- Calibration can emit a reviewable config patch or TOML block while remaining
  non-mutating by default.
- Decision examples teach common tradeoff patterns without requiring users to
  learn every scenario, probe, and tradeoff policy concept first.
- `decision suggest` explains why it made a recommendation, including relevant
  metric movement, thresholds, noise, probe evidence, and next commands.
- Canary evidence is tracked in a matrix with repo shape, last run, proof
  artifact, what it proves, what it does not prove, and freshness status.
- Server ledger backup/restore and retention behavior are smoked while keeping
  local receipts as the correctness contract.
- Agent repair-context behavior is documented and fixture-backed for missing
  baseline, regression, high noise, host mismatch, decision candidate, and
  server upload failure paths.
- Product claims are updated only after behavior and evidence land, with
  freshness tiers where appropriate.
- The lane closes with a handoff that records what evidence maturity states are
  covered, what remains advisory, what remains unproven, and what should happen
  next.

## Proposed shape

Add a 0.19 evidence-maturity lane that layers classification, explanation, and
freshness over the receipts perfgate already produces:

```text
benchmark recipe
baseline maturity
signal maturity
calibration patch
decision explanation
canary freshness
server operations proof
agent repair context
```

The lane should start with behavior contracts and reviewable suggestions, then
add narrow implementation slices. The goal is not to make benchmark selection
magical. Suggestions should remain explicit, editable, and conservative.

Benchmark recipe output should use stable metadata:

```text
Best for
Bad for
Expected noise
Recommended mode
Advisory vs blocking
Paired-mode hint
```

Baseline and signal maturity should be advisory at first. They can recommend
gate, advisory, paired mode, more samples, or recalibration, but they should not
promote baselines, loosen thresholds, or rewrite policy automatically.

Decision examples and `decision suggest` should turn structured decisions into
a recognizable review pattern:

```text
throughput improved, memory regressed
startup regressed, steady-state improved
latency worsened, batch time improved
probe regressed, dominant workload improved
noise too high, no decision yet
```

Server work should keep the established boundary:

```text
local receipts = correctness
server ledger = optional team history
```

Agent repair context should become a documented contract for safe automation.
The key guardrails are explicit: do not fix missing baselines by loosening
thresholds, do not promote blindly, rerun or pair noisy evidence, and generate a
decision only when the evidence points to a tradeoff.

## Alternatives considered

### Add another benchmark engine

Rejected. perfgate should sit above Criterion, hyperfine, pytest-benchmark,
k6, custom scripts, and project-specific benches. Its distinctive value is
consistent, portable, reviewable evidence, not replacing measurement tools.

### Make server ledger required

Rejected. Server ledger mode remains optional team history. Local receipts,
action summaries, repair context, and decision bundles remain the correctness
contract.

### Build a dashboard before richer evidence semantics

Rejected. A dashboard would only make immature signals easier to browse. The
product needs evidence maturity, baseline trust, canary freshness, and repair
contracts before more visual surfaces.

### Expand public crates

Rejected. The five public crates remain the stable contract surface. This lane
should add CLI/server behavior and docs without creating new package contracts.

### Over-automate benchmark selection

Rejected. Benchmark choice is product judgment and team context. perfgate should
generate reviewable recipes and comments, not silently pick blocking policy.

### Add heavy policy mutation before maturity is explicit

Rejected. Policy changes should follow mature evidence. The lane should first
make baseline and signal maturity visible.

## Specs to create or update

- `PERFGATE-SPEC-0009-evidence-maturity-contract` should define benchmark
  recipe metadata, maturity vocabulary, baseline doctor behavior, signal doctor
  behavior, calibration patch output, decision explanation rules, canary matrix
  fields, server backup/restore proof boundaries, and proof freshness tiers.
- `PERFGATE-SPEC-0010-agent-repair-context-contract` should define what agents
  may rely on in repair context: failure class, artifact paths, local
  reproduction, baseline-promotion guard, decision suggestion, do-not guidance,
  changed-files summary, host/runtime context, and server upload status.
- Update `PERFGATE-SPEC-0008-first-use-ux-contract` only if week-one maturity
  behavior changes first-use command contracts.
- Update `PERFGATE-SPEC-0003-performance-decision-contract` only if decision
  receipts, tradeoff semantics, or decision bundle contracts change.
- Update `PERFGATE-SPEC-0005-release-proof-contract` only if release proof
  starts to require canary freshness or public-install canary matrices.

## Architecture decisions needed

No ADR is required at lane start. Existing ADRs already cover the durable
boundaries:

- receipts-first performance decisions;
- public crates as contracts and modules as architecture boundaries;
- local receipts first and server ledger optional.

Create an ADR only if implementation changes those boundaries, such as making
canary freshness a release gate, promoting server history into correctness, or
creating a new public contract surface.

## Evidence plan

Proposal, spec, plan, and docs/status PRs should run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Behavior PRs should add focused tests for the changed surface before broadening
validation. Expected targeted gates include:

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

Cross-cutting implementation should also run:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

Canary and proof-freshness work should record what each proof does not prove.
External canaries should not become implicit release gates until the spec and
maintainers explicitly promote them.

## Risks

- Recipe comments can look more authoritative than intended and cause weak
  workloads to become permanent blocking gates.
- Baseline maturity labels can create false precision if sample count, host
  drift, and noise evidence are not visible.
- Signal doctor output can become noisy if it reports every receipt fact without
  a clear recommendation.
- Calibration patch output can be mistaken for a guarantee unless it names the
  evidence and when not to apply it.
- Decision examples can drift from actual CLI behavior if they are not backed
  by fixtures.
- Canary matrices can go stale unless freshness is explicit.
- Server backup/restore tests can accidentally imply server mode is part of
  correctness unless the optional boundary remains prominent.
- Agent repair context can encourage unsafe automation if guardrails are vague.

## Non-goals

- Do not reopen 0.18 publication, first-use UX, SRP hardening, decision
  semantics, post-SRP coverage, wrapper absorption, or source-of-truth
  governance.
- Do not add a new benchmark engine.
- Do not make server ledger mode required for correctness.
- Do not build a dashboard in this lane.
- Do not expand the five public crates.
- Do not change receipt schemas casually; schema changes need a spec and
  compatibility proof.
- Do not rename user-facing commands or artifact names without an explicit spec.
- Do not silently auto-promote baselines, auto-loosen thresholds, or auto-write
  benchmark policy.
- Do not make external canaries mandatory CI before freshness policy is defined.

## Exit criteria

This proposal is complete when:

- the evidence maturity contract spec exists and is accepted;
- the agent repair-context contract exists or is explicitly deferred with
  rationale;
- a 0.19 implementation plan sequences the lane into PR-sized changes;
- benchmark recipes include maturity metadata and anti-pattern guidance;
- baseline and signal doctor output classify trust and gating suitability;
- calibration can emit a reviewable non-mutating config patch;
- decision examples and `decision suggest` explain recognizable tradeoff
  patterns;
- canary freshness is tracked in a durable matrix;
- server ledger backup/restore or equivalent operational proof is recorded;
- repair-context fixtures cover common reviewer and agent decision paths;
- product claims link only implemented and proven behavior;
- a closeout records what became easier after week one, what remains advisory,
  what remains unproven, and the next recommended lane.
