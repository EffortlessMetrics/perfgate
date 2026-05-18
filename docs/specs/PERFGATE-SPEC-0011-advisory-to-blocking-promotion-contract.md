# PERFGATE-SPEC-0011: Advisory-to-blocking promotion contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-18
Milestone: 0.20.0
Behavior version: policy-promotion-contract.v1
Product surface: policy profiles, promotion readiness, non-mutating policy patch output, review packets, action summary posture, proof freshness discipline
CI surface: docs-source-check, product-claims-check, doc-test, focused CLI policy/report/check tests, action-check, schema-compat if receipt shape changes
Schema impact: no receipt schema change by default; policy ergonomics reads existing config, run, compare, report, repair context, decision, proof freshness, and canary records
Action impact: no action input, alias, or workflow behavior change by default; action summaries may surface policy posture from existing receipts and config
Server impact: server ledger remains optional team history and must not be required for policy promotion or local correctness
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions
Linked plan: plans/0.20.0/policy-ergonomics-team-rollout.md
Linked policy: policy ledgers remain source of truth for governed exceptions, public surface, workflow policy, generated files, and release proof
Support/status impact: product claims should add or promote policy ergonomics claims only after behavior, tests, and canary proof land
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

The 0.19 evidence-maturity lane tells teams whether evidence is trustworthy:
which benchmarks are smoke checks, which baselines are mature, which signals
are noisy, when paired mode is safer, when structured decisions are warranted,
and which proof is current enough to cite.

The next gap is team rollout. A mature signal should not automatically become a
blocking policy. Teams need a reviewable path from advisory evidence to required
CI gates:

```text
smoke -> advisory -> gate candidate -> required gate
```

Without a promotion contract, teams can overreact to maturity guidance:

```text
all mature benchmarks become blocking
noisy checks get forced through policy
agents loosen thresholds to unblock CI
missing baselines get promoted without review
server ledger availability becomes accidental correctness
```

This spec defines the policy promotion vocabulary, transition requirements,
profile metadata, promotion readiness output, policy patch behavior, review
packet fields, action posture, and proof freshness discipline for 0.20.

## Behavior

perfgate MUST keep maturity and policy separate:

```text
evidence maturity says whether evidence is trustworthy
policy ergonomics says how a team may roll that evidence into enforcement
```

Policy ergonomics output is advisory until an explicit team policy promotes a
benchmark or profile. perfgate MAY suggest profile posture, report promotion
readiness, and emit reviewable config patches. It MUST NOT silently promote
baselines, loosen thresholds, make mature evidence blocking, require server
ledger mode, or mutate policy by default.

## Rollout states

The canonical rollout states are:

| State | Meaning | Required behavior |
|-------|---------|-------------------|
| `smoke` | Fast setup or rough performance confidence; useful for command wiring and first-hour proof. | MUST NOT be treated as a required PR gate by default. |
| `advisory` | Useful review signal that should appear in local/CI output but should not block yet. | SHOULD report maturity and next steps without failing policy by itself. |
| `gate_candidate` | Evidence appears mature enough for a reviewer to consider blocking policy. | MUST require reviewable promotion evidence before becoming required. |
| `required_gate` | Explicitly approved to block PRs according to configured policy. | MUST have a clear failure meaning and local reproduction path. |
| `quarantined` | Temporarily removed from enforcement because evidence, host, or benchmark behavior is not trustworthy. | SHOULD explain why enforcement is paused and what evidence is needed to recover. |
| `retired` | Removed from active performance policy. | SHOULD keep history/audits as available but not affect current decisions. |

User-facing output MAY use friendly wording, but it MUST preserve these
meanings.

## Transition requirements

Promotion from `smoke` or `advisory` toward `gate_candidate` SHOULD require:

- baseline exists and is not missing setup;
- baseline is mature enough for the workload purpose;
- signal is stable or paired mode has been selected;
- host context is compatible or intentionally scoped;
- calibration was reviewed or explicitly deferred;
- benchmark workload is suitable for the intended policy posture;
- proof freshness is current or bounded as recent; and
- reviewer can reproduce the result locally.

Promotion from `gate_candidate` to `required_gate` MUST require explicit review
approval. At minimum, the review surface SHOULD show:

- current posture;
- proposed posture;
- baseline maturity;
- signal maturity;
- host compatibility;
- calibration status;
- proof freshness;
- decision readiness or tradeoff status;
- policy patch preview; and
- what not to do.

Demotion to `quarantined` SHOULD be available when evidence becomes
untrustworthy, such as high noise, host mismatch, stale proof, broken benchmark
command, or benchmark intent drift. `retired` SHOULD be used when a benchmark is
no longer useful for active policy.

## Policy profile catalog

perfgate SHOULD define repo-shape policy profiles as metadata before adding
behavior-changing defaults. Initial profile names SHOULD include:

- `rust-cli-standard`;
- `rust-workspace-advisory`;
- `node-command-advisory`;
- `python-command-advisory`;
- `http-local-smoke`;
- `generic-command-advisory`;
- `agent-heavy-repo`; and
- `server-ledger-optional`.

Each profile SHOULD define:

```text
starting posture
promotion requirements
default evidence expectations
known bad fits
failure meaning
what not to infer
```

Profiles MUST remain suggestions. Applying or generating a profile MUST NOT
silently promote baselines, make every benchmark blocking, require ledger mode,
or override benchmark-specific evidence.

## Promotion readiness doctor

perfgate SHOULD provide promotion readiness output through a command such as:

```bash
perfgate policy doctor --config perfgate.toml
```

The output SHOULD report per benchmark:

- current posture;
- recommended posture;
- maturity state;
- signal confidence;
- host compatibility;
- calibration status;
- proof freshness;
- decision/tradeoff readiness;
- missing requirements;
- artifact paths; and
- next command.

Example shape:

```text
bench: parser
current posture: advisory
recommended posture: gate_candidate
why:
  - baseline mature
  - CV below noise threshold
  - host compatible
  - calibration patch reviewed
missing:
  - required-gate reviewer approval
next:
  perfgate policy emit-patch --config perfgate.toml --bench parser --to gate_candidate
```

The doctor MUST be advisory. It MUST NOT write config or change gate behavior.

## Policy patch output

perfgate SHOULD provide non-mutating patch output through a command such as:

```bash
perfgate policy emit-patch --config perfgate.toml --bench parser --to gate_candidate
```

Patch output SHOULD include:

- a reviewable TOML fragment or unified diff;
- current posture and proposed posture;
- evidence used;
- reasons for the recommendation;
- required review notes;
- what the patch does not prove; and
- rollback or demotion guidance.

The command MUST NOT write config by default. A future write mode requires an
accepted spec update and explicit user action.

## Review packet

perfgate SHOULD provide a compact policy review packet in reports, comments, or
an artifact. The packet SHOULD gather:

- gate verdict;
- current and recommended posture;
- baseline maturity;
- signal maturity and confidence;
- calibration status;
- host compatibility;
- decision suggestion;
- proof freshness;
- relevant artifacts;
- local reproduction command;
- policy patch command; and
- do-not guidance.

The packet SHOULD summarize decision-relevant facts. It SHOULD NOT duplicate
every receipt field or replace the underlying receipts.

## GitHub Action posture

GitHub Action summaries SHOULD surface policy posture when the relevant data is
available:

- blocking gate;
- advisory signal;
- maturity warning;
- promotion candidate;
- policy review required;
- quarantined evidence; and
- retired benchmark.

The action MUST preserve existing verdict, artifact, and local reproduction
behavior. Advisory posture MUST NOT become blocking unless configured policy
already says so.

## Agent policy guardrails

This spec defines the policy promotion surface. A separate agent guardrail spec
SHOULD define detailed agent behavior. Until that spec lands, policy output
MUST clearly mark review-required actions:

- baseline promotion;
- threshold loosening;
- making a benchmark blocking;
- accepting a tradeoff;
- changing a profile;
- quarantining or retiring a gate; and
- requiring server ledger mode.

Agents MAY inspect artifacts, rerun commands, summarize maturity, suggest paired
mode, and propose patches. They MUST NOT perform review-required actions without
explicit human approval.

## Proof freshness discipline

Proof freshness MUST constrain promotion language:

- `current` proof MAY support current policy recommendations.
- `recent` proof MAY support bounded recommendations with explicit limits.
- `stale` proof MUST NOT support promotion by itself.
- `superseded` proof MUST point to newer evidence.
- `unproven` gaps MUST stay visible and MUST NOT be treated as policy support.

Promotion output SHOULD cite proof freshness where canaries, external runs,
release smoke, platform support, or hosted Action evidence are used.

## Non-goals

- Do not add another benchmark engine.
- Do not add a dashboard.
- Do not expand the five public crates.
- Do not require server ledger mode.
- Do not auto-promote baselines.
- Do not auto-loosen thresholds.
- Do not make all mature benchmarks blocking by default.
- Do not mutate policy by default.
- Do not make structured decisions mandatory for local gates.
- Do not change receipt schemas, CLI command names, GitHub Action behavior, or
  release aliases without an accepted spec and explicit proof.
- Do not make agents policy authorities.

## Required evidence

Documentation-only changes to this spec SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Behavior changes SHOULD add focused proof for the touched surface:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 test -p perfgate-cli --all-features report
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
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
| A mature advisory benchmark is reported as `gate_candidate` with reviewed calibration missing. | Pass |
| A noisy benchmark remains `advisory` and suggests paired mode before promotion. | Pass |
| A compile-heavy workspace recipe starts advisory and explains it should not become a first-hour required gate. | Pass |
| A promotion patch prints a TOML fragment and reason section without editing `perfgate.toml`. | Pass |
| A GitHub Action summary labels one benchmark as blocking and another as advisory. | Pass |
| Stale canary proof is visible and does not support required-gate promotion by itself. | Pass |
| Server ledger mode is suggested as optional team history and not required for policy promotion. | Pass |
| A mature baseline automatically makes a benchmark a required gate. | Fail |
| A missing baseline is promoted to unblock CI without review. | Fail |
| A threshold is loosened automatically because a regression failed. | Fail |
| A stale hosted canary is cited as current proof for a policy claim. | Fail |
| Optional server ledger upload failure invalidates local receipt correctness by default. | Fail |

## Test mapping

Current or planned proof maps to:

- CLI policy tests for profile catalog, policy doctor, and policy patch output;
- CLI report/comment tests for review packet content;
- CLI check tests for advisory versus blocking behavior;
- action-check fixtures for Action summary posture;
- product-claims-check for proof freshness claim discipline;
- docs-source-check for proposal/spec/plan/status links;
- schema-compat if any review packet or policy receipt shape changes; and
- public-surface checks if implementation touches package boundaries.

## Implementation mapping

The promotion contract is owned by:

- `docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md` for
  lane rationale;
- this spec for rollout state and behavior contract;
- the future 0.20 implementation plan for PR sequencing;
- `crates/perfgate-cli` for policy profile, doctor, patch, report, and check
  surfaces;
- GitHub Action summary generation and `xtask action-check` for CI posture;
- `docs/status/PROOF_FRESHNESS.md` and `docs/status/PRODUCT_CLAIMS.md` for
  freshness and support mapping;
- `docs/status/CANARY_MATRIX.md` for external proof context; and
- existing policy ledgers for governed exceptions, public surfaces, workflow
  policy, and release proof.

This spec may link policy ledgers but MUST NOT copy their rows.

## CI proof

Policy ergonomics changes MUST select proof commands by affected surface:

| Surface | Proof |
|---------|-------|
| Proposal/spec/plan/status docs | `docs-check`, `doc-test`, `docs-source-check`, `product-claims-check`, `git diff --check` |
| Profile catalog | focused CLI policy tests and docs examples |
| Promotion doctor | focused CLI policy tests |
| Policy patch output | focused CLI policy tests plus no-write assertions |
| Review packet | focused report/comment/check tests |
| Action posture | `cargo +1.95.0 run -p xtask -- action-check` |
| Proof freshness claim promotion | `cargo +1.95.0 run -p xtask -- product-claims-check` |
| Receipt/schema impact | `cargo +1.95.0 run -p xtask -- schema-compat` |
| Public surface risk | `cargo +1.95.0 run -p xtask -- public-surface --strict` |

## Promotion rule

This spec is accepted when merged as the advisory-to-blocking promotion
contract. It is implemented when:

- a 0.20 implementation plan exists;
- policy profile metadata exists;
- promotion readiness can be reported without mutating config;
- policy patch output is reviewable and non-mutating;
- review packets gather verdict, maturity, signal, calibration, decision,
  proof freshness, next command, and do-not guidance;
- Action summaries expose advisory/blocking/promotion posture when data exists;
- agent policy guardrails are specified and fixture-backed;
- proof freshness constrains claim promotion; and
- at least one external canary proves the policy rollout path.
