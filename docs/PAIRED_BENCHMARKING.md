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
  --name my-bench \
  --baseline-cmd "sleep 0.01" \
  --current-cmd "sleep 0.02" \
  --repeat 10 \
  --fail-on-regression 20.0 \
  --out artifacts/perfgate/compare.json
```

The output is a standard `perfgate.compare.v1` receipt, compatible with `md`,
`report`, `export`, and all other downstream commands.

## Significance-based Retries

The `paired` command supports automatic retries if statistical significance is
required but not reached:

```bash
perfgate paired \
  --name my-bench \
  --baseline-cmd "./bench-old" \
  --current-cmd "./bench-new" \
  --repeat 10 \
  --significance-alpha 0.05 \
  --require-significance \
  --max-retries 3 \
  --out compare.json
```

Retries only run when `--require-significance` is set. If the initial measured
pairs do not reach significance, each retry collects an adaptive extra batch:
retry 1 collects 2 extra pairs, retry 2 collects 3 extra pairs, retry 3 collects
5 extra pairs, and so on.

Use `--cv-threshold` to stop retrying early when the paired wall-time
differences are too noisy to trust:

```bash
perfgate paired \
  --name my-bench \
  --baseline-cmd "./bench-old" \
  --current-cmd "./bench-new" \
  --repeat 10 \
  --significance-alpha 0.05 \
  --require-significance \
  --max-retries 3 \
  --cv-threshold 0.50 \
  --out compare.json
```

The receipt includes noise diagnostics when retries are enabled, including CV,
noise level, retries used, and whether early termination occurred.
