# perfgate-budget

Threshold-based verdict computation -- the decision engine that turns metric
deltas into pass/warn/fail outcomes.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## How it works

Given a baseline value, a current value, and a `Budget` (thresholds +
direction), the crate computes the regression percentage and maps it to a
status:

| Condition | Status |
|---|---|
| `regression > threshold` | **Fail** |
| `warn_threshold <= regression <= threshold` | **Warn** |
| `regression < warn_threshold` | **Pass** |

Direction matters: for `Lower`-is-better metrics (e.g., latency), an increase
is a regression; for `Higher`-is-better (e.g., throughput), a decrease is.

When multiple metrics are evaluated, `aggregate_verdict` collapses them:
Fail dominates Warn dominates Pass.

## Key types

| Type | Role |
|---|---|
| `Budget` | Fail threshold, warn threshold, direction, noise policy |
| `BudgetResult` | Per-metric outcome: ratio, pct change, regression, status |
| `Verdict` | Aggregated status + per-status counts + reason tokens |
| `BudgetError` | `InvalidBaseline` (zero/negative), `NoSamples` |

## API

| Function | Purpose |
|---|---|
| `evaluate_budget(baseline, current, budget, cv)` | Single metric evaluation |
| `evaluate_budgets(metrics, budgets)` | Batch evaluation with verdict aggregation |
| `calculate_regression(baseline, current, direction)` | Raw regression fraction |
| `determine_status(regression, threshold, warn)` | Map regression to status |
| `aggregate_verdict(statuses)` | Collapse multiple statuses into one verdict |
| `reason_token(metric, status)` | Structured token, e.g. `wall_ms_fail` |

## Example

```rust
use perfgate_budget::{evaluate_budget, aggregate_verdict};
use perfgate_types::{Budget, Direction, MetricStatus, VerdictStatus};

let budget = Budget::new(0.20, 0.10, Direction::Lower);

let result = evaluate_budget(100.0, 115.0, &budget, None).unwrap();
assert_eq!(result.status, MetricStatus::Warn); // 15% regression

let verdict = aggregate_verdict(&[MetricStatus::Pass, MetricStatus::Warn]);
assert_eq!(verdict.status, VerdictStatus::Warn); // Warn dominates
```

## License

Licensed under either Apache-2.0 or MIT.
