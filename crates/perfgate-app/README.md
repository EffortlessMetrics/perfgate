# perfgate-app

Application layer for perfgate: use-cases and output rendering.

This crate orchestrates the domain logic and adapters to implement:

- **Run use-case** - Execute benchmarks and produce run receipts
- **Compare use-case** - Compare baseline vs current results
- **Report use-case** - Generate structured reports
- **Markdown rendering** - PR comment formatting
- **GitHub annotations** - CI annotation output
- **Export functionality** - CSV and other formats

## Part of perfgate

This crate is part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace for performance budgets and baseline diffs in CI.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
