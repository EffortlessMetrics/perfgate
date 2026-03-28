# perfgate

[![crates.io](https://img.shields.io/crates/v/perfgate-cli.svg)](https://crates.io/crates/perfgate-cli)
[![ci](https://github.com/EffortlessMetrics/perfgate/actions/workflows/ci.yml/badge.svg)](https://github.com/EffortlessMetrics/perfgate/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/perfgate-cli.svg)](https://github.com/EffortlessMetrics/perfgate#license)

**Catch performance regressions in CI before they ship.**

> Your CI is green. But is it *fast*? Someone adds a dependency, tweaks an
> allocator, or refactors a hot path -- and the service quietly gets 15% slower.
> Nobody notices until users complain. perfgate runs your benchmarks, compares
> against baselines, applies statistical significance testing, and fails the
> build when things get slower.

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

**1. Configure** -- define benchmarks in `perfgate.toml`:

```toml
[defaults]
repeat = 7
warmup = 1
threshold = 0.20
baseline_dir = "baselines"

[[bench]]
name = "my-service"
command = ["./target/release/my-bench"]
```

**2. Run** -- check locally or in CI:

```bash
perfgate check --config perfgate.toml --bench my-service
```

Optional diagnostics for regressing benches:

```bash
perfgate check --config perfgate.toml --bench my-service --profile-on-regression
```

**3. Gate** -- wire into GitHub Actions:

```yaml
# .github/workflows/perf.yml
- uses: EffortlessMetrics/perfgate@v0.15.1
  with:
    config: perfgate.toml
    all: "true"
```

Pin `@v0.15.1` for an exact patch release, or use `@v0.15` / `@v0` to follow
the current compatible action tag.

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

## Features

**Core Pipeline**
- Three-stage **run -> compare -> verdict** pipeline with versioned JSON receipts
- Config-driven `check` command runs the full pipeline from `perfgate.toml`
- Baselines stored in-repo, cloud storage (`s3://`, `gs://`), or the optional baseline server
- Bundled [presets](presets/) for standard, release, and fast-feedback workflows

**Statistical Analysis**
- Welch's t-test with configurable alpha and confidence intervals
- Paired benchmarking for noisy CI environments with significance-based retries
- Noise detection (CV-based) with configurable escalation policy
- Per-metric statistic selection (median, p95, etc.)
- Scaling validation with best-fit complexity classification via `perfgate scale`

**Diagnostics**
- `bisect` -- find the exact commit that introduced a regression
- `blame` -- map regressions to `Cargo.lock` dependency changes
- `explain` -- generate AI-ready regression diagnostics for PR comments
- Optional flamegraph capture on warn/fail regressions via `--profile-on-regression`
- Trend analysis and predictive budget alerts for drifting benchmarks

## Advanced Workflows

Validate computational complexity for a benchmark command:

```bash
perfgate scale --name parser --command "./target/release/parser-bench --size {n}" --sizes 100,1000,10000 --expected "O(n)"
```

Post or update a PR comment from a compare receipt:

```bash
perfgate comment --compare artifacts/perfgate/compare.json --repo owner/repo --pr 123
```

**CI Integration**
- GitHub Actions, GitLab CI support with native annotations
- Multi-format export: CSV, JSONL, HTML, Prometheus, JUnit
- Cockpit mode for dashboard integration via `sensor.report.v1`
- Fleet aggregation: merge results from distributed runners

**Baseline Server**
- REST API (Axum) with SQLite, PostgreSQL, or S3/GCS/Azure storage
- Role-based access with API keys and GitHub Actions OIDC
- Verdict history tracking and web dashboard (alpha)

## Commands

| Command | Description |
|---------|-------------|
| **`check`** | **Config-driven workflow (start here)** |
| `run` | Execute a benchmark, emit a run receipt |
| `compare` | Compare a run against a baseline |
| `diff` | Run a quick local regression check against discovered config/baselines |
| `paired` | Interleaved A/B benchmarking for noisy environments |
| `promote` | Promote a run to become the new baseline |
| `md` | Render a comparison as Markdown |
| `report` | Generate a cockpit-compatible report |
| `export` | Export to CSV, JSONL, HTML, Prometheus, or JUnit |
| `cargo-bench` | Wrap `cargo bench` and emit perfgate receipts |
| `ingest` | Import external benchmark results into perfgate format |
| `badge` | Generate SVG status, metric, or trend badges |
| `discover` | Scan a repo for benchmarks and print detected targets |
| `init` | Generate `perfgate.toml` and optional CI scaffolding |
| `watch` | Re-run a benchmark on file changes with live deltas |
| `serve` | Start the local dashboard/baseline server |
| `scale` | Validate complexity scaling across input sizes |
| `comment` | Post or update a GitHub PR performance comment |
| `trend` | Analyze metric drift and predict threshold breaches |
| `baseline` | Manage baselines on the server |
| `fleet` | Analyze dependency regressions across projects |
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
- [Bitbucket Pipelines](docs/GETTING_STARTED_BITBUCKET.md)
- [CircleCI](docs/GETTING_STARTED_CIRCLECI.md)
- [Baseline Server](docs/GETTING_STARTED_BASELINE_SERVER.md)
- [Step-by-Step Pipeline](docs/PIPELINE.md) -- manual run/compare/promote workflow

**How-To Guides** -- solve specific problems:
- [Paired Benchmarking](docs/PAIRED_BENCHMARKING.md) -- reduce noise in flaky CI
- [Cockpit Integration](docs/COCKPIT_MODE.md) -- dashboard integration via sensor.report.v1
- [Exporting Data](docs/EXPORT.md) -- CSV, JSONL, HTML, Prometheus, JUnit
- [Host Mismatch Detection](docs/HOST_MISMATCH.md) -- comparing across different hardware
- [Baseline Server Admin](docs/BASELINE_SERVICE_DESIGN.md)
- [Failure Playbook](docs/FAILURE_PLAYBOOK.md) -- diagnosing and fixing regressions

**Reference**:
- [Configuration](docs/CONFIG.md) -- `perfgate.toml` options and per-metric budgets
- [Output Schemas](docs/SCHEMAS.md) -- perfgate.run.v1, compare.v1, report.v1, sensor.report.v1
- [Artifact Layouts](docs/ARTIFACTS.md) -- standard and cockpit mode output structure
- [Architecture](docs/ARCHITECTURE.md) -- 26-crate workspace, clean-architecture layers
- [ADRs](docs/adrs/) -- architectural decision records

**Explanation**:
- [Design Philosophy](docs/DESIGN.md) -- why perfgate works the way it does
- [Self-Dogfooding](docs/SELF_DOGFOODING.md) -- how perfgate gates its own performance

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing, and repo automation.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
