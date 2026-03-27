# perfgate-stats

Descriptive statistics for performance data -- the numerical foundation of
perfgate's budget evaluation pipeline.

Pure functions with no I/O dependencies. Used by `perfgate-domain` and can be
independently tested and versioned.

## API

### Summary Statistics

- `summarize_u64(values) -> U64Summary` -- median, min, max, mean, stddev
- `summarize_f64(values) -> F64Summary` -- same for `f64` samples

### Median (pre-sorted)

- `median_u64_sorted(sorted)` -- overflow-safe `u64` median via split-halves
- `median_f64_sorted(sorted)` -- `f64` median

### Percentile

- `percentile(values, q)` -- q-th percentile (q in [0, 1]) with linear
  interpolation between adjacent ranks

### Mean and Variance

- `mean_and_variance(values) -> Option<(f64, f64)>` -- arithmetic mean and
  Bessel-corrected sample variance using Welford's online algorithm for
  numerical stability; returns `None` for empty or non-finite input

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

## Numerical Stability

`mean_and_variance` uses Welford's one-pass online algorithm, avoiding the
catastrophic cancellation that affects naive two-pass (sum-of-squares) methods.
Results are validated as finite before returning.

The `u64` median uses a split-halves technique (`a/2 + b/2 + remainder`) to
avoid overflow at `u64::MAX` boundaries.

## Testing

- Property-based (proptest): ordering invariants, overflow handling, percentile
  bounds, mean correctness against naive computation
- Benchmarks via Criterion (`cargo bench -p perfgate-stats`)

## License

Licensed under either Apache-2.0 or MIT.
