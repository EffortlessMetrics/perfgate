# perfgate-adapters

Platform adapters for perfgate: process runner, system metrics, and filesystem operations.

This crate provides the infrastructure layer:

- **Process runner** - Execute benchmarks and capture timing/metrics
- **System metrics** - Collect `max_rss_kb` and other resource usage (Unix via `rusage`)
- **Timeout support** - Process timeouts with polling (Unix only)

## Platform Support

- **Unix** - Full support including memory metrics and timeouts
- **Windows** - Basic support (timeouts return `AdapterError::TimeoutUnsupported`)

## Part of perfgate

This crate is part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace for performance budgets and baseline diffs in CI.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
