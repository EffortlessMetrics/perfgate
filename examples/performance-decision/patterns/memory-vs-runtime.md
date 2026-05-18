# Memory Regression With Runtime Improvement

## Shape

The workload finishes faster by using more memory.

```text
wall_ms       improved 9.8%
max_rss_kb    regressed 3.1%
```

## Why It Matters

This is one of the most common useful tradeoffs. It may be acceptable for a
batch job or developer tool, but unacceptable for memory-constrained runners,
serverless functions, or embedded deployments.

## Receipts To Inspect

```text
compare.json
report.json
tradeoff.json
decision.md
repair_context.json
```

## Reviewer Action

Check whether the memory regression is inside the configured local cap and
whether the runtime improvement applies to the workload that actually matters.
If the tradeoff is accepted, keep the decision bundle with the review.

```bash
perfgate decision evaluate --config perfgate.toml
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

## Do Not

- Do not compare memory and runtime by raw percent alone.
- Do not accept memory growth without deployment context.
- Do not make the server ledger part of merge correctness.
