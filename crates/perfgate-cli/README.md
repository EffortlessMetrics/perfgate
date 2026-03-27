# perfgate

**One tool to run benchmarks, detect regressions, and gate CI.**

You ship code daily. Performance regressions slip through because measuring,
comparing, and enforcing budgets are three separate problems. perfgate solves
all three in a single binary that fits into any CI pipeline.

## Install

```bash
# Pre-built binary (via cargo-binstall)
cargo binstall perfgate-cli

# From crates.io
cargo install perfgate-cli

# From source
cargo install --path crates/perfgate-cli
```

## Quick Start

```bash
# 1. Measure
perfgate run --name my-bench --out run.json -- ./my-benchmark

# 2. Compare against a baseline
perfgate compare --baseline baseline.json --current run.json --out cmp.json

# 3. Gate CI (exit 2 on regression)
perfgate report --compare cmp.json --out report.json
```

Or use `check` for the full workflow in one command:

```bash
perfgate check --config perfgate.toml --bench my-bench
```

## Commands

Commands are organized by workflow stage.

### Measure

| Command   | Purpose |
|-----------|---------|
| `run`     | Execute a command N times, emit a `perfgate.run.v1` receipt |
| `paired`  | Interleaved A/B benchmarking to cancel out environmental noise |

### Analyze

| Command   | Purpose |
|-----------|---------|
| `compare` | Diff current vs baseline, emit a `perfgate.compare.v1` receipt |
| `blame`   | Identify which Cargo.lock dependency changes caused a regression |
| `explain` | Generate AI-ready diagnostic prompts for regressions |

### Report

| Command              | Purpose |
|----------------------|---------|
| `md`                 | Render Markdown from a compare receipt |
| `report`             | Generate a `perfgate.report.v1` envelope |
| `summary`            | Print a terminal table from one or more compare receipts |
| `github-annotations` | Emit `::error::`/`::warning::` lines for GitHub Actions |
| `export`             | Export to CSV, JSONL, HTML, Prometheus, or JUnit |

### Manage

| Command     | Purpose |
|-------------|---------|
| `promote`   | Copy a run receipt into baseline storage |
| `baseline`  | Manage baselines on a centralized server (list, upload, download, delete, history, verdicts, migrate) |
| `aggregate` | Merge multiple run receipts (e.g. from a fleet) into one |
| `fleet`     | Fleet-wide dependency regression alerts and impact analysis |

### Automate

| Command  | Purpose |
|----------|---------|
| `check`  | Config-driven end-to-end workflow: run, compare, report, gate |
| `bisect` | Binary-search for the commit that introduced a regression |

## Exit Codes

Every command follows the same contract:

| Code | Meaning |
|------|---------|
| `0`  | Success (or warn without `--fail-on-warn`) |
| `1`  | Runtime error (I/O, parse, spawn failure) |
| `2`  | Policy fail (budget violated) |
| `3`  | Warn treated as failure (`--fail-on-warn`) |

## Examples

```bash
# Paired A/B benchmarking (interleaved to reduce noise)
perfgate paired --name my-bench \
  --baseline-cmd "./old" --current-cmd "./new" --repeat 30 --out cmp.json

# Export metrics for Prometheus
perfgate export --run run.json --format prometheus --out metrics.prom

# Find the commit that introduced a regression
perfgate bisect --good v1.0.0 --bad HEAD --executable ./target/release/my-bench

# Cockpit mode: always emit a sensor report, exit 0 for dashboard ingestion
perfgate check --config perfgate.toml --bench my-bench --mode cockpit
```

## More

- Workspace overview and CI examples: [README.md](../../README.md)
- Testing strategy: [TESTING.md](../../TESTING.md)
- API docs: [docs.rs/perfgate-cli](https://docs.rs/perfgate-cli)

## License

Licensed under either Apache-2.0 or MIT.
