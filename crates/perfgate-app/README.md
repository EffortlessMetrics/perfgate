# perfgate-app

Application layer for perfgate: use-cases and output rendering.

This crate orchestrates the domain logic and adapters to implement:

- **Run use-case** - Execute benchmarks and produce run receipts
- **Compare use-case** - Compare baseline vs current results
- **Paired use-case** - Interleaved baseline/current benchmarking
- **Report use-case** - Generate structured reports
- **Sensor report** - Build `sensor.report.v1` envelopes for cockpit mode
- **Markdown rendering** - PR comment formatting
- **GitHub annotations** - CI annotation output
- **Export functionality** - CSV and JSONL formats
- **Promote** - Normalize and copy run receipts to baselines

## Part of perfgate

This crate is part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace for performance budgets and baseline diffs in CI.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
