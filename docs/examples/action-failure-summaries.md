# Action Failure Summary Examples

These examples show the CI summary shapes reviewers should expect from the
composite GitHub Action. They are golden examples for the user-facing failure
copy, not a second behavior spec. The behavior contract lives in
[`PERFGATE-SPEC-0007`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md),
and the shell wiring is checked by `cargo +1.95.0 run -p xtask -- action-check`.

The exact artifact paths can vary by configuration. A useful summary must still
name the verdict, point to receipts, and show a local reproduction command.

## Policy Posture

Use this when the Action has run `perfgate check` and needs to show whether the
evidence is advisory, a maturity warning, a promotion candidate, or already
blocking because existing config says so. This summary does not change the
Action result.

```text
Policy posture:
Blocking behavior: this action preserves existing perfgate exit-code behavior; maturity guidance is advisory unless your config already makes it blocking.
Advisory signal: missing baselines remain setup guidance unless this workflow enables required-baseline mode.
Gate verdict: `pass` (check exit code `0`).
Policy doctor command:
  perfgate policy doctor --config perfgate.toml --out-dir artifacts/perfgate --bench parser
Review packet command:
  perfgate policy review-packet --config perfgate.toml --bench parser --out-dir artifacts/perfgate
Policy doctor output:
  recommended posture: gate_candidate
  missing: required-gate reviewer approval
Do not: make advisory maturity output blocking, loosen thresholds, promote baselines, or require server ledger mode from this summary alone.
```

The reviewer should treat `gate_candidate` as review-ready evidence, not as an
approved required gate. Promotion still requires a deliberate policy patch and
review.

## Missing Baseline

Use this when the first CI run has not promoted a baseline yet.

```text
Verdict: fail (pass=0, warn=0, fail=1, benches=1)
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
Next setup command:
  perfgate baseline promote --config perfgate.toml --all
Uploaded artifact: perfgate-artifacts-123456789-1
artifacts/perfgate/parser/run.json
artifacts/perfgate/parser/report.json
```

The reviewer should treat this as setup work unless a baseline was expected to
exist. Promote intentionally, commit `baselines/`, and rerun CI.

## Policy Failure

Use this when a required benchmark metric exceeded policy and no accepted
tradeoff applies.

```text
Verdict: fail (pass=0, warn=0, fail=1, benches=1)
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
Uploaded artifact: perfgate-artifacts-123456789-1
artifacts/perfgate/parser/compare.json
artifacts/perfgate/parser/report.json
artifacts/perfgate/parser/comment.md
```

The reviewer should inspect `compare.json` or `comment.md` first, then decide
whether the regression must be fixed or intentionally documented in a later
policy/baseline change.

## Warn With Accepted Tradeoff

Use this when decision mode accepts a local regression because configured
scenario, probe, and tradeoff evidence support the change.

```text
Verdict: warn (pass=0, warn=1, fail=0, benches=1)
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
  perfgate decision evaluate --config perfgate.toml
Uploaded artifact: perfgate-artifacts-123456789-1
artifacts/perfgate/parser/compare.json
artifacts/perfgate/parser/probe-compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
artifacts/perfgate/decision.index.json
```

The reviewer should read `decision.md`, then follow `decision.index.json` to the
probe and tradeoff receipts if the accepted tradeoff is surprising.

## Review Required

Use this when evidence exists but policy requires a human to inspect it before
the PR can rely on the decision.

```text
Verdict: warn (pass=0, warn=1, fail=0, benches=1)
Review required: tradeoff_review_required
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
  perfgate decision evaluate --config perfgate.toml
Uploaded artifact: perfgate-artifacts-123456789-1
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
artifacts/perfgate/decision.index.json
```

The reviewer should treat this as a performance review request, not as an
automatic pass. Check the named policy reason before accepting the tradeoff.

## Artifact Upload List

Use this to confirm the summary points to enough receipts for local reproduction
and asynchronous review.

```text
Uploaded artifact: perfgate-artifacts-123456789-1
artifacts/perfgate/parser/run.json
artifacts/perfgate/parser/compare.json
artifacts/perfgate/parser/report.json
artifacts/perfgate/parser/comment.md
artifacts/perfgate/parser/probe-compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
artifacts/perfgate/decision.index.json
```

The reviewer should be able to download the artifact and reproduce the same
decision locally from the listed receipt paths.

## Decision-Enabled Failure

Use this when `perfgate check` reaches a policy failure and decision mode is
enabled, but structured decision evaluation still rejects the change.

```text
Verdict: fail (pass=0, warn=0, fail=1, benches=1)
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
  perfgate decision evaluate --config perfgate.toml
Uploaded artifact: perfgate-artifacts-123456789-1
artifacts/perfgate/parser/compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
artifacts/perfgate/decision.index.json
```

The reviewer should read the structured decision first. A decision-enabled
failure means the action found or evaluated receipts, but policy still rejects
the performance change.

## Missing Benchmark Command

Use this when CI cannot start the configured benchmark command.

```text
perfgate check exited with code 1
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
Artifacts:
  artifacts/perfgate/ (no perfgate receipt files found)
```

The reviewer should inspect `perfgate.toml`, the benchmark command path, and
the CI runner environment before changing thresholds or promoting a baseline.
This is setup failure, not performance evidence.

## Wrong Baseline Path

Use this when CI can run the benchmark but cannot find the expected baseline
namespace or files.

```text
perfgate check exited with code 2
Verdict: fail (pass=0, warn=0, fail=1, benches=1)
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
Baseline bootstrap: after reviewing the first trusted run, promote it locally:
  perfgate baseline promote --config perfgate.toml --all
Artifacts:
artifacts/perfgate/parser/run.json
artifacts/perfgate/parser/report.json
```

The reviewer should decide whether this is a missing committed baseline,
wrong `baseline_dir`, wrong `baseline_pattern`, or a host/branch namespace
mismatch. Do not promote a baseline until the expected baseline source is clear.

## Artifact Upload Disabled

Use this when the action is configured with `upload_artifact: "false"`.

```text
Verdict: fail (pass=0, warn=0, fail=1, benches=1)
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
Artifacts:
artifacts/perfgate/parser/compare.json
artifacts/perfgate/parser/report.json
```

The reviewer can still use the printed local reproduction command. If the PR
needs asynchronous review, enable artifact upload or attach the relevant
receipts intentionally.

## Decision Missing Probe Evidence

Use this when decision mode is enabled but a scenario references missing probe
receipts.

```text
Verdict: warn (pass=0, warn=1, fail=0, benches=1)
Review required: tradeoff_review_required
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
  perfgate decision evaluate --config perfgate.toml
Artifacts:
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
artifacts/perfgate/decision.index.json
```

The reviewer should look for `probe evidence missing` in the scenario or
decision receipts. Missing probes can request review; they should not silently
accept a tradeoff that depends on named internal movement.

## Server Upload Failed

Use this when a workflow uploads decision receipts to a team ledger after the
local gate, but that upload fails.

```text
Verdict: warn (pass=0, warn=1, fail=0, benches=1)
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
  perfgate decision evaluate --config perfgate.toml
Artifacts:
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
artifacts/perfgate/decision.index.json
```

The reviewer should keep local receipts as the correctness contract. Treat a
server upload failure as an operations problem unless the repository explicitly
requires ledger persistence before merge.

## Review Required Fail Policy

Use this when `decision: "true"` and `review_required: "fail"` turn a
needs-review decision into a blocking check.

```text
perfgate decision review exited with code 2
Verdict: warn (pass=0, warn=1, fail=0, benches=1)
Review required: tradeoff_review_required
Reproduce locally:
  perfgate check --config perfgate.toml --all --require-baseline
  perfgate decision evaluate --config perfgate.toml
Artifacts:
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
artifacts/perfgate/decision.index.json
```

The reviewer should inspect the reason before overriding the policy. This is
not a failed benchmark by itself; it is branch protection enforcing human
review for incomplete or noisy tradeoff evidence.

## Windows Path Or Shell Quoting

Use this when the reproduction command contains paths with spaces or
platform-specific separators.

```text
Reproduce locally:
  perfgate check --config perfgate.toml --bench parser --out-dir artifacts/perfgate/windows\ path --require-baseline
Artifacts:
artifacts/perfgate/windows path/parser/compare.json
```

The reviewer should copy the command exactly from the action summary. If a
shell rewrites the path, rerun from the repository root with an explicit
`--out-dir` and avoid editing receipt paths by hand.
