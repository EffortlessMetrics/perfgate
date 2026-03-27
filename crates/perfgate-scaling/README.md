# perfgate-scaling

Computational complexity validation and curve fitting.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Overview

Pure mathematical library for validating that benchmarks conform to expected
algorithmic complexity classes. Runs curve fitting against multiple models
and reports the best match. No I/O dependencies.

## Key API

- `classify_complexity(measurements, threshold)` -- fit all models and return the best match
- `parse_complexity("O(n)")` -- parse a complexity class from a string
- `is_complexity_degraded(expected, actual)` -- check if detected complexity is worse than expected
- `render_ascii_chart(measurements, best_fit, coefficients, width, height)` -- ASCII visualization
- `ComplexityClass` -- enum of O(1), O(log n), O(n), O(n log n), O(n^2), O(n^3), O(2^n)

## Example

```rust
use perfgate_scaling::{SizeMeasurement, classify_complexity, ComplexityClass};

let measurements = vec![
    SizeMeasurement { input_size: 100, time_ms: 10.0 },
    SizeMeasurement { input_size: 200, time_ms: 20.0 },
    SizeMeasurement { input_size: 400, time_ms: 40.0 },
    SizeMeasurement { input_size: 800, time_ms: 80.0 },
];

let result = classify_complexity(&measurements, None).unwrap();
assert_eq!(result.best_fit, ComplexityClass::ON);
assert!(result.r_squared > 0.99);
```

## License

Licensed under either Apache-2.0 or MIT.
