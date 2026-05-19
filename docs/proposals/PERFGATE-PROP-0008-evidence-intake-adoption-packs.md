# PERFGATE-PROP-0008: Evidence intake and adoption packs

Status: proposed
Owner: perfgate maintainers
Created: 2026-05-19
Target milestone: 0.21.0
Linked specs: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Linked ADRs: none
Linked plan: plans/0.21.0/evidence-intake-adoption-packs.md
Support/status impact: docs/status/PRODUCT_CLAIMS.md and docs/status/CANARY_MATRIX.md should be updated only after adapters, adoption packs, tests, and external canaries land
Policy impact: no policy rows by default; imported evidence must remain advisory until normal maturity and policy promotion surfaces prove it is safe to review

## Problem

perfgate is now public, evidence-aware, and policy-aware. Teams can run checks,
inspect maturity, emit calibration and policy patches, generate review packets,
surface policy posture in GitHub Actions, and keep agents inside review
boundaries.

The next adoption blocker is that many real teams already have benchmark
ecosystems:

```text
Criterion
hyperfine
pytest-benchmark
k6
shell commands
custom JSON or CSV outputs
project-specific scripts
```

Those teams should not need to rewrite their measurement layer to adopt
perfgate. They need a safe intake path from existing measurements into
perfgate's receipts, maturity, policy, review, and Action surfaces.

The product risk is overreach. perfgate should not become another benchmark
engine, scheduler, dashboard, or profiler. Its lane is evidence and policy:

```text
external tools measure
perfgate normalizes evidence
receipts preserve review context
maturity classifies trust
policy decides advisory versus promotion
review packets explain the next step
CI summarizes the same evidence
```

## Users and surfaces

- Rust maintainers with Criterion or existing command benches need import
  paths that preserve measurement semantics without changing benchmark code.
- Python service teams using `pytest-benchmark` need JSON results mapped into
  receipts, maturity, and review packets.
- Node tool and GitHub Action maintainers need command, hyperfine, or custom
  JSON adapters that do not assume Rust.
- HTTP service teams using k6 or scripted smoke checks need explicit unit,
  direction, and non-inference guidance.
- Reviewers need imported evidence to say what units and directions were
  inferred, what source metadata was preserved, and what must be reviewed.
- CI users need adapter output to reach the same Action posture and artifact
  surfaces as native perfgate checks.
- Agents need adapter output and review packets that tell them what they may
  inspect or propose, not permission to rewrite policy or benchmark tools.

## Success criteria

- An evidence-source contract defines source kind, metric mapping, unit
  normalization, metric direction, sample model, host context, noise support,
  baseline compatibility, adapter metadata, and non-inferences.
- Adapter behavior prefers mapping external tool output into existing perfgate
  receipt shapes or isolated adapter metadata.
- Adapters report what they prove, what they cannot prove, what units and
  directions they inferred, and what the user must review.
- Generic command JSON import exists before tool-specific adapters.
- hyperfine JSON import exists with fixture-backed unit, direction, and sample
  mapping.
- Criterion output import exists for the parts of Criterion's model that are
  already close to perfgate's run/sample model.
- pytest-benchmark JSON import exists with explicit unit and host/context
  limitations.
- k6 summary JSON import exists with explicit HTTP/load-test non-inferences.
- Each adapter has fixtures, clear errors, doc examples, and at least one
  first-hour recipe showing how imported evidence reaches policy doctor and
  review-packet output.
- Adoption packs exist for Rust CLI, Rust workspace, Python service, Node
  tool/action, HTTP local smoke, and generic command repos.
- Each adoption pack names benchmark source, expected artifact path, suggested
  policy posture, known bad fits, Action snippet, local reproduction command,
  and promotion path.
- At least two external canaries prove the adoption path:
  one Rust existing-benchmark repo and one non-Rust command or HTTP repo.
- Canaries record import, baseline, check, policy doctor, review packet, and
  Action summary posture from public or normal repo workflows.

## Proposed shape

Add a 0.21 evidence-intake and adoption-packs lane that sits above existing
benchmark tools:

```text
existing benchmark output
  -> adapter mapping
  -> perfgate run/compare/report evidence
  -> maturity and policy posture
  -> review packet and Action summary
```

The lane should start with source-of-truth artifacts:

- this proposal, defining why intake comes after evidence maturity and policy
  ergonomics;
- an evidence-source contract spec that defines adapter behavior and proof;
- an implementation plan that sequences adapters from lowest to highest risk;
- status mappings only after behavior and proof exist; and
- canary records only after real external repo paths run.

Adapters should be transparent. A user should be able to inspect the imported
receipt or adapter output and understand:

```text
source kind
source artifact path
metric name
unit normalization
direction
sample count
host context
noise evidence
baseline compatibility
what was inferred
what was not inferred
what needs review before gating
```

Imported evidence should flow through existing surfaces wherever possible:

```text
baseline doctor
doctor signal
policy doctor
policy emit-patch
policy review-packet
GitHub Action summary
repair context
product claims
canary freshness
```

## Adapter sequence

Implement adapters in increasing semantic risk:

1. Generic command JSON.
   - Lowest product risk because the user controls the shape.
   - Requires explicit metric/unit/direction mapping.
   - Should produce strong errors for missing fields and ambiguous units.
2. hyperfine JSON.
   - Natural command benchmark fit.
   - Should preserve mean/median/stddev/user/system/raw runs where available.
   - Must explain that command timing may include setup or shell overhead.
3. Criterion output.
   - Natural Rust benchmark fit, but rich statistics can be easy to overstate.
   - Import only stable fields that map clearly into perfgate evidence.
   - Do not pretend Criterion confidence intervals are the same as perfgate
     maturity policy unless the spec defines the mapping.
4. pytest-benchmark JSON.
   - Useful for Python services, with explicit interpreter/environment limits.
   - Must separate correctness test suite success from performance evidence.
5. k6 summary JSON.
   - Useful for HTTP and load-test smoke, with explicit environment limits.
   - Must not infer production service capacity from local or shared-runner
     smoke output.

Each adapter should have:

```text
fixtures
positive import test
bad input tests
unit and direction tests
sample/noise tests where source supports it
doc example
first-hour recipe
policy doctor or review-packet proof
non-inference text
```

## Adoption packs

Adoption packs should be reviewable templates, not magic detection. Initial
packs should cover:

```text
rust-cli
rust-workspace
python-service
node-tool-action
http-local-smoke
generic-command
```

Each pack should include:

```text
benchmark source
expected artifact path
adapter command
suggested starting posture
known bad fits
Action snippet
local reproduction command
baseline path
promotion path
what not to infer
```

Packs should help a team keep existing benchmark tools while adding perfgate on
top. They should not silently choose a benchmark, promote a baseline, make a
gate blocking, loosen thresholds, or require server ledger mode.

## External canaries

The lane needs at least two real external canaries:

- one Rust repo with an existing benchmark source such as Criterion, hyperfine,
  or a command fixture; and
- one non-Rust command or HTTP repo with existing JSON, CSV, pytest-benchmark,
  k6, or command output.

Each canary should record:

```text
repo shape
existing benchmark source
adapter command
imported evidence artifact
baseline path
check or compare command
baseline doctor output
signal doctor output
policy doctor output
review packet output
Action summary posture when hosted CI is part of the canary
what confused the user
what was fixed
what it proves
what it does not prove
freshness state
```

Canaries should use public or normal repo workflows where practical. Source-built
canaries must not be cited as public install proof.

## Alternatives considered

### Build native benchmark runners for every ecosystem

Rejected. That would turn perfgate into a benchmark engine and compete with
Criterion, hyperfine, pytest-benchmark, k6, and project-specific scripts. The
right layer is evidence normalization and review policy.

### Require users to rewrite benchmarks as perfgate commands

Rejected. This blocks teams that already invested in benchmark tooling. The
adoption path should let existing tools stay in place.

### Add a dashboard for imported evidence first

Rejected. A dashboard would make imported results easier to browse, but the
first product need is trustworthy intake, receipts, maturity, policy posture,
and CI review.

### Add broad schema churn up front

Rejected. Start by mapping into existing receipt shapes or isolated adapter
metadata. Add a versioned schema only if the evidence-source contract proves
existing receipts cannot carry required review context safely.

### Auto-map ambiguous metrics

Rejected. Adapters may infer only when the source semantics are clear and the
output says what was inferred. Ambiguous unit or direction mapping should ask
for explicit user configuration.

## Specs to create or update

- `PERFGATE-SPEC-0013-evidence-source-contract` should define source kinds,
  adapter metadata, metric mapping, units, directions, sample models, host
  context, noise support, baseline compatibility, non-inferences, acceptance
  examples, fixtures, and error behavior.
- Update `PERFGATE-SPEC-0009-evidence-maturity-contract` only if imported
  evidence needs new maturity vocabulary.
- Update `PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract` only if
  imported evidence needs policy-specific promotion requirements beyond the
  existing advisory-to-blocking path.
- Update `PERFGATE-SPEC-0012-agent-policy-change-guardrails` only if adapter
  output creates new agent-specific review risks.
- Update receipt schemas only after the evidence-source contract proves a
  versioned schema change is necessary.

## Architecture decisions needed

No ADR is required at lane start. Existing ADRs already define the durable
boundaries:

- perfgate is receipts-first;
- local receipts are the correctness contract; and
- server ledger mode is optional team history.

Create an ADR only if implementation changes those boundaries, such as adding
a new public adapter crate, making adapter metadata a stable public schema, or
turning imported evidence into a separate source of correctness.

## Evidence plan

Proposal, spec, plan, and status PRs should run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Adapter PRs should add focused fixtures and tests before broader proof:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

Action/adoption-pack PRs should also run:

```bash
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
```

Cross-cutting implementation should prove public and architecture boundaries:

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

External canary PRs should record the exact commands, source artifacts,
imported receipts, Action runs when applicable, and non-inferences.

## Risks

- Adapter output can overstate what external measurements prove.
- Unit or direction inference can silently invert improvement and regression.
- Imported evidence can bypass maturity and policy review if the path is too
  magical.
- Tool-specific adapters can drift when upstream JSON formats change.
- k6 and HTTP smoke results can sound like production capacity evidence.
- Criterion and pytest-benchmark statistics can be misrepresented if perfgate
  collapses richer source models into simpler receipts without explanation.
- Adoption packs can look like universal best practices instead of starting
  points for review.
- Agents can treat adapter errors as permission to loosen thresholds or mutate
  baselines unless review packets preserve do-not guidance.

## Non-goals

- Do not add another benchmark engine.
- Do not add a benchmark scheduler.
- Do not add a dashboard.
- Do not expand public crates by default.
- Do not require server ledger mode.
- Do not auto-promote baselines.
- Do not auto-loosen thresholds.
- Do not make imported evidence blocking by default.
- Do not broadly change receipt schemas without an accepted spec.
- Do not reopen the 0.18 release lane, 0.19 evidence-maturity lane, or 0.20
  policy-ergonomics lane unless a real regression appears.
- Do not treat generated badge PRs as part of adapter semantics.

## Exit criteria

This proposal is complete when:

- the evidence-source contract spec exists and is accepted;
- a 0.21 implementation plan sequences adapters, adoption packs, status
  updates, canaries, and closeout into PR-sized changes;
- generic command JSON, hyperfine JSON, Criterion, pytest-benchmark, and k6
  summary adapters exist with fixtures and error tests;
- each adapter documents units, directions, sample model, host context, what it
  proves, and what it does not prove;
- imported evidence reaches baseline doctor, signal doctor, policy doctor,
  policy review-packet, and Action posture surfaces;
- adoption packs exist for Rust CLI, Rust workspace, Python service, Node
  tool/action, HTTP local smoke, and generic command repos;
- at least two external canaries prove the intake-to-review path across Rust
  and non-Rust repo shapes;
- product claims and canary freshness map adapter support without overclaiming;
  and
- a closeout records what teams can now adopt without rewriting benchmarks,
  what remains advisory, what agents cannot change, and what remains unproven.
