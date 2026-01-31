# Product Overview

perfgate is a Rust CLI for performance budgets and baseline diffs in CI/PR workflows.

## Core Purpose
- Run benchmarks and emit versioned JSON receipts
- Compare current runs against baselines with configurable thresholds
- Render Markdown tables for PR comments
- Emit GitHub Actions annotations

## Key Features
- Median-based, thresholded policy defaults
- Wall-clock time gating with optional throughput metrics
- Unix: collects `ru_maxrss` via `wait4()`
- Configurable warn/fail thresholds per metric
- Atomic file writes for receipts

## Exit Codes (compare command)
- `0`: pass (or warn without `--fail-on-warn`)
- `1`: tool error
- `2`: budget violated (fail)
- `3`: warn treated as failure

## Output Schemas
- `perfgate.run.v1` - benchmark run receipts
- `perfgate.compare.v1` - comparison results
