# Outcome: Pass

## Scenario

The changed branch stays inside every configured performance budget. No
tradeoff rule is needed.

## Input Receipts

```text
artifacts/perfgate/parser/compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.index.json
```

## decision.md Excerpt

```text
Decision: pass
Reason: all weighted scenarios stayed inside policy
Review required: no
```

## Action Summary Excerpt

```text
perfgate decision: pass
Artifacts: artifacts/perfgate/
Reproduce: perfgate decision evaluate --config perfgate.toml
```

## Reviewer Action

Treat the performance decision as clean. Continue normal code review.

## Local Reproduction

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
```

