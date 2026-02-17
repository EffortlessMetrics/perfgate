# perfgate-domain

Pure, I/O-free policy and statistics logic for perfgate.

## Responsibilities

- Computes summary statistics from samples (`median`, `min`, `max`).
- Compares baseline vs current stats against metric budgets.
- Produces per-metric deltas and verdicts (pass/warn/fail).
- Derives structured findings/reports from compare receipts.
- Detects host mismatch signals between baseline and current runs.
- Provides paired-benchmark math (`compute_paired_stats`, `compare_paired_stats`).

## Boundaries

- No process spawning.
- No filesystem or network I/O.
- No CLI parsing or formatting concerns.

## Why This Layer Exists

The crate is intentionally pure so it stays easy to test, deterministic, and reusable from both CLI and higher-level orchestration code.

## Workspace Role

`perfgate-domain` sits above `perfgate-types` and below `perfgate-app`:

`perfgate-types` -> `perfgate-domain` -> `perfgate-app`

## License

Licensed under either Apache-2.0 or MIT.
