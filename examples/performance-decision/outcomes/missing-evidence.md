# Outcome: Missing Evidence

## Scenario

The config requires a receipt, probe, or metric that was not present in the
local artifact set.

## Input Receipts

```text
artifacts/perfgate/parser/compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.index.json
```

Missing expected receipt:

```text
artifacts/perfgate/probe-compare.json
```

## decision.md Excerpt

```text
Decision: warn
Review required: yes
Reason: required probe evidence is missing
```

## Action Summary Excerpt

```text
perfgate decision: warn, missing evidence
Missing artifact: artifacts/perfgate/probe-compare.json
Reproduce: perfgate decision evaluate --config perfgate.toml
```

## Reviewer Action

Fix the artifact path, add the missing probe emission, or change the tradeoff
policy in a separate review. Do not accept the tradeoff as proven while the
required evidence is absent.

## Local Reproduction

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
```

