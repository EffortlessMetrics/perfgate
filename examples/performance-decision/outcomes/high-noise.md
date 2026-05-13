# Outcome: High Noise

## Scenario

Evidence exists, but the configured noise policy says it is too noisy for
automatic acceptance.

## Input Receipts

```text
artifacts/perfgate/parser/compare.json
artifacts/perfgate/probe-compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.index.json
```

## decision.md Excerpt

```text
Decision: warn
Review required: yes
Reason: coefficient of variation exceeded policy
```

## Action Summary Excerpt

```text
perfgate decision: warn, high noise
Noise policy: needs review
Reproduce: perfgate check --config perfgate.toml --all --require-baseline
```

## Reviewer Action

Rerun under steadier conditions, inspect whether the noise is expected for the
workload, or require explicit human approval before accepting the tradeoff.

## Local Reproduction

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
```

