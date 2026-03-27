# perfgate-summary

Summarization logic for perfgate comparison receipts.

## Overview

`perfgate-summary` aggregates one or more `CompareReceipt` files (produced by
`perfgate compare`) into a compact summary table. It powers the
`perfgate summary` CLI command, giving you a quick at-a-glance view of how
multiple benchmarks performed in a single CI run.

Glob patterns are supported, so you can point it at a directory of comparison
files and get a unified table.

## Key Types

- `SummaryRequest` — input: a list of file paths or glob patterns to summarize.
- `SummaryRow` — one row in the output table: benchmark name, verdict status,
  wall-clock time, and percentage change.
- `SummaryOutcome` — the complete result: a vector of rows plus a `failed` flag
  indicating whether any benchmark had a `fail` verdict.
- `SummaryUseCase` — the entry point that executes the summarization.

## Key Methods

| Method | Description |
|--------|-------------|
| `SummaryUseCase::execute(request)` | Reads and parses all matching comparison receipts, extracts wall-time deltas, and returns a `SummaryOutcome` |
| `SummaryUseCase::render_markdown(outcome)` | Renders the outcome as a Markdown table suitable for PR comments |

## Example

```rust
use perfgate_summary::{SummaryRequest, SummaryUseCase};

let usecase = SummaryUseCase;
let outcome = usecase.execute(SummaryRequest {
    files: vec!["artifacts/**/*.compare.json".to_string()],
})?;

if outcome.failed {
    eprintln!("One or more benchmarks failed!");
}

let table = usecase.render_markdown(&outcome);
println!("{}", table);
```

The rendered Markdown looks like:

```
| Benchmark | Status | Wall (ms) | Change |
|-----------|--------|-----------|--------|
| bench-a   | pass   | 42.50     | -2.1%  |
| bench-b   | fail   | 310.00    | +15.3% |
```

## CLI Usage

```bash
perfgate summary artifacts/perfgate/compare*.json
```

## Workspace Role

`perfgate-summary` is consumed by the application and CLI layers:

`perfgate-types` -> **`perfgate-summary`** -> `perfgate-app` -> `perfgate-cli`

## License

Licensed under either Apache-2.0 or MIT.
