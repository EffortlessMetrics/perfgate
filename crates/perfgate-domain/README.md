# perfgate-domain

Pure business logic with zero I/O -- statistics, budget evaluation, and verdict
computation for perfgate.

This crate is intentionally I/O-free. All data arrives via function arguments;
there is no filesystem access, no network, no process spawning. This makes every
function deterministic and trivially testable.

## Core API

### Statistics

- `compute_stats(samples, work_units) -> Stats` -- aggregate summary statistics
  from raw samples (excludes warmup iterations)
- Re-exports from `perfgate-stats`: `summarize_u64`, `summarize_f64`,
  `median_u64_sorted`, `median_f64_sorted`

### Budget Evaluation

- `compare_stats(baseline, current, budgets) -> Comparison` -- evaluate each
  metric against budget thresholds, producing deltas and a per-metric verdict
- `compare_runs(baseline, current, budgets, statistics, significance) ->
  Comparison` -- full run-level comparison with per-metric statistic selection
  (median or p95) and optional Welch's t-test significance gating
- Re-exports from `perfgate-budget`: `evaluate_budget`, `evaluate_budgets`,
  `calculate_regression`, `determine_status`, `aggregate_verdict`

### Report Derivation

- `derive_report(receipt) -> Report` -- extract warn/fail findings from a
  `CompareReceipt`, sorted by metric name for deterministic output

### Host Mismatch Detection

- `detect_host_mismatch(baseline, current) -> Option<HostMismatchInfo>` --
  triggers on OS/arch mismatch, CPU count >2x difference, memory >2x
  difference, or hostname hash mismatch

### Significance Testing

- `compute_significance(baseline_samples, current_samples, alpha) ->
  SignificanceResult` -- Welch's t-test with configurable alpha
- `SignificancePolicy` -- controls alpha, minimum sample count, and whether
  non-significant regressions are downgraded to pass

### Paired Benchmarking

- `compute_paired_stats(samples) -> PairedStats`
- `compare_paired_stats(stats, budgets) -> PairedComparison`

### Dependency Blame

- `compare_lockfiles(baseline, current) -> BinaryBlame` -- diff Cargo.lock
  files to identify added, removed, and updated dependencies

## Key Types

`Comparison` (deltas + verdict), `SignificancePolicy`, `Report`, `Finding`,
`FindingData`, `DomainError` (`NoSamples`, `Stats`, `InvalidAlpha`).

## Verdict Logic

A metric is **Fail** if regression exceeds the budget threshold, **Warn** if it
exceeds the warn threshold, **Pass** otherwise. The aggregate verdict is the
worst status across all evaluated metrics. All output uses `BTreeMap` for
deterministic ordering.

## Testing

- Unit tests: median, host mismatch boundaries, significance gating
- Property tests (proptest): host mismatch symmetry, budget boundary conditions
- Mutation testing target: **100% kill rate**

## License

Licensed under either Apache-2.0 or MIT.
