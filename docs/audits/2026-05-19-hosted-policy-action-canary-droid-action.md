# Hosted Policy Action Canary: droid-action

Date: 2026-05-19

Status: observed

Linked proposal: [`PERFGATE-PROP-0007-policy-ergonomics-team-rollout`](../proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md)

Linked specs: [`PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract`](../specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md), [`PERFGATE-SPEC-0012-agent-policy-change-guardrails`](../specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md)

Linked plan: [`policy-ergonomics-team-rollout.md`](../../plans/0.20.0/policy-ergonomics-team-rollout.md)

Support/status impact: this canary supports the hosted external Action policy
posture proof path. Product claims should cite it as scoped hosted evidence,
not broad hosted CI coverage.

Purpose: record a hosted external GitHub Action canary against a non-perfgate
repository after the policy ergonomics lane. The earlier local policy rollout
canary proved the CLI advisory-to-review path; this canary proves the Action
summary can surface policy posture, local policy commands, and do-not guidance
from an external repository shape.

## Canary Target

| Field | Value |
| --- | --- |
| External repository | `EffortlessSteven/droid-action` |
| External PR | `https://github.com/EffortlessSteven/droid-action/pull/8` |
| Canary branch | `sz/perfgate-policy-action-canary-20260519` |
| Base branch | `dev` |
| Workflow | `.github/workflows/perfgate-policy-canary.yml` |
| Action ref | `EffortlessMetrics/perfgate@main` |
| Action commit | `0e9542322b4456b896444d147f0faa24928cf9cf` |
| External commit | `9403156a0961835d797a3a25942f4f7d67501d96` |
| Runner | GitHub-hosted `ubuntu-24.04` |
| Config | `perfgate.toml` |
| Bench | `droid-node-version` |

The external PR added:

```text
.github/workflows/perfgate-policy-canary.yml
perfgate.toml
```

The workflow used `all: "false"`, `bench: "droid-node-version"`,
`require_baseline: "false"`, `fail_on_warn: "false"`, and
`upload_artifact: "true"`. It also printed collected step-summary files so the
hosted policy summary was visible in durable logs.

## Hosted Run

| Field | Value |
| --- | --- |
| Run | `26084181650` |
| URL | `https://github.com/EffortlessSteven/droid-action/actions/runs/26084181650` |
| Job | `76692962641` |
| Job URL | `https://github.com/EffortlessSteven/droid-action/actions/runs/26084181650/job/76692962641` |
| Result | success |
| Artifact | `perfgate-artifacts-26084181650-1` |
| Artifact ID | `7077981373` |
| Artifact digest | `ea0a67dd086650804ff2581be228fcd2c2c4b8d716d7cf6f8238c7c2dfba3d09` |

Downloaded artifact contents:

```text
comment.md
repair_context.json
report.json
run.json
```

The action installed perfgate from the action source at commit
`0e9542322b4456b896444d147f0faa24928cf9cf` and reported:

```text
perfgate 0.18.0
```

The check path reported setup guidance rather than a regression:

```text
Status: missing_baseline
Artifacts:
  artifacts/perfgate/comment.md
  artifacts/perfgate/repair_context.json
  artifacts/perfgate/report.json
  artifacts/perfgate/run.json
Next:
  perfgate check --config perfgate.toml --bench droid-node-version
  perfgate baseline promote --config perfgate.toml --bench droid-node-version
```

## Hosted Policy Summary

The workflow printed the Action step summary back into hosted logs. The policy
summary included:

```text
### perfgate policy posture

Blocking behavior: this action preserves existing perfgate exit-code behavior; maturity guidance is advisory unless your config already makes it blocking.
Advisory signal: missing baselines remain setup guidance unless this workflow enables required-baseline mode.
Gate verdict: `warn` (check exit code `0`).

Policy doctor command:

perfgate policy doctor --config perfgate.toml --out-dir artifacts/perfgate --bench droid-node-version

Review packet command:

perfgate policy review-packet --config perfgate.toml --bench droid-node-version --out-dir artifacts/perfgate
```

The embedded policy doctor output said:

```text
bench: droid-node-version
current posture: smoke
recommended posture: advisory
baseline maturity: missing
signal confidence: no_decision_yet
host compatibility: compatible_or_not_checked (run-only (linux-x86_64))
calibration status: paired mode or calibration review required before promotion
proof freshness: unproven (baseline missing)
decision readiness: simple gate first; structured decisions are optional until a tradeoff appears
missing:
  - baseline promotion after workload review
  - complete setup receipts
next:
  perfgate check --config perfgate.toml --bench droid-node-version
  perfgate baseline promote --config perfgate.toml --bench droid-node-version
```

The hosted summary also preserved the policy guardrail:

```text
Do not: make advisory maturity output blocking, loosen thresholds, promote baselines, or require server ledger mode from this summary alone.
```

## What This Canary Proves

- A hosted external PR can run the current perfgate Action policy posture path
  from a non-perfgate repository.
- The Action summary distinguishes advisory setup evidence from blocking
  policy.
- Missing baseline remains setup guidance when `require_baseline` is not
  enabled.
- The Action summary prints copyable local `policy doctor` and
  `policy review-packet` commands for the configured bench.
- The hosted policy doctor output includes current posture, recommended
  posture, baseline maturity, signal confidence, host compatibility,
  calibration status, proof freshness, decision readiness, missing evidence,
  next commands, and do-not guidance.
- Artifact upload still works on the hosted advisory path.

## What This Canary Does Not Prove

- It does not prove public `0.20` install behavior.
- It does not prove every hosted runner, shell, or action input combination.
- It does not prove mature `gate_candidate` promotion in a hosted external
  repo.
- It does not prove `required_gate` approval behavior in a hosted external
  repo.
- It does not prove probe-backed policy rollout.
- It does not prove server-ledger correctness.

## Cleanup

The canary used a temporary local clone under `C:\perfgate-canaries` plus the
external PR and hosted run. The local clone and downloaded artifacts are
evidence caches only; the durable evidence is this audit plus the external PR,
run, job, and artifact URLs.
