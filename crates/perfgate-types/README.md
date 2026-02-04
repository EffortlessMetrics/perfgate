# perfgate-types

Shared types and schemas for perfgate receipts.

This crate provides the core data structures used throughout the perfgate workspace:

- **Run receipts** (`perfgate.run.v1`) - Results from benchmark executions
- **Compare receipts** (`perfgate.compare.v1`) - Baseline vs current comparisons
- **Report receipts** (`perfgate.report.v1`) - Structured reports for CI integration
- **Configuration types** - Budget definitions and thresholds

## Features

- `arbitrary` - Enable structure-aware fuzzing support via the `arbitrary` crate

## Part of perfgate

This crate is part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace for performance budgets and baseline diffs in CI.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
