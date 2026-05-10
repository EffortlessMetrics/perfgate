# perfgate

[![crates.io](https://img.shields.io/crates/v/perfgate-cli.svg)](https://crates.io/crates/perfgate-cli)
[![ci](https://github.com/EffortlessMetrics/perfgate/actions/workflows/ci.yml/badge.svg)](https://github.com/EffortlessMetrics/perfgate/actions/workflows/ci.yml)
[![Codecov](https://codecov.io/gh/EffortlessMetrics/perfgate/branch/main/graph/badge.svg)](https://codecov.io/gh/EffortlessMetrics/perfgate)
[![license](https://img.shields.io/crates/l/perfgate-cli.svg)](https://github.com/EffortlessMetrics/perfgate#license)

**Catch performance regressions in CI before they ship.**

perfgate runs benchmarks, compares the current run against explicit baselines and
budgets, writes versioned receipts/reports, and exits nonzero only when the
configured policy says the build should stop.

## Install

Use the binary installer path first:

```bash
cargo binstall perfgate-cli
```

Or install from source:

```bash
cargo install perfgate-cli
```

Prebuilt archives are published on
[GitHub Releases](https://github.com/EffortlessMetrics/perfgate/releases) for
Linux, macOS, and Windows. Verify the binary with:

```bash
perfgate --version
perfgate doctor --help
```

## Start Here

From a repository with benchmark commands:

```bash
perfgate init --ci github --profile standard
perfgate doctor --config perfgate.toml
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
git add perfgate.toml .github/workflows/perfgate.yml baselines/ .perfgate/
```

`perfgate init --ci github --profile standard` creates:

```text
perfgate.toml
.github/workflows/perfgate.yml
baselines/.gitkeep
.perfgate/README.md
```

The generated config defaults to local checked-in baselines and predictable
artifacts:

```toml
[defaults]
repeat = 7
warmup = 1
threshold = 0.20
warn_factor = 0.50
noise_threshold = 0.10
noise_policy = "warn"
out_dir = "artifacts/perfgate"
baseline_dir = "baselines"

[[bench]]
name = "my-service"
command = ["./target/release/my-bench"]
```

The generated GitHub workflow uses the repository action:

```yaml
- uses: EffortlessMetrics/perfgate@v0
  with:
    config: perfgate.toml
    all: "true"
    require_baseline: "true"
    upload_artifact: "true"
```

Use `@v0.15.1` for an exact patch pin, or `@v0.15` / `@v0` to follow the
current compatible action tag.

## Performance Decisions

The normal gate is still `perfgate check`. When a repo has workload scenarios,
probe evidence, or accepted tradeoff policy, use the one-command decision
workflow after `check`:

```bash
perfgate decision evaluate --config perfgate.toml
```

It writes `scenario.json`, `tradeoff.json`, `decision.md`, and
`decision.index.json` under the configured artifact directory. `decision.md` is
the review surface: it explains the weighted workload result, probe movement,
accepted or rejected tradeoff rules, policy reasons, evidence files, and the
local reproduction command. `decision.index.json` is the machine-readable
manifest for the evidence files behind that review.
When scenarios configure probe baseline/current paths, `decision evaluate` also
writes the configured `probe-compare.json` before evaluating the decision.
To attach or archive the whole decision evidence set, export the indexed
bundle:

```bash
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

Teams running the baseline server can persist that decision evidence as a
ledger, export it for audits, and prune old records explicitly:

```bash
perfgate decision upload --file artifacts/perfgate/tradeoff.json --index artifacts/perfgate/decision.index.json
perfgate decision export --days 90 --out artifacts/perfgate/decision-history.jsonl
perfgate decision prune --older-than 365d --dry-run
```

In GitHub Actions, opt in with:

```yaml
- uses: EffortlessMetrics/perfgate@v0
  with:
    config: perfgate.toml
    all: "true"
    require_baseline: "true"
    decision: "true"
    review_required: "warn"
```

Use `review_required: "fail"` when review-required decisions should block
branch protection, or `review_required: "pass"` when another workflow step owns
that review policy.

### Adoption levels

Perfgate is designed to be adopted in stages:

- **Basic gate** — `perfgate check` and tracked baselines.
- **Decision mode** — add `[[scenario]]` and `[[tradeoff]]` config plus
  `perfgate decision evaluate`.
- **Ledger mode** — connect a baseline server, upload decisions, and use
  `decision export|history|debt|prune` for auditability.

Use that order to start quickly, then opt into richer tradeoff review and
auditable governance as teams become ready.

## Daily Use

Run the whole configured suite:

```bash
perfgate check --config perfgate.toml --all
```

Inspect local baseline state:

```bash
perfgate baseline status --config perfgate.toml
```

Promote a trusted current run into local baselines:

```bash
perfgate baseline promote --config perfgate.toml --all
```

Diagnose setup or path issues:

```bash
perfgate doctor --config perfgate.toml
```

Exit codes are stable: `0` pass, `1` tool/runtime error, `2` budget fail, and
`3` warn treated as failure with `--fail-on-warn`.

## Artifacts

`check --bench <name>` writes:

```text
artifacts/perfgate/
  run.json
  compare.json  # when a baseline exists
  report.json
  comment.md
```

`check --all` writes per-benchmark subdirectories, even when the config only has
one benchmark:

```text
artifacts/perfgate/<bench>/
  run.json
  compare.json  # when a baseline exists
  report.json
  comment.md
```

`run.json`, `compare.json`, and `report.json` are versioned machine-readable
receipts. `compare.json` is omitted while bootstrapping without a baseline. See
[Artifact Layouts](docs/ARTIFACTS.md) and [Output Schemas](docs/SCHEMAS.md) for
the contract details.

## What Gets Measured

| Metric | Description |
| ------ | ----------- |
| `wall_ms` | Wall-clock time |
| `cpu_ms` | User + system CPU time |
| `max_rss_kb` | Peak resident set size |
| `page_faults` | Major page faults where available |
| `ctx_switches` | Context switches where available |
| `binary_bytes` | Executable size |
| `throughput_per_s` | Ops/sec with `--work` |

perfgate supports local baselines, cloud paths, and the optional baseline
server, but local in-repo baselines are the default first setup.

## Documentation

Start with:

- [GitHub Actions](docs/GETTING_STARTED_GITHUB_ACTIONS.md)
- [Configuration](docs/CONFIG.md)
- [Artifact Layouts](docs/ARTIFACTS.md)
- [Debugging the First CI Run](docs/DEBUGGING_FIRST_CI_RUN.md)
- [Failure Playbook](docs/FAILURE_PLAYBOOK.md)

For specific workflows:

- [Performance Decisions](docs/PERFORMANCE_DECISIONS.md)
- [Step-by-Step Pipeline](docs/PIPELINE.md)
- [Baseline Server](docs/GETTING_STARTED_BASELINE_SERVER.md)
- [Paired Benchmarking](docs/PAIRED_BENCHMARKING.md)
- [Flakiness History](docs/FLAKINESS.md)
- [Fleet Aggregation](docs/FLEET_AGGREGATION.md)
- [Performance Decision Example](examples/performance-decision/README.md)
- [Cockpit Integration](docs/COCKPIT_MODE.md)
- [Exporting Data](docs/EXPORT.md)
- [Host Mismatch Detection](docs/HOST_MISMATCH.md)

For project contracts and internals:

- [Output Schemas](docs/SCHEMAS.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Public Crate Seams](docs/CRATE_SEAMS.md)
- [Release Readiness](docs/RELEASE_READINESS.md)

## Public Crates

The intended public package surface is:

```text
perfgate
perfgate-cli
perfgate-types
perfgate-client
perfgate-server
```

Internal seams live behind modules and private compatibility wrappers. The
workspace enforces this with `cargo run -p xtask -- public-surface --strict` and
`cargo run -p xtask -- arch`.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing, and repo
automation.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE).
