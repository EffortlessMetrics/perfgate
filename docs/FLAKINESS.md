# Flakiness History

Perfgate treats noisy benchmark data as part of the signal. A regression gate
is useful only when teams can tell the difference between a real slowdown and a
benchmark that is too unstable to trust.

The baseline service stores two wall-time noise fields on
`perfgate.verdict.v1` records:

| Field | Meaning |
|-------|---------|
| `wall_ms_cv` | The coefficient of variation for the current verdict's wall-time samples. This is stored as a ratio, so `0.30` means 30%. |
| `flakiness_score` | A 0.0 to 1.0 score derived from recent wall-time CV history for the same project and benchmark. |

## Score Contract

When a verdict is submitted, the server combines the current verdict's
`wall_ms_cv` with up to 19 previous verdicts for the same project and
benchmark. Invalid, missing, infinite, NaN, and negative CV values are ignored.
If no valid CV remains, `flakiness_score` is omitted.

The current scoring threshold for high wall-time noise is `0.30`.

```text
noisy_ratio = count(cv > 0.30) / valid_cv_count
mean_severity = average(min(cv / 0.30, 2.0) / 2.0)
flakiness_score = min(1.0, noisy_ratio * 0.7 + mean_severity * 0.3)
```

This intentionally gives more weight to repeated noisy verdicts than to one
isolated spike, while still surfacing severe single-run instability.

## Interpreting Scores

| Score | Meaning |
|-------|---------|
| `< 0.50` | Usually stable enough to read as a normal pass/warn/fail signal. |
| `0.50..0.74` | Elevated noise; inspect sample count, runner class, and benchmark setup. |
| `>= 0.75` | High noise; treat the benchmark as flaky until the cause is understood. |

These bands are display guidance, not automatic policy changes. Budget verdicts
still come from the compare receipt and configured noise policy.

## Operator Workflow

List the latest noisy benchmarks for a project:

```bash
perfgate baseline flaky --project my-project --min-score 0.50
```

Limit the scan to one benchmark:

```bash
perfgate baseline flaky --project my-project --benchmark parser --min-score 0.25
```

The dashboard verdict table also shows the latest wall-time CV and flakiness
score. Use the `Flaky` filter to focus on verdicts with score `>= 0.50`.

## Improving A Flaky Benchmark

Start with the runner and sampling setup before changing budgets:

1. Prefer a stable self-hosted runner for required gates.
2. Raise paired sample count or use `perfgate paired` for noisy A/B comparisons.
3. Increase warmups for benchmarks with one-time initialization cost.
4. Split benchmarks that mix unrelated workloads.
5. Set `noise_policy = "warn"` only when the benchmark is still useful but
   inherently noisy.
