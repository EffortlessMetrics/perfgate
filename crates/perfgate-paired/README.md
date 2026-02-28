# perfgate-paired

Paired benchmarking statistics for A/B comparison.

Part of the [perfgate](https://github.com/nicholasgasior/perfgate) workspace.

## Overview

Provides statistical analysis for paired benchmark data where each measurement
consists of a baseline and current observation from the same experimental unit.
Uses paired t-test methodology with 95% confidence intervals.

## Key API

- `compute_paired_stats(samples, budget)` — compute summary stats from paired samples → `PairedStats`
- `compare_paired_stats(stats)` — compare with confidence intervals → `PairedComparison`
- `summarize_paired_diffs(samples)` — summarize the distribution of differences
- `PairedError` — error type (re-exported from `perfgate-error`)

## Statistical Methodology

- **Paired t-test** with conservative t-values (2.0 for n < 30, 1.96 for n ≥ 30)
- **95% CI**: `mean ± t × (std_dev / √n)`
- **Significance**: CI does not span zero

## Example

```rust
use perfgate_paired::{compute_paired_stats, compare_paired_stats};
use perfgate_types::{PairedSample, PairedSampleHalf};

// After collecting paired samples...
// let stats = compute_paired_stats(&samples, None)?;
// let comparison = compare_paired_stats(&stats);
// println!("Mean diff: {:.2}ms", comparison.mean_diff_ms);
// println!("Significant: {}", comparison.is_significant);
```

## License

Licensed under either Apache-2.0 or MIT.
