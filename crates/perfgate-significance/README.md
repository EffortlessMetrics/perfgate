# perfgate-significance

Statistical significance testing to separate real regressions from noise.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Methodology

Implements a two-tailed **Welch's t-test**, which handles unequal variances
and sample sizes -- exactly the conditions you get comparing CI benchmark
runs on shared runners.

- **Degrees of freedom**: Welch-Satterthwaite approximation
- **Minimum samples**: configurable (`min_samples`, typically 8); returns
  `None` when data is insufficient
- **Zero variance**: handled explicitly (p = 1.0 if means equal, 0.0 otherwise)
- **Variance**: Bessel-corrected (n-1 denominator) via Welford's one-pass algorithm

## Key API

- `compute_significance(baseline, current, alpha, min_samples) -> Option<Significance>`
- `mean_and_variance(values) -> Option<(f64, f64)>`

The returned `Significance` contains `p_value`, `alpha`, `significant`,
`baseline_samples`, `current_samples`, and optional confidence-interval bounds.

## Example

```rust
use perfgate_significance::compute_significance;

let baseline = vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0, 101.0, 99.0];
let current  = vec![110.0, 112.0, 108.0, 111.0, 109.0, 110.0, 111.0, 109.0];

let sig = compute_significance(&baseline, &current, 0.05, 8).unwrap();
assert!(sig.significant);
assert!(sig.p_value.unwrap() < 0.05);
```

## Limitations

- Assumes approximately normal data; highly skewed distributions may need
  non-parametric alternatives.
- Requires at least 2 observations per group (variance is undefined for n=1).

## License

Licensed under either Apache-2.0 or MIT.
