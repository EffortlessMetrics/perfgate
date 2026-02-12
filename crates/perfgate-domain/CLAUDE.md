# perfgate-domain

Pure domain logic for perfgate — statistics, budget evaluation, and comparison policy. **Intentionally I/O-free.**

## Build and Test

```bash
cargo test -p perfgate-domain
```

Mutation testing target: **100% kill rate**.

## What This Crate Contains

All the math and policy logic. No file I/O, no network, no process spawning.

### Source Layout

- `src/lib.rs` — Statistics, comparison, report derivation, host mismatch detection
- `src/paired.rs` — Paired comparison logic (`compute_paired_stats`, `compare_paired_stats`)

### Key Functions

**Statistics:**
- `summarize_u64(values) -> U64Summary` — Median, min, max from a slice
- `summarize_f64(values) -> F64Summary` — Same for floats
- `compute_stats(samples, work_units) -> Stats` — Full stats from samples (excludes warmup)

**Comparison:**
- `compare_stats(baseline, current, budgets) -> Comparison` — Core budget evaluation. Calculates ratio, percentage change, regression detection, and applies budget thresholds. Returns `Comparison` with deltas and verdict.
- `metric_value(stats, metric) -> Option<f64>` — Extracts median value for a metric

**Report derivation:**
- `derive_report(receipt) -> Report` — Extracts warn/fail findings from comparison deltas

**Host mismatch detection:**
- `detect_host_mismatch(baseline, current) -> Option<HostMismatchInfo>` — Triggers on: OS/arch mismatch, CPU count >2x difference, memory >2x difference, hostname hash mismatch

**Paired:**
- `compute_paired_stats(samples) -> PairedStats`
- `compare_paired_stats(stats, budgets) -> PairedComparison`

### Key Types

- `DomainError` — `NoSamples`, `InvalidBaseline` (thiserror)
- `Comparison` — `deltas: BTreeMap<Metric, Delta>`, `verdict: Verdict`
- `Report` — `verdict`, `findings` (sorted by metric name)
- `Finding` — `code`, `check_id`, `data: FindingData`
- `FindingData` — metric details: name, baseline, current, regression_pct, threshold

## Design Rules

- **Zero I/O** — This crate must never import `std::fs`, `std::io`, `std::net`, or `std::process`. All data comes in via function arguments.
- **Deterministic output** — Uses `BTreeMap` for ordered deltas; findings sorted by metric name.
- **Budget evaluation logic**: A metric is `Fail` if regression exceeds the budget threshold, `Warn` if it exceeds the warn threshold, `Pass` otherwise. The verdict aggregates all metrics.

## Testing

- **Unit tests**: Median calculation, host mismatch detection with threshold boundaries
- **Property tests** (proptest): Host mismatch symmetry and boundary conditions
