# Performance Decision Example

This fixture demonstrates the structured decision path without requiring a
benchmark harness. It uses committed compare receipts plus language-agnostic
probe JSONL so the decision is deterministic.

Run from the repository root:

```bash
perfgate ingest probes --file examples/performance-decision/probes-baseline.jsonl --out artifacts/perfgate/large-file/probes-baseline.json
perfgate ingest probes --file examples/performance-decision/probes-current.jsonl --out artifacts/perfgate/large-file/probes-current.json
perfgate decision evaluate --config examples/performance-decision/perfgate.toml
```

`decision evaluate` is the command to teach users. It consumes the configured
compare receipts, runs the configured probe comparison, evaluates scenario
weights and tradeoff policy, then writes the review-ready Markdown.

Expected shape:

```text
artifacts/perfgate/
  large-file/probe-compare.json
  scenario.json
  tradeoff.json
  decision.md
```

The fixture models a memory-for-speed decision:

- the weighted workload improves on `wall_ms`;
- `max_rss_kb` fails before tradeoff policy is applied;
- `parser.batch_loop` improves enough to satisfy the probe-backed requirement;
- `parser.tokenize` regresses, but stays under the configured 3% local cap;
- the final decision is `warn` with an accepted tradeoff.

`parser.tokenize` is also present in the probe comparison and regresses
slightly. That evidence remains visible in the generated `probe-compare.json`;
the tradeoff rule accepts the memory regression only because the local
tokenizer regression stays bounded and the dominant batch-loop probe improves.
