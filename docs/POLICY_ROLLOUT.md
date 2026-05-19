# Policy Rollout Profiles

Policy rollout is the step after evidence maturity. Use it when a team already
has one or more perfgate benchmarks and needs to decide which signals should
stay advisory, which are candidates for blocking policy, and which should be
quarantined or retired.

The rule is:

```text
smoke -> advisory -> gate_candidate -> required_gate
```

Promotion is deliberate. A mature baseline is useful evidence, not automatic
approval to block pull requests.

## Inspect Profiles

List the reviewable starting points:

```bash
perfgate policy profiles
```

Inspect one profile:

```bash
perfgate policy profiles --profile rust-cli-standard
perfgate policy profiles --profile rust-workspace-advisory
perfgate policy profiles --profile node-command-advisory
perfgate policy profiles --profile python-command-advisory
perfgate policy profiles --profile http-local-smoke
perfgate policy profiles --profile generic-command-advisory
perfgate policy profiles --profile agent-heavy-repo
perfgate policy profiles --profile server-ledger-optional
```

Profiles are metadata. They do not edit `perfgate.toml`, promote baselines,
loosen thresholds, make checks blocking, or require server ledger mode.

## Choose Starting Posture

Use the smallest posture that answers the review question:

| Posture | Use when | Do not infer |
|---------|----------|--------------|
| `smoke` | the command proves setup, startup, or first-hour wiring | the workload is safe to block PRs |
| `advisory` | the signal is useful but not proven enough to enforce | failures can be ignored forever |
| `gate_candidate` | evidence looks mature enough for review | the gate is already approved |
| `required_gate` | reviewers explicitly approved blocking policy | every future failure is a code regression |
| `quarantined` | host, noise, benchmark intent, or proof freshness is untrustworthy | the benchmark is permanently useless |
| `retired` | the workload no longer helps active review | old receipts should be deleted |

Start advisory unless the workload is already well understood. Promote one
benchmark at a time.

## Promotion Checklist

Before a benchmark moves toward `gate_candidate`, check:

```text
baseline exists
baseline is mature enough for the workload
signal is stable, or paired mode is selected
host context is compatible or intentionally scoped
calibration was reviewed or explicitly deferred
the workload is suitable for the intended policy
proof freshness is current or explicitly bounded as recent
reviewers can reproduce the result locally
```

Before a benchmark becomes `required_gate`, require human review. The review
should show:

```text
current posture
proposed posture
baseline maturity
signal maturity
host compatibility
calibration status
proof freshness
decision or tradeoff readiness
local reproduction command
what not to do
```

## Use The Existing Maturity Commands

Policy rollout should be based on receipts, not confidence by naming.

Check baseline maturity:

```bash
perfgate baseline doctor --config perfgate.toml
```

Check signal maturity:

```bash
perfgate doctor signal --config perfgate.toml
```

Check whether the evidence is ready for policy review:

```bash
perfgate policy doctor --config perfgate.toml
perfgate policy doctor --config perfgate.toml --bench parser
```

`policy doctor` reports the current posture, recommended posture, baseline
maturity, signal confidence, host compatibility, calibration status, proof
freshness, decision readiness, missing requirements, artifacts, and next
commands. It is advisory: it does not edit config, promote baselines, loosen
thresholds, make a gate blocking, or require server ledger mode.

Emit a reviewable calibration patch when enough samples exist:

```bash
perfgate calibrate --config perfgate.toml --bench parser --emit-patch
```

Reproduce the gate locally:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

If repeated runs disagree, keep the benchmark advisory, increase samples, or
use paired mode before promoting.

## Profile Guidance

| Profile | Starting posture | Good fit | Promotion caution |
|---------|------------------|----------|-------------------|
| `rust-cli-standard` | advisory, then one `gate_candidate` command | small Rust CLIs with fast command workloads | startup smoke does not prove steady-state throughput |
| `rust-workspace-advisory` | advisory | larger workspaces with broad integration signal | compile and test setup can dominate the measurement |
| `node-command-advisory` | advisory | dedicated Node benchmark scripts | JIT warmup and package-manager setup can hide true signal |
| `python-command-advisory` | advisory | dedicated Python benchmark modules or scripts | interpreter startup and environment setup need review |
| `http-local-smoke` | smoke or advisory | local endpoint checks and isolated services | remote services and startup timing should not block by default |
| `generic-command-advisory` | advisory | language-neutral commands with stable local input | unknown noise is not a required-gate foundation |
| `agent-heavy-repo` | advisory with review-required policy changes | repos where agents inspect receipts and propose patches | agents must not loosen thresholds or promote baselines alone |
| `server-ledger-optional` | advisory ledger history | teams that want retained decision history | ledger mode is optional team history, not correctness |

## Quarantine Or Retire

Quarantine a benchmark when evidence becomes untrustworthy:

```text
high noise
host mismatch
stale proof
broken benchmark command
benchmark intent drift
external service variance
```

Retire a benchmark when it no longer answers an active review question. Keep
the history if it helps audits or release notes, but do not let retired
benchmarks affect current policy.

## Reviewer Rules

Reviewers should approve policy changes only when the evidence says what the
team thinks it says:

- promote one benchmark at a time;
- keep noisy workloads advisory;
- use paired mode when host drift changes the verdict;
- use structured decisions for real tradeoffs;
- require local reproduction for blocking gates;
- treat missing baselines as setup, not regression;
- do not loosen thresholds to make a red check green; and
- do not make server ledger mode required for local correctness.

The useful question is not whether perfgate can run the command. It is whether
the result should become team policy.
