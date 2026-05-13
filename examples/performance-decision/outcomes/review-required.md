# Outcome: Review Required

## Scenario

The available evidence could satisfy a tradeoff, but policy says the evidence
is not trustworthy enough for automatic acceptance. Common reasons are missing
noise data, high CV, or incomplete named probe evidence.

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
Reason: required tradeoff evidence needs human review
```

## Action Summary Excerpt

```text
perfgate decision: warn, review required
Review policy: warn
Reproduce: perfgate decision evaluate --config perfgate.toml
```

## Reviewer Action

Inspect the receipt that triggered review. Do not treat review-required as the
same thing as an accepted automatic tradeoff.

## Local Reproduction

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
```

