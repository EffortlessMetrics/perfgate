# perfgate

A small Rust CLI for **performance budgets** and **baseline diffs**.

`perfgate` is designed for modern dev automation:
- emits stable **JSON receipts** for artifacts
- renders a compact **Markdown table** for PR comments
- can output **GitHub Actions annotations**
- uses boring policy defaults (median-based, thresholded)

## Install

From source:

```bash
cargo install --path crates/perfgate-cli
```

Or run in-repo:

```bash
cargo run -p perfgate -- --help
```

## Commands

| Command | Description |
|---------|-------------|
| `run` | Execute a command and record a run receipt |
| `compare` | Compare current run against baseline |
| `md` | Render comparison as Markdown for PR comments |
| `github-annotations` | Emit GitHub Actions annotations |
| `report` | Generate cockpit-compatible report from comparison |
| `promote` | Promote a run receipt to become the new baseline |
| `export` | Export data to CSV/JSONL for trend analysis |
| `check` | Config-driven one-command workflow |
| `paired` | Paired benchmarking with interleaved baseline/current runs |

## Exit Codes

All commands use consistent exit codes:

| Code | Meaning |
|------|---------|
| `0` | Success (or warn without `--fail-on-warn`) |
| `1` | Tool/runtime error (I/O, parse, spawn failures) |
| `2` | Policy fail (budget violated) |
| `3` | Warn treated as failure (with `--fail-on-warn`) |

## Canonical Artifact Layout

perfgate writes artifacts in a predictable structure:

```
artifacts/perfgate/
├── run.json        # perfgate.run.v1 - raw measurement receipt
├── compare.json    # perfgate.compare.v1 - comparison result
├── report.json     # perfgate.report.v1 - cockpit ingestion format
└── comment.md      # PR comment markdown
```

Baseline-missing semantics for `check`:
- `report.json` and `comment.md` are always written
- `compare.json` is omitted when no baseline exists
- `report.json` uses verdict reason token `no_baseline`

## Quickstart

### 1) Run a command and write a receipt

```bash
perfgate run \
  --name pst_extract \
  --repeat 7 \
  --warmup 1 \
  --work 1000 \
  --out artifacts/perfgate/run.json \
  -- \
  sh -c 'sleep 0.02'
```

### 2) Compare to a baseline

```bash
perfgate compare \
  --baseline baselines/pst_extract.json \
  --current artifacts/perfgate/run.json \
  --threshold 0.20 \
  --warn-factor 0.90 \
  --out artifacts/perfgate/compare.json
```

### 3) Render a PR-ready comment

```bash
perfgate md \
  --compare artifacts/perfgate/compare.json \
  --out artifacts/perfgate/comment.md
```

### 4) GitHub Actions annotations

```bash
perfgate github-annotations --compare artifacts/perfgate/compare.json
```

### 5) Generate a cockpit report

```bash
perfgate report \
  --compare artifacts/perfgate/compare.json \
  --out artifacts/perfgate/report.json
```

### 6) Promote run to baseline

After merging to main, promote the current run to become the new baseline:

```bash
perfgate promote \
  --current artifacts/perfgate/run.json \
  --to baselines/pst_extract.json
```

### 7) Export for trend analysis

Export historical data to CSV or JSONL for external analysis tools:

```bash
# Export to CSV
perfgate export \
  --run artifacts/perfgate/run.json \
  --format csv \
  --out trends/data.csv

# Export to JSONL (one JSON object per line)
perfgate export \
  --run artifacts/perfgate/run.json \
  --format jsonl \
  --out trends/data.jsonl
```

### 8) Config-driven workflow (check)

Run the entire workflow from a single config file:

```bash
perfgate check --config perfgate.toml --bench pst_extract

# Run all benchmarks in the config
perfgate check --config perfgate.toml --all
```

#### Cockpit Mode

The `check` command supports a `--mode cockpit` flag for integration with monitoring dashboards. In cockpit mode, the output follows the `sensor.report.v1` schema and includes additional artifacts:

```bash
perfgate check --config perfgate.toml --bench pst_extract --mode cockpit
```

Cockpit mode artifact layout (single bench):

```
artifacts/perfgate/
├── report.json                         # sensor.report.v1 envelope
├── comment.md                          # PR comment markdown
└── extras/
    ├── perfgate.run.v1.json            # perfgate.run.v1
    ├── perfgate.compare.v1.json        # perfgate.compare.v1 (if baseline)
    └── perfgate.report.v1.json         # perfgate.report.v1
```

Multi-bench cockpit mode (`--all`):

```
artifacts/perfgate/
├── report.json                         # aggregated sensor.report.v1
├── comment.md
└── extras/
    ├── bench-a/perfgate.run.v1.json
    ├── bench-a/perfgate.compare.v1.json
    ├── bench-a/perfgate.report.v1.json
    ├── bench-b/perfgate.run.v1.json
    └── ...
```

Example `perfgate.toml`:

```toml
[defaults]
repeat = 7
warmup = 1
threshold = 0.20
warn_factor = 0.90
baseline_dir = "baselines"

[[bench]]
name = "pst_extract"
command = ["sh", "-c", "sleep 0.02"]
work = 1000
```

### 9) Paired benchmarking mode

Paired benchmarking runs baseline and current commands in interleaved fashion to reduce noise from environmental variations. This is especially useful in noisy CI environments where system load can fluctuate.

```bash
perfgate paired \
  --baseline "sleep 0.01" \
  --current "sleep 0.02" \
  --samples 10 \
  --threshold 0.20 \
  --out artifacts/perfgate/compare.json
```

How it works:
1. Runs baseline and current commands alternately (B, C, B, C, ...)
2. Each pair is measured back-to-back to minimize environmental variance
3. Results are compared using the same statistical methods as `compare`

When to use paired mode:
- Noisy CI runners with variable system load
- When you need high-confidence measurements
- Comparing two different implementations directly

## Baseline Workflow

### On Pull Requests

1. Run benchmarks against the PR branch
2. Compare against the stored baseline
3. Post results as PR comment
4. Fail the build if budget is violated

### On Main Branch (after merge)

1. Run benchmarks on the merged code
2. Use `perfgate promote` to update the baseline
3. Commit the new baseline to the repository (or store in artifact storage per your org policy)

```bash
# After merge to main
perfgate run --name mybench --out run.json -- ./my-benchmark
perfgate promote --current run.json --to baselines/mybench.json
git add baselines/mybench.json
git commit -m "Update performance baseline"
```

### Baseline Storage Options

- **In-repo**: Commit baselines to `baselines/` directory (simple, versioned)
- **Artifact storage**: Store in S3/GCS/Azure Blob (scales better for many baselines)
- **Database**: Store in a metrics database for advanced trend analysis

## Output Schemas

Receipts are versioned:
- `perfgate.run.v1` - run measurement receipt
- `perfgate.compare.v1` - comparison result
- `perfgate.report.v1` - cockpit-compatible report
- `sensor.report.v1` - sensor integration envelope (cockpit mode)

Generate JSON Schemas:

```bash
cargo run -p xtask -- schema
```

Schemas are written to `schemas/`.

## Design

Workspace structure:
- `perfgate-types`: receipt/config types + JSON schema support
- `perfgate-domain`: pure math/policy (stats + budget comparison)
- `perfgate-adapters`: process runner + best-effort system metrics
- `perfgate-app`: use-cases + Markdown/annotation rendering
- `perfgate` (cli): clap interface + JSON read/write
- `xtask`: schema generation

### Measurement model

- Uses wall-clock time (median) for gating.
- Supports optional `work_units` to compute `throughput_per_s`.
- On Unix, attempts to collect `ru_maxrss` via `wait4()`.

### CPU Time Tracking (Unix only)

On Unix platforms, perfgate collects CPU time metrics via `rusage`:
- `user_time_ms`: Time spent in user mode
- `system_time_ms`: Time spent in kernel mode

These metrics are included in the run receipt when available and can help identify whether performance changes are due to CPU-bound work or I/O wait.

### Host Mismatch Detection

When comparing runs from different hosts, perfgate can detect and warn about potential inconsistencies. Use the `--host-mismatch` flag to control behavior:

```bash
perfgate compare \
  --baseline baselines/bench.json \
  --current run.json \
  --host-mismatch warn \
  --out compare.json
```

Options for `--host-mismatch`:
- `ignore`: Silently allow comparisons across different hosts (default)
- `warn`: Emit a warning but continue with comparison
- `fail`: Treat host mismatch as an error and exit with code 1

This is useful when baselines are generated on dedicated benchmark machines but CI runs on different hardware.

## Testing

See [TESTING.md](TESTING.md) for the comprehensive testing guide covering:
- Unit and integration tests
- BDD tests with Cucumber
- Property-based tests with proptest
- Fuzz testing with cargo-fuzz
- Mutation testing with cargo-mutants

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for release history.

## License

Dual-licensed under MIT or Apache-2.0.
