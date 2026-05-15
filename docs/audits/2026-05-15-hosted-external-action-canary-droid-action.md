# Hosted External Action Canary: droid-action

Date: 2026-05-15

Status: observed

Linked proposal: [`PERFGATE-PROP-0005`](../proposals/PERFGATE-PROP-0005-first-use-intelligence.md)

Linked specs: [`PERFGATE-SPEC-0008`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md)

Linked plan: [`first-use-intelligence.md`](../../plans/0.19.0/first-use-intelligence.md)

Support/status impact: this canary supports the hosted external action proof
path. Product claims use it as scoped hosted canary evidence, not broad hosted
CI coverage.

Purpose: record a hosted external GitHub Action canary against a non-perfgate
repository. Earlier external canaries proved local adoption in external repos;
this canary proves a PR workflow can run the perfgate Action in a hosted
external repository, upload artifacts, and print a copyable local reproduction
command.

## Canary Target

| Field | Value |
| --- | --- |
| External repository | `EffortlessSteven/droid-action` |
| External PR | `https://github.com/EffortlessSteven/droid-action/pull/7` |
| Canary branch | `sz/perfgate-hosted-action-canary-20260515` |
| Base branch | `dev` |
| Workflow | `.github/workflows/perfgate.yml` |
| Action ref | `EffortlessMetrics/perfgate@main` |
| Action commit | `a172a37b3e7f59351aaea402907b924f42c55320` |
| Runner | GitHub-hosted `ubuntu-24.04` |
| Config | `perfgate.toml` |
| Bench | `droid-node-version` |

The external PR added:

```text
.github/workflows/perfgate.yml
.perfgate/README.md
perfgate.toml
baselines/droid-node-version.json
```

The workflow used `require_baseline: true`, `fail_on_warn: true`, and
`upload_artifact: true`.

## Local Preflight

The canary first verified the external repo locally with the workspace-built
perfgate binary:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

After the baseline was adjusted to force a hosted failure path, the local
preflight used:

```bash
perfgate check --config perfgate.toml --all --require-baseline --fail-on-warn
```

Result: the CLI failed as expected and printed the first-use failure taxonomy
for `performance_regression` and `high_noise`, including next commands and
guardrail guidance.

## Hosted Run: Passing Setup

| Field | Value |
| --- | --- |
| Run | `25941466798` |
| URL | `https://github.com/EffortlessSteven/droid-action/actions/runs/25941466798` |
| Result | success |
| Artifact | `perfgate-artifacts-25941466798-1` |
| Artifact ID | `7026601760` |

Downloaded artifact contents:

```text
droid-node-version/comment.md
droid-node-version/compare.json
droid-node-version/report.json
droid-node-version/run.json
```

The PR-ready comment reported a pass. `wall_ms` compared a `46 ms` baseline to
a `27 ms` current run.

## Hosted Run: Forced Failure

| Field | Value |
| --- | --- |
| Run | `25941883937` |
| URL | `https://github.com/EffortlessSteven/droid-action/actions/runs/25941883937` |
| Job URL | `https://github.com/EffortlessSteven/droid-action/actions/runs/25941883937/job/76261230315` |
| Result | failure, expected by canary setup |
| Artifact | `perfgate-artifacts-25941883937-1` |
| Artifact ID | `7026765094` |
| Artifact digest | `184253bd75c3a316b6506949f429fb9d173752b1fa63f094ca319a1f2b5be727` |

Downloaded artifact contents:

```text
droid-node-version/comment.md
droid-node-version/compare.json
droid-node-version/repair_context.json
droid-node-version/report.json
droid-node-version/run.json
```

The action log printed:

```text
perfgate check exited with code 2
Verdict: fail (pass=3, warn=0, fail=1, benches=1)
Reproduce locally: perfgate check --config perfgate.toml --all --fail-on-warn --require-baseline
Uploaded artifact: perfgate-artifacts-25941883937-1
```

The artifact list in the action output named:

```text
artifacts/perfgate/droid-node-version/comment.md
artifacts/perfgate/droid-node-version/compare.json
artifacts/perfgate/droid-node-version/report.json
artifacts/perfgate/droid-node-version/run.json
```

The PR-ready comment reported:

```text
wall_ms baseline 1 ms, current 27 ms, delta +2600%, budget 20%, fail
```

`repair_context.json` recorded a failing status and recommended local follow-up
commands, including:

```text
rerun current command: node -e console.log(process.version)
perfgate explain --compare artifacts/perfgate/droid-node-version/compare.json
perfgate paired --name droid-node-version --baseline-cmd "<baseline-cmd>" --current-cmd "<current-cmd>" --repeat 10 --out artifacts/perfgate/droid-node-version/paired.json
perfgate compare --baseline baselines/droid-node-version.json --current artifacts/perfgate/droid-node-version/run.json --out artifacts/perfgate/droid-node-version/recompare.json
perfgate bisect --good <good-ref> --bad HEAD --executable <bench-binary>
```

## Finding

The hosted failure run exposed a shell bug in the action step-summary path:

```text
decision_repro_line: unbound variable
$'text\n    print_artifacts\n    echo ': command not found
```

The action still uploaded artifacts and printed enough console output to
reproduce the failure locally. The finding blocks product-claim promotion and
lane closeout until the action summary shell is fixed and the hosted canary is
rerun or otherwise revalidated.

Required follow-up:

```text
action: fix failure summary step-summary guard
```

The follow-up fix landed in perfgate `main` as commit
`978f1c211b2910c53918b522c01bdc8078381c33`.

The failed canary job was rerun as run `25941883937`, attempt `2`, job
`76268311506`. That rerun downloaded `EffortlessMetrics/perfgate@main` at
`978f1c211b2910c53918b522c01bdc8078381c33`, failed intentionally with the same
policy failure, printed the local reproduction command, and uploaded
`perfgate-artifacts-25941883937-2`.

The rerun log no longer contained:

```text
decision_repro_line: unbound variable
command not found
```

The rerun uploaded artifact ID `7027590462` with digest:

```text
fd637dd5c738437bce4e78ed458739f3f24d23ed6f8d61ea02ba6317c10d3e52
```

## What This Canary Proves

- A hosted external PR can run the perfgate GitHub Action from a non-perfgate
  repository.
- A committed baseline plus `require_baseline` works in an external hosted PR
  workflow.
- The action uploads perfgate artifacts on both pass and fail paths.
- The forced failure path prints a copyable local reproduction command.
- The forced failure path creates `repair_context.json` with local next
  commands.
- The hosted step-summary shell fix prevents Markdown fences from being treated
  as Bash command substitution and keeps decision-mode reproduction optional
  when `decision: false`.

## What This Canary Does Not Prove

- It does not prove the public `0.18.0` release, `v0.18`, or `v0` action
  aliases. The canary used `EffortlessMetrics/perfgate@main`.
- It does not prove every external repository shape or hosted runner.
- It does not prove server-ledger correctness.
- It does not prove a probe-backed external canary.
- It does not prove full external CI health for the target repository. The
  unrelated `Droid Auto Review` workflow failed separately and is not perfgate
  evidence.

## Cleanup

The canary used a temporary local worktree and downloaded artifact directories
under `C:\perfgate-canaries`. Those local files are evidence caches only; the
durable evidence is this audit plus the external PR and workflow run URLs.
