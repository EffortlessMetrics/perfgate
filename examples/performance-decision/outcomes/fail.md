# Outcome: Fail

## Scenario

A required metric regressed beyond the configured budget and no tradeoff rule
accepted the change.

## Input Receipts

```text
artifacts/perfgate/parser/compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.index.json
```

## decision.md Excerpt

```text
Decision: fail
Failed metric: wall_ms
Reason: weighted workload exceeded configured budget
Review required: no
```

## Action Summary Excerpt

```text
perfgate decision: fail
Failed metric: wall_ms
Reproduce: perfgate check --config perfgate.toml --all --require-baseline
```

## Reviewer Action

Ask for a fix, a separate baseline update with evidence, or a policy change
that explains why the regression is intentional.

## Local Reproduction

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
```

