# perfgate 0.20.0 Policy Ergonomics and Team Rollout Plan

Status: active
Owner: perfgate maintainers
Created: 2026-05-18
Milestone: 0.20.0
Current PR: promotion-readiness-doctor
Linked proposal: [`PERFGATE-PROP-0007-policy-ergonomics-team-rollout`](../../docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md)
Linked specs: [`PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract`](../../docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md), `PERFGATE-SPEC-0012-agent-policy-change-guardrails` (planned)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../../docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked policy: policy ledgers remain referenced by specs and status docs; no policy row changes in this plan PR
Support/status impact: product claims should add or promote policy ergonomics claims only after behavior, tests, proof freshness, and at least one rollout canary land
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check
Blocks: policy profile catalog, promotion doctor, policy patch output, review packet, action posture, agent guardrails, proof freshness claim discipline, rollout canary, closeout
Blocked by:
Rollback: revert this plan and `.codex/goals/active.toml`; proposal and promotion contract remain accepted source-of-truth artifacts

## Goal

Make perfgate safe for team policy rollout. The 0.19 evidence maturity lane
tells teams which benchmarks, baselines, signals, decisions, canaries, repair
context, and ledger history are trustworthy enough to reason about. This lane
defines how teams can promote that evidence into policy without creating brittle
gates:

```text
advisory first
prove maturity
promote deliberately
block only when evidence is stable
explain failures clearly
keep escape hatches reviewed
keep agents from weakening policy
```

The implementation target is a reviewable advisory-to-blocking workflow:
profiles suggest starting posture, policy doctor reports readiness, patch
output is non-mutating, review packets summarize the evidence, Action summaries
separate advisory and blocking posture, agents have explicit guardrails, and
fresh proof constrains claim promotion.

## Activation Boundary

The 0.18 release cutover is complete and archived. The 0.19 evidence maturity
lane is complete and archived. This 0.20 lane builds policy ergonomics on top of
those surfaces.

This plan does not publish crates, move tags, change action aliases, expand the
public crate surface, add a dashboard, require server ledger mode, auto-promote
baselines, auto-loosen thresholds, or change receipt schemas by default. Any
schema, public surface, release alias, or action behavior change requires an
accepted spec and explicit proof.

## Operating Rules

- Keep one semantic artifact or narrow product delta per PR.
- Preserve the five public crates.
- Preserve CLI command names, receipt schemas, GitHub Action behavior, and
  release aliases unless an accepted spec says otherwise.
- Keep local receipts as the correctness contract.
- Keep server ledger mode optional team history.
- Keep policy profiles as suggestions, not behavior-changing magic.
- Keep maturity output advisory until explicitly promoted by team policy.
- Do not silently promote baselines, loosen thresholds, write policy, or make
  mature evidence blocking.
- Do not let agents change gates, thresholds, baselines, tradeoff decisions, or
  ledger requirements without a review surface.
- Product claims must wait for behavior, tests, freshness mapping, and canary
  proof.
- Generated badge or baseline churn stays separate from policy semantics.

## PR Sequence

| PR | Work item | Status | Files / surface |
|----|-----------|--------|-----------------|
| 534 | Policy ergonomics proposal | merged | `docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md` |
| 536 | Advisory-to-blocking promotion contract | merged | `docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md` |
| 538 | Policy ergonomics implementation plan | merged | `plans/0.20.0/policy-ergonomics-team-rollout.md`, `.codex/goals/active.toml` |
| 540 | Policy profile catalog | merged | policy profile metadata and focused CLI tests |
| 544 | Rollout profile guidance | merged | user-facing profile and promotion-path docs |
| TBD | Promotion readiness doctor | current | `perfgate policy doctor --config perfgate.toml` |
| TBD | Policy patch output | pending | `perfgate policy emit-patch --config perfgate.toml --bench <bench> --to <state>` |
| TBD | Performance review packet | pending | report/comment artifact summary of policy posture and maturity |
| TBD | GitHub Action posture summary | pending | Action summary posture and `action-check` fixtures |
| TBD | Agent policy guardrail spec | pending | `PERFGATE-SPEC-0012-agent-policy-change-guardrails` |
| TBD | Agent policy fixtures | pending | policy guardrail fixtures for review-required changes |
| TBD | Proof freshness claim discipline | pending | product-claims proof freshness enforcement for policy promotion |
| TBD | External policy rollout canary plan | pending | status canary rerun plan for policy ergonomics |
| TBD | Public policy rollout canary | pending | one real canary proving advisory-to-promotion path |
| TBD | Policy ergonomics closeout | pending | handoff and archived active goal |

## Work item: implementation-plan

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: policy-profile-catalog, rollout-profile-guidance, promotion-readiness-doctor
Blocked by:

### Goal

Create the implementation sequence and active goal manifest for the 0.20 policy
ergonomics lane.

### Production delta

Add:

```text
plans/0.20.0/policy-ergonomics-team-rollout.md
.codex/goals/active.toml
```

Update proposal and spec headers to point at this concrete plan.

### Non-goals

- No product behavior change.
- No public crate, receipt schema, release, tag, alias, or Action behavior
  change.
- No product-claim promotion before behavior exists.

### Acceptance

- Plan links proposal, promotion contract, ADRs, source-of-truth boundaries,
  PR sequence, proof commands, and rollback.
- `.codex/goals/active.toml` points at this lane.
- Proposal/spec plan pointers no longer say planned.
- Product claims remain unchanged.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert this plan, active goal, and proposal/spec pointer updates. The proposal
and promotion contract remain accepted artifacts.

## Work item: policy-profile-catalog

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: rollout-profile-guidance, promotion-readiness-doctor
Blocked by: implementation-plan

### Goal

Add repo-shape policy profile metadata without making behavior-changing
defaults.

### Production delta

Add profile metadata for:

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

Each profile should include starting posture, promotion requirements, evidence
expectations, known bad fits, failure meaning, and what not to infer.

### Non-goals

- Do not silently select benchmarks.
- Do not auto-promote baselines.
- Do not make generated profiles blocking.
- Do not require server ledger mode.

### Acceptance

- Profile metadata is reviewable and test-covered.
- Existing first-use recipe behavior remains compatible.
- Server-ledger profile stays optional team history.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 test -p perfgate-cli --test cli_help_snapshot_tests --all-features
cargo +1.95.0 clippy -p perfgate-cli --all-targets --all-features -- -D warnings
git diff --check
```

### Rollback

Revert profile metadata and focused tests.

## Work item: rollout-profile-guidance

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: promotion-readiness-doctor
Blocked by: policy-profile-catalog

### Goal

Explain how teams choose profiles and promote one benchmark at a time.

### Production delta

Add or update docs covering:

- start advisory;
- prove maturity;
- promote one benchmark at a time;
- keep noisy workloads advisory;
- quarantine evidence when host or signal changes;
- use structured decisions for tradeoffs; and
- keep server ledger optional.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## Work item: promotion-readiness-doctor

Status: current
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: policy-patch-output, review-packet, product-claims
Blocked by: policy-profile-catalog

### Goal

Report advisory promotion readiness without changing config or gate behavior.

### Production delta

Add:

```bash
perfgate policy doctor --config perfgate.toml
```

The command should report current posture, recommended posture, baseline
maturity, signal confidence, host compatibility, calibration status, proof
freshness, decision readiness, missing requirements, artifacts, and next
command.

### Non-goals

- Do not write config.
- Do not make mature benchmarks blocking.
- Do not loosen thresholds.
- Do not promote baselines.

### Acceptance

- Missing baseline remains setup guidance.
- Noisy signal remains advisory or paired-mode guidance.
- Mature advisory evidence can become `gate_candidate`, not `required_gate`,
  until reviewer approval exists.
- Output names artifacts and next command.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 test -p perfgate-cli --test cli_help_snapshot_tests --all-features
cargo +1.95.0 clippy -p perfgate-cli --all-targets --all-features -- -D warnings
cargo +1.95.0 run -p xtask -- docs-source-check
git diff --check
```

## Work item: policy-patch-output

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: review-packet, action-posture-summary
Blocked by: promotion-readiness-doctor

### Goal

Emit reviewable policy patches without mutating config.

### Production delta

Add:

```bash
perfgate policy emit-patch --config perfgate.toml --bench parser --to gate_candidate
```

The output should include a TOML fragment or unified diff, current/proposed
posture, evidence used, reasons, review-required notes, what it does not prove,
and demotion guidance.

### Non-goals

- No `--write` mode in this PR.
- No automatic policy mutation.
- No threshold loosening.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 clippy -p perfgate-cli --all-targets --all-features -- -D warnings
git diff --check
```

## Work item: review-packet

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: action-posture-summary, agent-policy-guardrails
Blocked by: promotion-readiness-doctor, policy-patch-output

### Goal

Add a compact performance review packet that gathers policy posture and
decision-relevant evidence.

### Production delta

Reports, comments, or a dedicated artifact should include verdict, current and
recommended posture, baseline maturity, signal maturity, calibration status,
host compatibility, decision suggestion, proof freshness, artifacts, local
reproduction, policy patch command, and do-not guidance.

### Non-goals

- Do not duplicate every receipt field.
- Do not replace receipts as source of truth.
- Do not change receipt schema unless a follow-up spec requires it.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features report
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: action-posture-summary

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: policy-claims-freshness, rollout-canary-plan
Blocked by: review-packet

### Goal

Surface advisory/blocking/promotion posture in GitHub Action summaries.

### Acceptance

- Summary distinguishes blocking gates from advisory signals.
- Maturity warnings do not become blocking unless configured policy says so.
- Local reproduction and artifact links remain visible.
- `action-check` fixtures cover posture output.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: agent-policy-guardrails

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked specs: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md; PERFGATE-SPEC-0012-agent-policy-change-guardrails (planned)
Blocks: agent-policy-fixtures
Blocked by: review-packet

### Goal

Define what agents may do, what requires review, and what is forbidden by
default for policy changes.

### Production delta

Add:

```text
docs/specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md
```

### Acceptance

- Agents may inspect, rerun, summarize, suggest paired mode, and propose
  reviewable patches.
- Agents require explicit human review for baseline promotion, making gates
  blocking, loosening thresholds, accepting tradeoffs, changing profiles,
  quarantining/retiring gates, or requiring ledger mode.
- The contract links repair context but does not make agents policy
  authorities.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## Work item: agent-policy-fixtures

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked specs: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md; docs/specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md
Blocks: policy-claims-freshness
Blocked by: agent-policy-guardrails

### Goal

Back the agent policy contract with fixtures.

### Fixture cases

```text
missing baseline
noisy signal
mature promotion candidate
regression
tradeoff candidate
stale proof
```

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: policy-claims-freshness

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: rollout-canary-plan, policy-rollout-canary
Blocked by: action-posture-summary, agent-policy-fixtures

### Goal

Use proof freshness to prevent stale or unproven evidence from promoting policy
claims.

### Production delta

Update product-claims checks or status docs so policy claims citing canaries or
lane proof carry current/recent/stale/superseded/unproven language.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Work item: rollout-canary-plan

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: policy-rollout-canary
Blocked by: policy-claims-freshness

### Goal

Define which external canaries should be rerun after policy ergonomics exists.

### Canary targets

```text
small Rust CLI
large Rust workspace
non-Rust command repo
hosted Action path
public install path
failure summary path
agent-heavy repo
```

### Non-goals

- Do not rerun every canary in this planning PR.
- Do not make canaries mandatory release gates without an accepted spec.

## Work item: policy-rollout-canary

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked spec: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md
Blocks: final-closeout
Blocked by: rollout-canary-plan

### Goal

Record at least one external canary using the policy ergonomics path.

### Acceptance

The canary records advisory check, baseline maturity, signal maturity,
promotion doctor output, review packet, GitHub Action posture, what confused
the user, what changed, what it proves, and what it does not prove.

## Work item: final-closeout

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md
Linked specs: docs/specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md; docs/specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md
Blocks:
Blocked by: policy-rollout-canary

### Goal

Close the policy ergonomics lane with durable proof and non-inferences.

### Acceptance

- Handoff records what teams can now promote safely.
- It records what stayed advisory.
- It records what agents can and cannot change.
- It records current proof and remaining unproven surfaces.
- It archives `.codex/goals/active.toml`.
- It names the next recommended lane.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the closeout handoff and goal archive. Implemented policy ergonomics
behavior remains intact unless the closeout PR also changed status mappings.
