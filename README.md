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
| `export` | Export data to CSV/JSONL/HTML/Prometheus for trend analysis |
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
  --metric-stat wall_ms=p95 \
  --significance-alpha 0.05 \
  --significance-min-samples 8 \
  --out artifacts/perfgate/compare.json
```

`--metric-stat` lets you choose `median` or `p95` per metric. With `--significance-alpha`, compare/check also emits p-value metadata (Welch's t-test). Add `--require-significance` to require significance before warn/fail escalation.

### 3) Render a PR-ready comment

```bash
perfgate md \
  --compare artifacts/perfgate/compare.json \
  --out artifacts/perfgate/comment.md

# Optional: custom Handlebars template
perfgate md \
  --compare artifacts/perfgate/compare.json \
  --template .github/perfgate-comment.hbs \
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

# Optional markdown with custom template
perfgate report \
  --compare artifacts/perfgate/compare.json \
  --out artifacts/perfgate/report.json \
  --md artifacts/perfgate/comment.md \
  --md-template .github/perfgate-comment.hbs
```

### 6) Promote run to baseline

After merging to main, promote the current run to become the new baseline:

```bash
perfgate promote \
  --current artifacts/perfgate/run.json \
  --to baselines/pst_extract.json

# Optional: promote directly to cloud object storage
perfgate promote \
  --current artifacts/perfgate/run.json \
  --to s3://my-perfgate-baselines/pst_extract.json
```

### 7) Export for trend analysis

Export historical data to CSV, JSONL, HTML, or Prometheus text format:

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

# Export to HTML summary
perfgate export \
  --compare artifacts/perfgate/compare.json \
  --format html \
  --out trends/summary.html

# Export to Prometheus text format
perfgate export \
  --compare artifacts/perfgate/compare.json \
  --format prometheus \
  --out trends/metrics.prom
```

### 8) Config-driven workflow (check)

Run the entire workflow from a single config file:

```bash
perfgate check --config perfgate.toml --bench pst_extract

# Run all benchmarks in the config
perfgate check --config perfgate.toml --all

# Run a subset of benches in --all mode
perfgate check --config perfgate.toml --all --bench-regex "^service/"

# Emit GitHub Actions outputs (verdict/counts) for workflow branching
perfgate check --config perfgate.toml --bench pst_extract --output-github

# Pull baseline from cloud storage
perfgate check --config perfgate.toml --bench pst_extract --baseline gs://my-baselines/pst_extract.json

# Use a custom markdown template
perfgate check --config perfgate.toml --bench pst_extract --md-template .github/perfgate-comment.hbs
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
baseline_pattern = "baselines/{bench}.json"
markdown_template = ".github/perfgate-comment.hbs"

[[bench]]
name = "pst_extract"
command = ["sh", "-c", "sleep 0.02"]
work = 1000
budgets = { wall_ms = { threshold = 0.20, warn_factor = 0.90, statistic = "p95" }, max_rss_kb = { threshold = 0.15, warn_factor = 0.90, statistic = "median" } }
```

#### Config Presets

Bundled presets are available in `presets/` for common scenarios:

| Preset | Repeat | Warmup | Threshold | Use case |
|--------|--------|--------|-----------|----------|
| `standard.toml` | 5 | 1 | 20% | Regular PR checks |
| `release.toml` | 10 | 2 | 10% | Release branches, nightly checks |
| `tier1-fast.toml` | 3 | 1 | 30% | Draft PRs, fast feedback |

### 9) Paired benchmarking mode

Paired benchmarking runs baseline and current commands in interleaved fashion to reduce noise from environmental variations. This is especially useful in noisy CI environments where system load can fluctuate.

```bash
perfgate paired \
  --baseline-cmd "sleep 0.01" \
  --current-cmd "sleep 0.02" \
  --repeat 10 \
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
- **Cloud object storage**: Use `s3://...` or `gs://...` directly with `check --baseline` / `defaults.baseline_pattern` and `promote --to`
- **Baseline Server**: Use the perfgate baseline server for centralized management (see [v2.0 Baseline Server](#v20-baseline-server))
- **Database**: Store in a metrics database for advanced trend analysis

## v2.0 Baseline Server

perfgate v2.0 introduces an optional client/server mode for centralized baseline management. This is useful for teams that want to:

- Share baselines across multiple repositories
- Manage baseline promotion with role-based access control
- Track baseline version history
- Enable CI runners to fetch/upload baselines without git access

### Quick Start

**1. Start the server:**

```bash
# Development (in-memory storage)
perfgate-server

# Production (SQLite storage)
perfgate-server --storage-type sqlite --database-url ./perfgate.db
```

**2. Configure the CLI:**

```bash
# Set the server URL
export PERFGATE_BASELINE_SERVER=http://localhost:8080
export PERFGATE_API_KEY=pg_live_your_api_key
```

**3. Use server-backed baselines:**

```bash
# Upload a baseline to the server
perfgate promote --current run.json --to-server --project my-project --benchmark my-bench

# Compare against server baseline
perfgate check --config perfgate.toml --bench my-bench --baseline-server http://localhost:8080
```

### CLI Integration

The `check` and `promote` commands support server integration:

```bash
# Fetch baseline from server
perfgate check --config perfgate.toml --bench my-bench \
  --baseline-server http://localhost:8080 \
  --project my-project

# Upload baseline to server
perfgate promote --current run.json \
  --to-server \
  --project my-project \
  --benchmark my-bench
```

### Baseline Management CLI

The `perfgate baseline` subcommand provides direct baseline management:

```bash
# List baselines
perfgate baseline list --project my-project

# Upload a baseline
perfgate baseline upload --project my-project --benchmark my-bench --file run.json

# Download a baseline
perfgate baseline download --project my-project --benchmark my-bench --out baseline.json

# Delete a baseline version
perfgate baseline delete --project my-project --benchmark my-bench --version v1.0.0
```

### Authentication

The server uses API keys with role-based access control:

| Role | Permissions |
|------|-------------|
| `viewer` | Read-only access |
| `contributor` | Upload and read baselines |
| `promoter` | Upload, read, and promote baselines |
| `admin` | Full access including delete |

For detailed setup and configuration, see [Getting Started with Baseline Server](docs/GETTING_STARTED_BASELINE_SERVER.md).

## GitHub Action

Use the official composite action for zero-config setup:

```yaml
- uses: EffortlessMetrics/perfgate@main
  with:
    config: perfgate.toml
    all: "true"
    out_dir: artifacts/perfgate
```

## CI Guides

- [GitHub Actions Getting Started](docs/GETTING_STARTED_GITHUB_ACTIONS.md)
- [GitLab CI Getting Started](docs/GETTING_STARTED_GITLAB_CI.md)

## Output Schemas

Receipts are versioned:
- `perfgate.run.v1` - run measurement receipt
- `perfgate.compare.v1` - comparison result
- `perfgate.report.v1` - cockpit-compatible report
- `sensor.report.v1` - sensor integration envelope (cockpit mode)

Generate JSON Schemas:

```bash
cargo run -p xtask -- schema
cargo run -p xtask -- schema-check
```

Schemas are written to `schemas/`. `schema-check` verifies that committed schema files are byte-for-byte locked to generated output.

Validate fixtures against the vendored schema:

```bash
cargo run -p xtask -- conform
cargo run -p xtask -- conform --file path/to/report.json
cargo run -p xtask -- conform --fixtures path/to/dir
```

`--fixtures` validates all `*.json` files in the provided directory, which supports third-party sensor artifact validation.

## Self-Dogfooding

`perfgate` uses itself to monitor and gate its own performance across three distinct CI lanes:

- **Action Smoke Lane**: Validates the GitHub Action integration and installation path.
- **Core Perf Lane**: Strictly gates the performance of core CLI commands against fixed workloads on `ubuntu-24.04`.
- **Nightly Calibration**: Performs high-precision trend analysis and automated baseline refreshes via bot PRs.

See [SELF_DOGFOODING.md](docs/SELF_DOGFOODING.md) and [BASELINE_POLICY.md](docs/BASELINE_POLICY.md) for detailed governance.

## Automation (xtask)

The `xtask` crate provides comprehensive repo automation:

| Command | Description |
|---------|-------------|
| `schema` / `schema-check` | Manage and verify JSON schemas |
| `ci` | Run standard repo checks (fmt, clippy, test, conform) |
| `conform` | Validate fixtures against schemas |
| `sync-fixtures` | Synchronize golden fixtures to contracts |
| `dogfood fixtures` | Regenerate stable dogfooding fixtures |
| `dogfood verify` | Validate artifact layout in CI |
| `docs-sync` / `docs-check` | Manage and verify system documentation |
| `mutants` | Run mutation testing via cargo-mutants |
| `microcrates` | Inventory all workspace crates and kill rate targets |

## Design

`perfgate` follows a highly modularized architecture composed of 21 specialized micro-crates. For the rationale behind this design, see the [Architectural Decision Records (ADRs)](docs/adrs/).

Workspace crates:
- [`crates/perfgate-adapters`](crates/perfgate-adapters/README.md): process execution and host probing adapters
- [`crates/perfgate-app`](crates/perfgate-app/README.md): use-case orchestration and high-level application flows
- [`crates/perfgate-budget`](crates/perfgate-budget/README.md): budget policy and threshold calculation logic
- [`crates/perfgate-cli`](crates/perfgate-cli/README.md): user-facing `perfgate` CLI, JSON I/O, and exit policy
- [`crates/perfgate-client`](crates/perfgate-client/README.md): API client library for centralized baseline management
- [`crates/perfgate-domain`](crates/perfgate-domain/README.md): core domain entities and measurement models
- [`crates/perfgate-error`](crates/perfgate-error/README.md): shared error types and stage/kind classifications
- [`crates/perfgate-export`](crates/perfgate-export/README.md): multi-format data export (CSV, JSONL, HTML, Prometheus)
- [`crates/perfgate-fake`](crates/perfgate-fake/README.md): test doubles and fake implementations for internal testing
- [`crates/perfgate-host-detect`](crates/perfgate-host-detect/README.md): host fingerprinting and mismatch detection
- [`crates/perfgate-paired`](crates/perfgate-paired/README.md): orchestration for paired (interleaved) benchmarking
- [`crates/perfgate-render`](crates/perfgate-render/README.md): Markdown, terminal, and Handlebars rendering logic
- [`crates/perfgate-selfbench`](crates/perfgate-selfbench/README.md): internal benchmarking workloads for self-dogfooding
- [`crates/perfgate-sensor`](crates/perfgate-sensor/README.md): cockpit-compatible sensor report generation
- [`crates/perfgate-server`](crates/perfgate-server/README.md): REST API server for centralized baseline management
- [`crates/perfgate-sha256`](crates/perfgate-sha256/README.md): SIMD-accelerated SHA-256 for finding fingerprints
- [`crates/perfgate-significance`](crates/perfgate-significance/README.md): statistical significance testing (Welch's t-test, p-values)
- [`crates/perfgate-stats`](crates/perfgate-stats/README.md): pure statistical summaries and aggregations
- [`crates/perfgate-types`](crates/perfgate-types/README.md): versioned receipt/config contracts and JSON schemas
- [`crates/perfgate-validation`](crates/perfgate-validation/README.md): fixture and schema conformance validation logic
- [`crates/perfgate`](crates/perfgate/README.md): unified facade library
- [`xtask`](xtask/README.md): workspace automation (schema, CI bundle, fixture conformance, mutants)

### Measurement model

- Uses wall-clock time (median) for gating.
- Supports optional `work_units` to compute `throughput_per_s`.
- Collects `binary_bytes` (best-effort executable size) on supported platforms.
- On Unix, collects `cpu_ms`, `max_rss_kb`, `page_faults`, and `ctx_switches` via `wait4()/rusage`.
- On Windows, collects best-effort `cpu_ms` and `max_rss_kb` via process APIs.

### System Metrics

On Unix platforms, perfgate collects process-level metrics via `rusage`:
- `cpu_ms`: Combined user and system CPU time
- `max_rss_kb`: Peak resident set size
- `page_faults`: Major page faults
- `ctx_switches`: Voluntary + involuntary context switches

On Windows, perfgate collects best-effort:
- `cpu_ms`: Combined user and system CPU time
- `max_rss_kb`: Peak resident set size

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
- `ignore`: Silently allow comparisons across different hosts
- `warn`: Emit a warning but continue with comparison
- `error`: Treat host mismatch as an error and exit with code 1

This is useful when baselines are generated on dedicated benchmark machines but CI runs on different hardware.

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the project vision and planned features.

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
