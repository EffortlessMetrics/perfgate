# Policy Rollout Canary: droid-action

Date: 2026-05-19

Status: observed

Linked proposal: [`PERFGATE-PROP-0007`](../proposals/PERFGATE-PROP-0007-policy-ergonomics-team-rollout.md)

Linked specs:
[`PERFGATE-SPEC-0011`](../specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md),
[`PERFGATE-SPEC-0012`](../specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md)

Linked plan: [`policy-ergonomics-team-rollout.md`](../../plans/0.20.0/policy-ergonomics-team-rollout.md)

Purpose: record the first external policy-rollout canary after the 0.20 policy
ergonomics surfaces landed. This canary used a temporary local clone of a
non-Rust repository and exercised the advisory-to-promotion review path without
pushing a hosted external PR.

## Canary Target

| Field | Value |
| --- | --- |
| Repository shape | TypeScript GitHub Action repository |
| Source repo | `H:\Code\Typescript\droid-action` |
| Canary clone | `C:\perfgate-canaries\policy-rollout-droid-action-20260519` |
| Source branch | `sz/99-fork-smoke-harness` |
| Source commit | `3f325c127dad1e3909e090f0447a5669fe023a9e` |
| perfgate source commit | `9c98dee6c702682efe52df0077a1645597c30506` |
| perfgate binary | `D:\Code\Rust\perfgate\target\debug\perfgate.exe` |
| Platform | Windows x86_64 |

## Commands

The canary used the current workspace-built perfgate binary:

```bash
perfgate --version
```

Result:

```text
perfgate 0.18.0
```

The external repo was cloned into an isolated canary directory:

```bash
git clone --local --no-hardlinks H:/Code/Typescript/droid-action C:/perfgate-canaries/policy-rollout-droid-action-20260519
git rev-parse HEAD
```

Result:

```text
3f325c127dad1e3909e090f0447a5669fe023a9e
```

First contact:

```bash
perfgate doctor
```

Result excerpt:

```text
State: no_config
Meaning: No perfgate config was found for this repo.
Next:
  perfgate init --ci github --profile standard
Do not:
  do not copy another repo's baselines before initializing this repo
```

Initialization used the standard init profile plus benchmark suggestions:

```bash
perfgate init --ci github --profile standard --suggest-benches
```

Result excerpt:

```text
Appended reviewable benchmark suggestions (node-command) to perfgate.toml.
Review and edit suggestions before committing baselines.
```

The generated suggestions correctly stayed commented and reviewable. The
canary added one explicit non-Rust command benchmark:

```toml
[[bench]]
name = "droid-node-version"
command = ["node", "-e", "console.log(process.version)"]
```

First check:

```bash
perfgate check --config perfgate.toml --all
```

Result excerpt:

```text
Status: missing_baseline
Meaning: Setup is incomplete; this is not a performance regression.
Next:
  perfgate check --config perfgate.toml --bench droid-node-version
  perfgate baseline promote --config perfgate.toml --bench droid-node-version
Do not:
  do not loosen thresholds to fix a missing baseline
```

Baseline promotion:

```bash
perfgate baseline promote --config perfgate.toml --all
```

Result:

```text
Promoted baseline for droid-node-version
  current: artifacts/perfgate\droid-node-version\run.json
  baseline: baselines\droid-node-version.json
```

Required-baseline rerun:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

Result excerpt:

```text
Status: performance_regression
Meaning: A configured benchmark exceeded its performance budget or warning threshold.
Status: high_noise
Meaning: The run is noisy enough that the result may need paired mode or calibration.
Do not:
  do not treat noisy single-run evidence as release proof
```

Baseline maturity:

```bash
perfgate baseline doctor --config perfgate.toml
```

Result excerpt:

```text
bench: droid-node-version
status: high_noise
samples: 7 measured samples
cv: 47.6%
recommendation: keep advisory; calibrate or use paired mode before blocking PRs
```

Signal maturity:

```bash
perfgate doctor signal --config perfgate.toml
```

Result excerpt:

```text
recommendation: use_paired_mode
meaning: ordinary runs are noisy; compare baseline/current under paired conditions
Do not:
  treat noisy or immature evidence as policy just because receipts exist
  make server ledger upload part of local correctness
```

Policy readiness:

```bash
perfgate policy doctor --config perfgate.toml
```

Result excerpt:

```text
bench: droid-node-version
current posture: advisory
recommended posture: advisory
baseline maturity: high_noise
signal confidence: use_paired_mode
calibration status: paired mode or calibration review required before promotion
missing:
  - paired-mode or calibration review
  - paired-mode evidence
Advisory only: no config, baseline, threshold, policy, or server setting was changed.
```

Policy patch output:

```bash
perfgate policy emit-patch --config perfgate.toml --bench droid-node-version --to gate_candidate
```

Result excerpt:

```text
current posture: advisory
recommended posture: advisory
proposed posture: gate_candidate
Missing or review-required:
  - paired-mode or calibration review
  - paired-mode evidence
  - gate_candidate is review-ready evidence, not blocking policy
Advisory only: no config, baseline, threshold, policy, or server setting was changed.
```

Calibration patch output:

```bash
perfgate calibrate --config perfgate.toml --bench droid-node-version --emit-patch
```

Result excerpt:

```text
CV: 61.1%
Suggested fail threshold: 30.0%
Repeat guidance: collect at least 10 measured samples before tightening.
Paired mode: recommended before making this gate blocking.
Advisory only: no config was written.
```

Review packet:

```bash
perfgate policy review-packet --config perfgate.toml --bench droid-node-version --out artifacts/perfgate/droid-node-version/policy-review.md
```

Result:

```text
Wrote policy review packet: artifacts/perfgate/droid-node-version/policy-review.md
```

Review packet excerpt:

```text
Gate verdict: `warn`
Current posture: `advisory`
Recommended posture: `advisory`
Baseline maturity: `high_noise`
Signal confidence: `use_paired_mode`
Proof freshness: current (local run and compare receipts present)

Agent Guardrails
Scenario: `noisy_signal`
Allowed: recommend paired mode, more samples, or calibration review
Review required: policy promotion or threshold changes
Forbidden by default: do not treat noisy evidence as a confirmed regression or required gate
```

## Generated Files

Durable setup files in the canary clone:

```text
perfgate.toml
.github/workflows/perfgate.yml
.perfgate/README.md
baselines/.gitkeep
baselines/droid-node-version.json
```

Transient artifacts:

```text
artifacts/perfgate/droid-node-version/run.json
artifacts/perfgate/droid-node-version/compare.json
artifacts/perfgate/droid-node-version/report.json
artifacts/perfgate/droid-node-version/comment.md
artifacts/perfgate/droid-node-version/repair_context.json
artifacts/perfgate/droid-node-version/policy-review.md
```

## CI Wiring

`perfgate init --ci github --profile standard --suggest-benches` generated a
workflow using the public action alias:

```yaml
uses: EffortlessMetrics/perfgate@v0
with:
  config: perfgate.toml
  all: "true"
  require_baseline: "true"
  upload_artifact: "true"
```

This canary did not push a hosted PR or run hosted CI. Action posture remains
covered by in-repo `action-check` and the earlier hosted Action canary until a
dedicated hosted policy-rollout canary is run.

## Observations

What worked:

- `doctor` identified the `no_config` state and gave the next init command.
- `init --suggest-benches` selected a Node command recipe and kept suggested
  benches commented for review.
- Missing baseline was classified as setup, not a regression.
- Baseline and signal doctors correctly refused to treat noisy Node startup
  evidence as gate-ready.
- `policy doctor` kept the benchmark advisory and named paired-mode evidence as
  missing before promotion.
- `policy emit-patch` produced a reviewable patch while saying the proposed
  `gate_candidate` state was not blocking policy.
- `calibrate --emit-patch` produced copy-ready threshold guidance while
  preserving the no-write boundary.
- `policy review-packet` gave a reviewer and agent the posture, maturity,
  signal, artifacts, next commands, and do-not guidance in one Markdown file.

What was confusing or operationally important:

- `perfgate init --profile generic-command` is not an init profile; benchmark
  recipe selection currently comes from `--suggest-benches` under the standard
  init profile.
- The `node -e console.log(process.version)` workload is intentionally too
  startup-heavy and noisy to become a required gate. The policy rollout output
  handled that correctly by staying advisory.
- The generated workflow points at the public `v0` action alias; this local
  source-built canary does not prove hosted 0.20 policy posture.

## Follow-Up Decision

No product change is required from this canary. It supports the 0.20 policy
ergonomics shape: teams can start advisory, inspect maturity, request a
reviewable patch, and stop before making noisy evidence blocking.

The next canary gap is hosted policy posture:

```text
Run a hosted external PR canary that uses the current policy posture summary
and records the Action output from an external repo.
```

## What This Canary Proves

- A real non-Rust repo can use the policy rollout path from a source-built
  perfgate binary.
- Benchmark suggestions remain reviewable and non-magical.
- Baseline and signal maturity can prevent a noisy workload from becoming a
  blocking gate.
- `policy doctor`, `policy emit-patch`, `calibrate --emit-patch`, and
  `policy review-packet` work together as an advisory-to-review workflow.
- Agent guardrails in the review packet tell agents what is allowed,
  review-required, and forbidden for noisy evidence.

## What This Canary Does Not Prove

- Hosted external Action policy posture.
- Public 0.20 install behavior or release aliases.
- A mature `gate_candidate` promotion in an external repo.
- Every non-Rust command shape or hosted runner.
- Probe-backed policy rollout.
- Server ledger correctness or any requirement that ledger mode be part of
  local correctness.

## Cleanup

The canary used a temporary local clone under `C:\perfgate-canaries` and a
workspace-built perfgate binary. The durable evidence is this audit and the
recorded commands above; the temporary clone and build artifacts can be removed
after the audit is committed.
