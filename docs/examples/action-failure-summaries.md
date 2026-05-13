# Action Failure Summary Examples

These examples show the CI summary shapes reviewers should expect from the
composite GitHub Action. They are golden examples for the user-facing failure
copy, not a second behavior spec. The behavior contract lives in
[`PERFGATE-SPEC-0007`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md),
and the shell wiring is checked by `cargo +1.95.0 run -p xtask -- action-check`.

The exact artifact paths can vary by configuration. A useful summary must still
name the verdict, point to receipts, and show a local reproduction command.

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
