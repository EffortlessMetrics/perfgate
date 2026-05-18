# Latency Regression With Throughput Improvement

## Shape

A request-path latency metric regresses, while a throughput metric improves.
The raw signs differ by metric direction: higher throughput can be good while
higher latency is bad.

```text
wall_ms             regressed 2.4%
throughput_per_s    improved 12.4%
```

## Why It Matters

This can be a real product win when the changed code improves batch capacity or
worker throughput and the latency regression stays inside accepted policy. It
can also be a bad tradeoff if the latency metric represents the user-facing
path that reviewers care about.

## Receipts To Inspect

```text
compare.json
scenario.json
tradeoff.json
decision.md
decision.index.json
```

## Reviewer Action

Use a structured decision when both movements exceed policy thresholds and the
project has scenario weights or tradeoff policy that says which workload
matters.

```bash
perfgate decision evaluate --config perfgate.toml
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

## Do Not

- Do not call a throughput improvement a regression because the percent is
  positive.
- Do not accept the tradeoff unless the latency regression is bounded by
  policy.
- Do not loosen the latency threshold just to make the review pass.
