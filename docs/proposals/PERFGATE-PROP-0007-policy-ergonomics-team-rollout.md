# PERFGATE-PROP-0007: Policy ergonomics and team rollout

Status: proposed
Owner: perfgate maintainers
Created: 2026-05-18
Target milestone: 0.20.0
Linked specs: PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract (planned), PERFGATE-SPEC-0012-agent-policy-change-guardrails (planned)
Linked ADRs: none
Linked plan: policy ergonomics and team rollout implementation plan (planned)
Support/status impact: docs/status/PRODUCT_CLAIMS.md and docs/status/PROOF_FRESHNESS.md should be updated only after policy ergonomics behavior and proof land
Policy impact: no policy rows by default; this lane defines reviewable rollout guidance before promoting maturity output into blocking policy

## Problem

perfgate 0.18 made the tool public, installable, and credible. The 0.19
evidence-maturity lane made the tool useful after the first hour by showing
whether benchmarks, baselines, signals, decisions, canaries, and repair context
are trustworthy enough to reason about.

The next team problem is policy adoption. Once perfgate can say evidence is
mature, a team still needs to answer:

```text
which benchmarks should stay advisory?
which benchmarks can become blocking gates?
what evidence is required before promotion?
what config patch changes policy?
what does a reviewer approve?
what should CI say before blocking?
what should agents never change without review?
which proof is fresh enough to support a promoted claim?
```

Without a guided rollout model, teams can create brittle performance gates:
compile-heavy smoke checks become required, noisy benchmarks block PRs, missing
baselines get "fixed" by loosening thresholds, and agents may weaken policy to
make red checks go away.

perfgate should make the safe path obvious:

```text
advisory first
prove maturity
promote deliberately
block only when evidence is stable
explain failures clearly
keep escape hatches reviewed
keep agents from weakening policy
```

## Users and surfaces

- Team maintainers need repo-shape policy profiles that describe starting
  posture, promotion requirements, default evidence expectations, and known bad
  fits without silently changing policy.
- Reviewers need a compact review packet that gathers verdict, baseline
  maturity, signal confidence, calibration status, decision readiness, proof
  freshness, next command, and do-not guidance.
- CI users need GitHub Action summaries that distinguish blocking gates from
  advisory signals, maturity warnings, promotion candidates, and review-required
  policy changes.
- Agents need policy-change guardrails that separate allowed inspection and
  suggestion work from changes that require human review.
- Maintainers need product claims and canaries to enforce proof freshness before
  a claim or policy posture is promoted.

## Success criteria

- The policy promotion contract defines rollout states:
  `smoke`, `advisory`, `gate_candidate`, `required_gate`, `quarantined`, and
  `retired`.
- Promotion requirements are explicit: mature baseline, stable signal, host
  compatibility, reviewed calibration, paired mode considered, reviewer
  approval, and proof freshness.
- Repo-shape policy profiles exist for conservative starting points such as
  `rust-cli-standard`, `rust-workspace-advisory`,
  `node-command-advisory`, `python-command-advisory`, `http-local-smoke`, and
  `generic-command-advisory`.
- Policy profiles describe what they gate, what stays advisory, required
  evidence before promotion, failure meaning, and what not to infer.
- `perfgate policy doctor --config perfgate.toml` or an equivalent surface can
  report promotion readiness without changing config.
- `perfgate policy emit-patch --config perfgate.toml --bench parser --to
  gate_candidate` or an equivalent surface emits a reviewable non-mutating TOML
  patch plus reasons.
- Reports or comments include a compact performance review packet with verdict,
  maturity, signal confidence, baseline health, decision readiness, proof
  freshness, next command, and do-not guidance.
- GitHub Action summaries surface maturity and policy posture without making
  advisory evidence block unless policy explicitly says so.
- Agent policy guardrails define what agents may do, what requires explicit
  review, and what is forbidden by default.
- Proof freshness rules prevent stale or unproven evidence from supporting
  promoted claims or policy posture.
- At least one external canary records advisory check, maturity review,
  promotion doctor output, review packet, and Action summary behavior.
- The lane closes with a handoff that records what became governable, what
  stayed advisory, and what remains unproven.

## Proposed shape

Add a 0.20 policy ergonomics lane that sits on top of 0.19 evidence maturity:

```text
evidence maturity -> promotion readiness -> reviewable policy patch
                  -> review packet -> action posture -> agent guardrails
```

The lane should start with source-of-truth artifacts:

- a proposal that defines why policy ergonomics comes after evidence maturity;
- a promotion contract spec that defines rollout states and transition
  requirements;
- an implementation plan that sequences small, reversible PRs; and
- a later agent policy guardrail spec for agent-safe policy changes.

Implementation should stay non-mutating by default. perfgate can suggest a
profile, report readiness, and print a policy patch, but it should not silently
promote baselines, loosen thresholds, make mature evidence blocking, require
server ledger mode, or write policy without explicit review.

## Rollout states

The initial vocabulary should be:

| State | Meaning |
|-------|---------|
| `smoke` | Useful for quick feedback, setup proof, or first-hour confidence; not suitable as a required PR gate by itself. |
| `advisory` | Useful review evidence, but not yet stable enough or important enough to block. |
| `gate_candidate` | Evidence appears mature enough to review for blocking policy. |
| `required_gate` | Explicitly reviewed and approved to block PRs. |
| `quarantined` | Temporarily removed from enforcement because evidence, host, or benchmark behavior is not trustworthy. |
| `retired` | No longer part of active performance policy. |

Promotion should be one benchmark at a time. A broad profile can suggest
starting posture, but each required gate should have its own evidence trail.

## Profile catalog

The profile catalog should be metadata first, not behavior-changing defaults:

```text
rust-cli-standard
rust-workspace-advisory
node-command-advisory
python-command-advisory
http-local-smoke
generic-command-advisory
agent-heavy-repo
server-ledger-optional
```

Each profile should record:

```text
starting posture
promotion requirements
default evidence expectations
known bad fits
failure meaning
what not to infer
```

Profiles should help teams choose policy posture. They should not silently
select a benchmark, promote a baseline, or make a check blocking.

## Agent policy guardrails

Agents should be able to:

- rerun perfgate commands;
- inspect artifacts;
- summarize failure and maturity evidence;
- suggest paired mode;
- propose config or policy patches; and
- open PRs that require review.

Agents should require explicit human review before they:

- promote a baseline;
- make a benchmark blocking;
- loosen thresholds;
- accept a tradeoff;
- change a policy profile;
- quarantine or retire a gate; or
- require server ledger mode.

Agent guardrails should be fixture-backed. The goal is to prevent automation
from weakening the evidence contract while still letting agents do useful
repair and review-prep work.

## Alternatives considered

### Make mature benchmarks blocking automatically

Rejected. Maturity is necessary evidence, not team approval. A benchmark can be
stable but still not important enough to block PRs.

### Add a dashboard first

Rejected. A dashboard would make policy status easier to browse, but the
product needs rollout semantics, review packets, and agent guardrails before a
larger visual surface.

### Require the server ledger for policy history

Rejected. Local receipts remain the correctness contract. The server ledger is
optional team history and should not be required for policy rollout.

### Expand public crates for policy modules

Rejected. The five public crates remain the contract surface. Policy ergonomics
should start as CLI/docs/module behavior inside existing crates.

### Auto-write promotion patches

Rejected. Policy changes should be reviewable. The first implementation should
emit patches and reasons, not silently mutate config.

## Specs to create or update

- `PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract` should define
  rollout states, transition requirements, policy profile metadata, promotion
  doctor behavior, policy patch output, review packet fields, action summary
  posture, and proof freshness expectations.
- `PERFGATE-SPEC-0012-agent-policy-change-guardrails` should define what
  agents may do, what requires review, and what remains forbidden by default
  when policy is involved.
- Update `PERFGATE-SPEC-0009-evidence-maturity-contract` only if maturity
  classifications need new fields or behavior.
- Update `PERFGATE-SPEC-0010-agent-repair-context-contract` only if repair
  context begins carrying policy-specific fields.
- Update `PERFGATE-SPEC-0005-release-proof-contract` only if proof freshness or
  policy canaries become release requirements.

## Architecture decisions needed

No ADR is required at lane start. Existing ADRs already cover the durable
boundaries:

- receipts-first performance decisions;
- public crates as contracts and modules as architecture boundaries; and
- local receipts first with server ledger optional.

Create an ADR only if implementation changes those boundaries, such as making
server ledger history part of correctness, adding a new public policy surface,
or turning canary freshness into a release gate.

## Evidence plan

Proposal, spec, plan, and status PRs should run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Behavior PRs should add focused tests before broad proof. Expected targeted
gates include:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 test -p perfgate-cli --all-features report
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
```

Cross-cutting implementation should also prove the existing public and
compatibility boundaries:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

External canary work should record what each canary proves and does not prove,
and should not become an implicit release gate without an accepted spec update.

## Risks

- Promotion language can make advisory maturity output sound like automatic
  enforcement.
- Profiles can look like universal defaults instead of starting posture
  suggestions.
- A review packet can become too large and duplicate every receipt instead of
  summarizing the decision-relevant facts.
- Action posture output can confuse users if advisory and blocking paths are
  not visibly separated.
- Agents can weaken policy if the guardrails do not clearly mark dangerous
  changes as review-required.
- Proof freshness can become marketing language instead of an operational
  constraint.

## Non-goals

- Do not add another benchmark engine.
- Do not add a dashboard.
- Do not expand the five public crates.
- Do not require server ledger mode.
- Do not auto-promote baselines.
- Do not auto-loosen thresholds.
- Do not make all mature benchmarks blocking by default.
- Do not bury policy changes in generated config.
- Do not let agents change gates without a review surface.
- Do not change receipt schemas, CLI command names, GitHub Action behavior, or
  release aliases without an accepted spec and explicit proof.

## Exit criteria

This proposal is complete when:

- the advisory-to-blocking promotion contract spec exists and is accepted;
- a 0.20 implementation plan sequences the lane into PR-sized changes;
- policy profiles exist as reviewable metadata;
- promotion readiness can be reported without mutating config;
- policy patch output is reviewable and non-mutating;
- review packets gather verdict, maturity, signal, calibration, decision,
  freshness, next command, and do-not guidance;
- GitHub Action summaries expose advisory/blocking/promotion posture;
- agent policy guardrails and fixtures prevent unsafe policy changes;
- proof freshness gates claim promotion where appropriate;
- at least one external canary proves the rollout path; and
- a closeout records what teams can now govern, what remains advisory, what
  agents cannot change, and what remains unproven.

