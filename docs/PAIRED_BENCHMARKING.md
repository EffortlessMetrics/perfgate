# Paired Benchmarking

Paired benchmarking runs baseline and current commands in interleaved fashion
(B, C, B, C, ...) to cancel out environmental noise. Each pair is measured
back-to-back to minimize variance from system load fluctuations.

## When to Use

- Noisy CI runners with variable system load
- When you need high-confidence measurements
- Comparing two different implementations directly

## Usage

```bash
perfgate paired \
  --baseline-cmd "sleep 0.01" \
  --current-cmd "sleep 0.02" \
  --repeat 10 \
  --threshold 0.20 \
  --out artifacts/perfgate/compare.json
```

The output is a standard `perfgate.compare.v1` receipt, compatible with `md`,
`report`, `export`, and all other downstream commands.

## Significance-based Retries

The `paired` command supports automatic retries if statistical significance is
not reached:

```bash
perfgate paired \
  --baseline-cmd "./bench-old" \
  --current-cmd "./bench-new" \
  --repeat 10 \
  --max-retries 3 \
  --significance-alpha 0.05 \
  --out compare.json
```

If the initial run doesn't reach significance, perfgate will retry up to
`--max-retries` times with the same parameters before reporting the final result.
