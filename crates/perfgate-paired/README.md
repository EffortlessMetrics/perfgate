# perfgate-paired

CI runners are noisy. CPU frequency scaling, background daemons, and
shared-tenancy VMs mean that a single-run comparison can easily produce a
false-positive regression alert.

`perfgate-paired` solves this with **interleaved A/B benchmarking**: baseline
and current commands alternate within the same execution window, so
environmental noise affects both sides equally and cancels out in the paired
difference.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## How it works

1. **Alternating execution** -- each "pair" runs baseline then current
   back-to-back, sharing the same thermal/load conditions.
2. **Paired t-test** -- the difference distribution (current - baseline) is
   tested for statistical significance with a 95% confidence interval.
3. **Significance-based retries** -- if `require_significance` is set and
   the CI still spans zero, additional pairs are collected automatically
   (up to `max_retries`).
4. **Warmup rounds** -- configurable warmup pairs are excluded from
   statistics so JIT, caches, and page faults stabilize first.

## Key API

| Function | Returns | Purpose |
|---|---|---|
| `compute_paired_stats(samples, work_units, policy)` | `PairedStats` | Summary statistics for wall time, RSS, and throughput diffs |
| `compare_paired_stats(stats)` | `PairedComparison` | Confidence interval and significance flag |
| `summarize_paired_diffs(diffs, policy)` | `PairedDiffSummary` | Mean, median, std dev, min/max, optional significance |

## Statistical methodology

- **t-value**: 2.0 for n < 30 (conservative), 1.96 for n >= 30
- **95% CI**: `mean +/- t * (std_dev / sqrt(n))`
- **Significant**: the CI does not span zero *and* `n >= min_samples`

## Example

```rust
use perfgate_paired::{compute_paired_stats, compare_paired_stats};

// After collecting interleaved paired samples...
let stats = compute_paired_stats(&samples, None, None)?;
let cmp = compare_paired_stats(&stats);

println!("mean diff:   {:.2} ms", cmp.mean_diff_ms);
println!("95% CI:      [{:.2}, {:.2}]", cmp.ci_95_lower, cmp.ci_95_upper);
println!("significant: {}", cmp.is_significant);
```

## License

Licensed under either Apache-2.0 or MIT.
