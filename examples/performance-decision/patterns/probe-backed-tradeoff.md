# Probe Regression With Dominant Workload Improvement

## Shape

A local probe regresses, but the dominant scenario or workload improves enough
to justify the change.

```text
parser.tokenize      regressed 2.0%
parser.batch_loop    improved 11.0%
weighted scenario    improved 8.5%
```

## Why It Matters

Probes should explain where work moved. They are not a profiler replacement.
This pattern is useful when reviewers need to see that a local cost is bounded
and the overall workload improved.

## Receipts To Inspect

```text
probe-compare.json
scenario.json
tradeoff.json
decision.md
decision.index.json
decision-bundle.json
```

## Reviewer Action

Check that the regressed probe is covered by a local cap and that the dominant
probe or weighted scenario improved. Bundle the decision when it needs to
travel with a PR, release, or agent handoff.

```bash
perfgate ingest probes --file probes-current.jsonl --out artifacts/perfgate/probes-current.json
perfgate decision evaluate --config perfgate.toml
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

## Do Not

- Do not add probes before the benchmark shows a meaningful tradeoff.
- Do not accept a local probe regression without a configured cap.
- Do not treat probe evidence as a substitute for the outside benchmark result.
