# perfgate-significance

Statistical significance testing for benchmarking.

Part of the [perfgate](https://github.com/nicholasgasior/perfgate) workspace.

## Overview

Implements Welch's t-test for detecting statistically significant performance
changes between benchmark runs. Handles unequal variances and sample sizes,
making it robust for real-world CI benchmarking.

## Key API

- `compute_significance(baseline, current, alpha, min_samples)` — Welch's t-test returning `Option<Significance>`
- `mean_and_variance(values)` — sample mean and Bessel-corrected variance

## Methodology

- **Test**: Two-tailed Welch's t-test (unequal variance t-test)
- **Degrees of freedom**: Welch–Satterthwaite approximation
- **Edge cases**: Handles zero-variance samples explicitly
- **Minimum samples**: Requires ≥ `min_samples` in both groups (typically 8)

## Example

```rust
use perfgate_significance::compute_significance;

let baseline = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0, 101.0, 99.0];
let current  = vec![110.0, 112.0, 108.0, 111.0, 109.0, 110.0, 111.0, 109.0];

let sig = compute_significance(&baseline, &current, 0.05, 8).unwrap();
assert!(sig.significant);   // Clear performance regression
assert!(sig.p_value < 0.05);
assert_eq!(sig.baseline_samples, 8);
```

## License

Licensed under either Apache-2.0 or MIT.
