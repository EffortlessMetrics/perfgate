# ADR 0008: Statistical Significance for Gating

## Status
Accepted

## Context
Benchmark measurements are inherently noisy. A 5% wall-time increase might be a real regression or just environmental variance (system load, thermal throttling, background processes). Naive threshold-based gating produces false positives, leading teams to ignore performance gates entirely.

## Decision
We integrate Welch's t-test as an optional significance layer on top of threshold-based gating:

1. **Thresholds remain the primary gate.** A metric must exceed its configured threshold (e.g., 20% regression) to trigger a verdict.
2. **Significance is opt-in** via `--significance-alpha` (default: disabled). When enabled, the comparison includes p-value metadata.
3. **`--require-significance`** makes significance a hard gate: if the sample size is too small to reach statistical significance, the verdict is suppressed (no false alarm).
4. **Noise policy** (`ignore`, `warn`, `skip`) handles metrics with high coefficient of variation — benchmarks that are inherently unstable can be flagged and optionally excluded from gating.
5. **Paired benchmarking** (`perfgate paired`) further reduces noise by interleaving baseline and current measurements back-to-back, canceling out environmental drift.

The implementation lives in `perfgate-significance` (Welch's t-test, confidence intervals) and `perfgate-domain` (noise policy evaluation).

## Consequences
- Teams can tune their confidence level per-project, balancing sensitivity against false-positive rates.
- The `compare` receipt includes p-value and confidence interval metadata for audit.
- Paired mode is recommended for noisy CI environments but adds ~2x runtime.
- The default (significance disabled) preserves backward compatibility with simple threshold gating.
