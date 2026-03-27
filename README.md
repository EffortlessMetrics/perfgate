# perfgate

Your CI is green. But is it *fast*?

Someone adds a dependency, tweaks an allocator, or refactors a hot path — and
the service quietly gets 15% slower. Nobody notices until users complain.

**perfgate** catches performance regressions in CI before they ship. It runs your
benchmarks, compares against baselines, applies statistical significance testing
to filter noise from real regressions, and fails the build when things get
slower.

```
perfgate: warn

Bench: pst_extract

| metric    | baseline | current  | delta   | budget | status |
|-----------|----------|----------|---------|--------|--------|
| wall_ms   | 793 ms   | 892 ms   | +12.48% | 15.0%  | pass   |
| cpu_ms    | 31 ms    | 35 ms    | +12.90% | 20.0%  | pass   |
| max_rss_kb| 8220 KB  | 8220 KB  | 0.00%   | 20.0%  | pass   |

Notes:
- wall_ms: +12.48% (warn >= 10.00%, fail > 15.00%)
```

## Quick Start

**1. Define your benchmarks:**

```toml
# perfgate.toml
[defaults]
repeat = 7
warmup = 1
threshold = 0.20
baseline_dir = "baselines"

[[bench]]
name = "my-service"
command = ["./target/release/my-bench"]
```

**2. Run:**

```bash
perfgate check --config perfgate.toml --bench my-service
```

**3. Wire into CI:**

```yaml
# .github/workflows/perf.yml
- uses: EffortlessMetrics/perfgate@main
  with:
    config: perfgate.toml
    all: "true"
```

Exit code `2` = budget violated. That's it.

## Install

**Pre-built binaries** (fastest):

```bash
# Download from GitHub Releases (Linux x86_64 example)
curl -fsSL https://github.com/EffortlessMetrics/perfgate/releases/latest/download/perfgate-x86_64-unknown-linux-gnu.tar.gz \
  | tar xz -C /usr/local/bin
```

Available targets: `x86_64-unknown-linux-gnu`, `x86_64-unknown-linux-musl`,
`aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc`.

**Via cargo-binstall** (auto-detects platform):

```bash
cargo binstall perfgate-cli
```

**From source**:

```bash
cargo install perfgate-cli
```

## How It Works

perfgate has a three-stage pipeline: **run** → **compare** → **verdict**.

Each stage produces a versioned JSON receipt (`perfgate.run.v1`,
`perfgate.compare.v1`) so you can inspect, export, or pipe results into other
tools. The `check` command runs the full pipeline from a config file. For finer
control, each stage is available as a standalone command.

**Baselines** are just JSON files. Store them in-repo (`baselines/` directory),
in cloud storage (`s3://`, `gs://`), or on the optional
[baseline server](docs/GETTING_STARTED_BASELINE_SERVER.md) with PostgreSQL
storage and role-based access.

**On PRs**: run benchmarks → compare against baseline → post Markdown comment → fail if budget violated.

**After merge**: run benchmarks → `perfgate promote` → commit new baseline.

## What Gets Measured

| Metric | Description | Unix | Windows |
|--------|-------------|:----:|:-------:|
| `wall_ms` | Wall-clock time (median) | yes | yes |
| `cpu_ms` | User + system CPU time | yes | yes |
| `max_rss_kb` | Peak resident set size | yes | yes |
| `page_faults` | Major page faults | yes | -- |
| `ctx_switches` | Context switches | yes | -- |
| `binary_bytes` | Executable size | yes | yes |
| `throughput_per_s` | Ops/sec (with `--work`) | yes | yes |

Comparisons use [Welch's t-test](https://en.wikipedia.org/wiki/Welch%27s_t-test)
with configurable alpha. Add `--require-significance` to suppress verdicts when
sample sizes are too small to be conclusive.

## Configuration

```toml
[defaults]
repeat = 7
warmup = 1
threshold = 0.20          # fail if >20% regression
warn_factor = 0.90         # warn at 90% of threshold
baseline_dir = "baselines"
baseline_pattern = "baselines/{bench}.json"

[[bench]]
name = "my-service"
command = ["./target/release/my-bench"]
work = 1000
budgets = { wall_ms = { threshold = 0.20, statistic = "p95" } }
```

Bundled presets in `presets/`:

| Preset | Threshold | Use case |
|--------|-----------|----------|
| `standard.toml` | 20% | Regular PR checks |
| `release.toml` | 10% | Release branches |
| `tier1-fast.toml` | 30% | Draft PRs, fast feedback |

## Commands

| Command | Description |
|---------|-------------|
| **`check`** | **Config-driven workflow (start here)** |
| `run` | Execute a benchmark, emit a run receipt |
| `compare` | Compare a run against a baseline |
| `paired` | Interleaved A/B benchmarking for noisy environments |
| `promote` | Promote a run to become the new baseline |
| `md` | Render a comparison as Markdown |
| `report` | Generate a cockpit-compatible report |
| `export` | Export to CSV, JSONL, HTML, Prometheus, or JUnit |
| `baseline` | Manage baselines on the server |
| `summary` | Summarize multiple comparisons in a table |
| `aggregate` | Merge run receipts from multiple runners |
| `bisect` | Find the commit that introduced a regression |
| `blame` | Map regressions to Cargo.lock dependency changes |
| `explain` | Generate AI-ready regression diagnostics |

Exit codes: `0` pass, `1` error, `2` fail, `3` warn (with `--fail-on-warn`).

## Documentation

**Tutorials** -- get started step by step:
- [GitHub Actions](docs/GETTING_STARTED_GITHUB_ACTIONS.md)
- [GitLab CI](docs/GETTING_STARTED_GITLAB_CI.md)
- [Baseline Server](docs/GETTING_STARTED_BASELINE_SERVER.md)
- [Step-by-Step Pipeline](docs/PIPELINE.md) -- manual run/compare/promote workflow

**How-To Guides** -- solve specific problems:
- [Paired Benchmarking](docs/PAIRED_BENCHMARKING.md) -- reduce noise in flaky CI
- [Cockpit Integration](docs/COCKPIT_MODE.md) -- dashboard integration via sensor.report.v1
- [Exporting Data](docs/EXPORT.md) -- CSV, JSONL, HTML, Prometheus, JUnit
- [Host Mismatch Detection](docs/HOST_MISMATCH.md) -- comparing across different hardware
- [Baseline Server Admin](docs/BASELINE_SERVICE_DESIGN.md)

**Reference**:
- [Output Schemas](docs/SCHEMAS.md) -- perfgate.run.v1, compare.v1, report.v1, sensor.report.v1
- [Artifact Layouts](docs/ARTIFACTS.md) -- standard and cockpit mode output structure
- [Configuration Reference](docs/CONFIG.md)
- [Architecture](docs/ARCHITECTURE.md) -- 26-crate workspace, clean-architecture layers
- [ADRs](docs/adrs/) -- architectural decision records

**Explanation**:
- [Design Philosophy](docs/DESIGN.md) -- why perfgate works the way it does
- [Self-Dogfooding](docs/SELF_DOGFOODING.md) -- how perfgate gates its own performance
- [Failure Playbook](docs/FAILURE_PLAYBOOK.md) -- diagnosing and fixing regressions

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing, and repo automation.

## License

Dual-licensed under MIT or Apache-2.0.
