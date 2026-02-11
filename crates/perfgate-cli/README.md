# perfgate

Performance budgets and baseline diffs for CI / PR bots.

## Installation

```bash
cargo install perfgate
```

## Usage

```bash
# Run a benchmark
perfgate run --name my-bench --out run.json -- ./my-benchmark

# Compare against baseline
perfgate compare --baseline baseline.json --current run.json --out compare.json

# Generate markdown for PR comments
perfgate md --compare compare.json

# Generate GitHub annotations
perfgate github-annotations --compare compare.json

# Generate structured report
perfgate report --compare compare.json --out report.json

# Promote current run to baseline
perfgate promote --current run.json --to baselines/my-bench.json

# Export data
perfgate export --run run.json --format csv --out data.csv

# Check against budget config
perfgate check --config perfgate.toml --bench my-bench

# Check in cockpit mode
perfgate check --config perfgate.toml --bench my-bench --mode cockpit

# Check all benchmarks
perfgate check --config perfgate.toml --all

# Paired benchmarking
perfgate paired --baseline-cmd "sleep 0.01" --current-cmd "sleep 0.02" --repeat 10 --out compare.json
```

## Exit Codes

- `0` - Success (or warn without `--fail-on-warn`)
- `1` - Tool/runtime error (I/O, parse, spawn failures)
- `2` - Policy fail (budget violated)
- `3` - Warn treated as failure (with `--fail-on-warn`)

## Part of perfgate

This is the CLI crate for the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
