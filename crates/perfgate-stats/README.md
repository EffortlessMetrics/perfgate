# perfgate-stats

Statistical functions for benchmarking analysis.

Part of the [perfgate](https://github.com/nicholasgasior/perfgate) workspace.

## Overview

Pure statistical functions with no I/O dependencies. Provides summary
statistics, percentile calculation, and mean/variance computation for
benchmark sample data.

## Key API

- `summarize_u64(values)` — compute median, min, max for `u64` samples → `U64Summary`
- `summarize_f64(values)` — compute median, min, max for `f64` samples → `F64Summary`
- `median_u64_sorted(sorted)` — median of pre-sorted `u64` slice (overflow-safe)
- `median_f64_sorted(sorted)` — median of pre-sorted `f64` slice
- `percentile(values, q)` — compute the q-th percentile (q ∈ [0, 1])
- `mean_and_variance(values)` — arithmetic mean and sample variance (Bessel-corrected)

## Example

```rust
use perfgate_stats::{summarize_u64, percentile, mean_and_variance};

let summary = summarize_u64(&[120, 100, 110, 105, 115]).unwrap();
assert_eq!(summary.median, 110);
assert_eq!(summary.min, 100);
assert_eq!(summary.max, 120);

let p95 = percentile(vec![1.0, 2.0, 3.0, 4.0, 5.0], 0.95).unwrap();
let (mean, var) = mean_and_variance(&[10.0, 20.0, 30.0]).unwrap();
```

## License

Licensed under either Apache-2.0 or MIT.
