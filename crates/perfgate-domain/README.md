# perfgate-domain

Pure domain logic for perfgate: statistics, budgets, and comparisons.

This crate contains the core business logic with **no I/O dependencies**:

- Statistical calculations (mean, standard deviation, percentiles)
- Budget policy evaluation (pass/warn/fail thresholds)
- Baseline comparison logic
- Regression detection algorithms

## Design Philosophy

This crate is intentionally I/O-free to ensure:
- Easy testing with pure functions
- Deterministic behavior
- Clear separation of concerns

## Part of perfgate

This crate is part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace for performance budgets and baseline diffs in CI.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
