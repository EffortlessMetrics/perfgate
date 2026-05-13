# Outcome: Warn With Accepted Tradeoff

## Scenario

A local metric regressed, but a configured tradeoff rule accepted it because a
more important workload or probe improved enough.

The deterministic fixture in `examples/performance-decision` uses this shape:
`max_rss_kb` warns, `parser.batch_loop` improves, and `parser.tokenize` stays
inside the local regression cap.

## Input Receipts

```text
artifacts/perfgate/large-file/probe-compare.json
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.index.json
```

## decision.md Excerpt

```text
Decision: warn
Accepted tradeoff: memory-for-batch-loop-speed
Review required: no
```

## Action Summary Excerpt

```text
perfgate decision: warn
Accepted tradeoff: memory-for-batch-loop-speed
Reproduce: perfgate decision evaluate --config perfgate.toml
```

## Reviewer Action

Review the accepted rule and the receipts that satisfied it. If the tradeoff is
intentional, the warning is expected evidence rather than a surprise failure.

## Local Reproduction

```bash
perfgate ingest probes --file examples/performance-decision/probes-baseline.jsonl --out artifacts/perfgate/large-file/probes-baseline.json
perfgate ingest probes --file examples/performance-decision/probes-current.jsonl --out artifacts/perfgate/large-file/probes-current.json
perfgate decision evaluate --config examples/performance-decision/perfgate.toml
```

