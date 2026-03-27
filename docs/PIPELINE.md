# Step-by-Step Pipeline

The `check` command handles the full pipeline automatically. This guide shows
each step individually, which is useful for custom workflows or debugging.

## 1. Run a benchmark

```bash
perfgate run \
  --name pst_extract \
  --repeat 7 \
  --warmup 1 \
  --work 1000 \
  --out artifacts/perfgate/run.json \
  -- \
  sh -c 'sleep 0.02'
```

This produces a `perfgate.run.v1` receipt with timing data, system metrics, and
host fingerprint.

## 2. Compare against a baseline

```bash
perfgate compare \
  --baseline baselines/pst_extract.json \
  --current artifacts/perfgate/run.json \
  --threshold 0.20 \
  --warn-factor 0.90 \
  --metric-stat wall_ms=p95 \
  --significance-alpha 0.05 \
  --significance-min-samples 8 \
  --out artifacts/perfgate/compare.json
```

`--metric-stat` selects `median` or `p95` per metric. With `--significance-alpha`,
the comparison includes p-value metadata (Welch's t-test). Add
`--require-significance` to require significance before warn/fail escalation.

## 3. Render a PR comment

```bash
perfgate md \
  --compare artifacts/perfgate/compare.json \
  --out artifacts/perfgate/comment.md

# With a custom Handlebars template
perfgate md \
  --compare artifacts/perfgate/compare.json \
  --template .github/perfgate-comment.hbs \
  --out artifacts/perfgate/comment.md
```

## 4. Emit GitHub Actions annotations

```bash
perfgate github-annotations --compare artifacts/perfgate/compare.json
```

## 5. Generate a cockpit report

```bash
perfgate report \
  --compare artifacts/perfgate/compare.json \
  --out artifacts/perfgate/report.json
```

## 6. Promote to baseline

After merging to main:

```bash
perfgate promote \
  --current artifacts/perfgate/run.json \
  --to baselines/pst_extract.json

# Or promote to cloud storage
perfgate promote \
  --current artifacts/perfgate/run.json \
  --to s3://my-perfgate-baselines/pst_extract.json
```

## 7. Export for trend analysis

```bash
perfgate export --run run.json --format csv --out data.csv
perfgate export --run run.json --format jsonl --out data.jsonl
perfgate export --compare compare.json --format html --out summary.html
perfgate export --compare compare.json --format prometheus --out metrics.prom
perfgate export --compare compare.json --format junit --out results.xml
```
