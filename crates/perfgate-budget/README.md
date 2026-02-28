# perfgate-budget

Budget evaluation logic for performance thresholds.

Part of the [perfgate](https://github.com/nicholasgasior/perfgate) workspace.

## Overview

Pure budget evaluation functions with no I/O dependencies. Handles threshold
checking, regression calculation, and verdict aggregation for performance
metrics against configurable budgets.

## Key API

- `evaluate_budget(baseline, current, budget)` — evaluate a single metric → `BudgetResult`
- `calculate_regression(baseline, current, direction)` — compute regression percentage
- `determine_status(regression, threshold, warn_threshold)` — map regression to Pass/Warn/Fail
- `aggregate_verdict(statuses)` — aggregate multiple statuses into a `Verdict`
- `evaluate_budgets(metrics, budgets)` — batch-evaluate multiple metrics
- `reason_token(metric, status)` — generate structured reason tokens

## Status Rules

| Condition                          | Status |
|------------------------------------|--------|
| regression > threshold             | Fail   |
| warn_threshold ≤ regression ≤ threshold | Warn   |
| regression < warn_threshold        | Pass   |

## Example

```rust
use perfgate_budget::{evaluate_budget, aggregate_verdict};
use perfgate_types::{Budget, Direction, MetricStatus, VerdictStatus};

let budget = Budget {
    threshold: 0.20,
    warn_threshold: 0.10,
    direction: Direction::Lower,
};

let result = evaluate_budget(100.0, 115.0, &budget).unwrap();
assert_eq!(result.status, MetricStatus::Warn);

let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Warn]);
assert_eq!(verdict.status, VerdictStatus::Warn);
```

## License

Licensed under either Apache-2.0 or MIT.
