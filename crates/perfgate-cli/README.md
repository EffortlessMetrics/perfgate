# perfgate

**One tool to run benchmarks, detect regressions, and gate CI.**

You ship code daily. Performance regressions slip through because measuring,
comparing, and enforcing budgets are three separate problems. perfgate solves
all three in a single binary that fits into any CI pipeline.

## Install

The binary installer avoids compiling perfgate locally. If `cargo-binstall` is
not installed yet, install it once with `cargo install cargo-binstall`.

```bash
# Pre-built binary (via cargo-binstall)
cargo binstall perfgate-cli

# From crates.io with Rust 1.95 or newer
cargo install perfgate-cli --locked

# From this repository checkout
cargo install --path crates/perfgate-cli
```

Check the local install and project setup:

```bash
perfgate doctor
```

## Quick Start

Start from the repository you want to protect:

```bash
perfgate doctor
perfgate init --ci github --profile standard --suggest-benches
```

`init` writes the local configuration, a GitHub Actions workflow, a baseline
directory placeholder, and `.perfgate/README.md`:

```text
perfgate.toml
.github/workflows/perfgate.yml
baselines/.gitkeep
.perfgate/README.md
```

Edit `perfgate.toml`, uncomment or add one `[[bench]]`, then prove the local
gate before pushing:

```bash
perfgate doctor --config perfgate.toml
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

Commit the durable setup and baseline files, but leave generated artifacts out
unless your team intentionally archives them:

```bash
git add perfgate.toml .github/workflows/perfgate.yml baselines/ .perfgate/README.md
```

For the full first-hour walkthrough, including expected artifacts and failure
handling, see [First Hour With perfgate](../../docs/FIRST_HOUR.md).

## GitHub Actions

The generated workflow uses the published composite action:

```yaml
- uses: EffortlessMetrics/perfgate@v0
  with:
    config: perfgate.toml
    all: "true"
    require_baseline: "true"
    upload_artifact: "true"
```

Use `@v0.18.0` with `version: "0.18.0"` for an exact patch pin and release
binary install, `@v0.18` for the current 0.18 line, or `@v0` to follow the
current compatible action tag. Omit `version` with moving tags so the action
builds from the checked-out action source.

For the full workflow guide, see
[Getting Started: GitHub Actions](../../docs/GETTING_STARTED_GITHUB_ACTIONS.md).

## Low-Level Receipts

The paved road is `check`, but the lower-level receipt commands are available
for custom pipelines:

```bash
# 1. Measure
perfgate run --name my-bench --out run.json -- ./my-benchmark

# 2. Compare against a baseline
perfgate compare --baseline baseline.json --current run.json --out cmp.json

# 3. Generate a report artifact
perfgate report --compare cmp.json --out report.json
```

`report` renders artifacts from an existing comparison; it does not enforce CI
policy. Use `check` for the full workflow and CI exit codes.

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
| `probe compare` | Compare named probe receipts, emit a `perfgate.probe_compare.v1` receipt |
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
| `decision bundle`    | Export indexed decision evidence as a portable JSON bundle |

### Manage

| Command     | Purpose |
|-------------|---------|
| `promote`   | Copy a run receipt into baseline storage |
| `baseline`  | Manage baselines on a centralized server (list, upload, download, delete, history, verdicts, migrate) |
| `decision`  | Manage decision ledger records (upload, history, latest, export, prune, debt) |
| `serve`     | Start or preflight a local SQLite-backed dashboard server |
| `aggregate` | Merge multiple run receipts (e.g. from a fleet) into one |
| `fleet`     | Fleet-wide dependency regression alerts and impact analysis |

### Automate

| Command  | Purpose |
|----------|---------|
| `check`  | Config-driven end-to-end workflow: run, compare, report, gate |
| `doctor` | Diagnose config, benchmark commands, baselines, artifacts, CI, and server reachability |
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
- First-hour walkthrough: [docs/FIRST_HOUR.md](../../docs/FIRST_HOUR.md)
- GitHub Actions setup: [docs/GETTING_STARTED_GITHUB_ACTIONS.md](../../docs/GETTING_STARTED_GITHUB_ACTIONS.md)
- Testing strategy: [TESTING.md](../../TESTING.md)
- API docs: [docs.rs/perfgate-cli](https://docs.rs/perfgate-cli)

## License

Licensed under either Apache-2.0 or MIT.
