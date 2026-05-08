# Configuration Reference

perfgate uses TOML configuration files for the `check` command.

## Full Example

```toml
[defaults]
repeat = 7                                    # iterations per benchmark
warmup = 1                                    # warmup iterations (discarded)
threshold = 0.20                              # fail if regression exceeds 20%
warn_factor = 0.50                            # warn at 50% of threshold
noise_threshold = 0.10                        # warn when CV exceeds 10%
noise_policy = "warn"                         # warn on noisy measurements
out_dir = "artifacts/perfgate"                # default artifact directory
baseline_dir = "baselines"                    # directory for baseline receipts
baseline_pattern = "baselines/{bench}.json"   # pattern with {bench} placeholder
markdown_template = ".github/perfgate-comment.hbs"  # optional Handlebars template

[[bench]]
name = "pst_extract"
command = ["sh", "-c", "sleep 0.02"]
work = 1000                                   # work units for throughput calc
budgets = { wall_ms = { threshold = 0.20, warn_factor = 0.90, statistic = "p95" }, max_rss_kb = { threshold = 0.15, warn_factor = 0.90, statistic = "median" } }

[[bench]]
name = "api_latency"
command = ["./target/release/api-bench"]
repeat = 10                                   # override defaults per bench
scaling = { sizes = [100, 1000, 10000], expected = "O(n)", repeat = 5, r_squared_threshold = 0.90 }

[[scenario]]
name = "api_latency_release"
weight = 0.60
bench = "api_latency"
description = "Release-gate API latency workload"

[[scenario]]
name = "pst_extract_batch"
weight = 0.40
bench = "pst_extract"

[[tradeoff]]
name = "memory-for-latency"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
min_improvement_ratio = 1.10
```

## Budget Configuration

Each metric can have its own budget:

```toml
[budgets.wall_ms]
threshold = 0.20        # 20% regression = fail
warn_factor = 0.50      # warn at 10% (0.50 * 0.20)
statistic = "p95"       # gate on p95 instead of median
```

Available statistics: `median` (default), `p95`.

## Scaling Configuration

Each benchmark can optionally declare a scaling policy for `perfgate scale`
and JSON/TOML schema validation:

```toml
[[bench]]
name = "parser"
command = ["./target/release/parser-bench", "--size", "{n}"]
scaling = { sizes = [100, 1000, 10000], expected = "O(n)", repeat = 7, r_squared_threshold = 0.95 }
```

| Field | Description |
|-------|-------------|
| `sizes` | Required input sizes used for complexity fitting |
| `expected` | Optional expected complexity class such as `O(n)` or `O(n^2)` |
| `repeat` | Optional repetitions per input size |
| `r_squared_threshold` | Optional minimum fit quality threshold |

## Scenario Configuration

Scenarios define a weighted workload model over configured benchmarks. After
`perfgate check --config perfgate.toml --all` has produced compare receipts,
evaluate the workload into a `perfgate.scenario.v1` receipt:

```bash
perfgate scenario evaluate --config perfgate.toml --out artifacts/perfgate/scenario.json
```

Each `[[scenario]]` references one `[[bench]]`. By default, `scenario evaluate`
reads `compare.json` from `[defaults].out_dir/<bench>/compare.json`. Use
`compare = "path/to/compare.json"` when the compare receipt lives somewhere
else.

```toml
[[bench]]
name = "large-file"
command = ["cargo", "bench", "--bench", "large_file"]

[[bench]]
name = "small-edit"
command = ["cargo", "bench", "--bench", "small_edit"]

[[scenario]]
name = "large_file_parse"
weight = 0.35
bench = "large-file"

[[scenario]]
name = "small_incremental_edit"
weight = 0.50
bench = "small-edit"
compare = "artifacts/perfgate/small-edit/compare.json"
```

## Tradeoff Configuration

Tradeoff rules make accepted performance exchanges explicit. They only apply to
metrics that are already failing, and every required compensating improvement
must be present and satisfied.

```toml
[[tradeoff]]
name = "memory-for-latency"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
min_improvement_ratio = 1.10
```

Evaluate the rules against a scenario receipt to produce
`perfgate.tradeoff.v1` decision evidence:

```bash
perfgate tradeoff evaluate --config perfgate.toml --scenario artifacts/perfgate/scenario.json --out artifacts/perfgate/tradeoff.json
```

Render the decision for local review or PR comments:

```bash
perfgate md --tradeoff artifacts/perfgate/tradeoff.json --out artifacts/perfgate/tradeoff.md
perfgate comment --tradeoff artifacts/perfgate/tradeoff.json --dry-run
```

To run the structured decision path as one local workflow, use:

```bash
perfgate decision evaluate --config perfgate.toml
```

It uses the configured artifact directory for compare lookups and writes
`scenario.json`, `tradeoff.json`, and `decision.md` there by default.

`min_improvement_ratio` follows metric direction. For lower-is-better metrics
such as `wall_ms`, `1.10` means the baseline/current ratio must be at least
1.10. For higher-is-better metrics such as `throughput_per_s`, the
current/baseline ratio must be at least 1.10.

## Presets

Bundled presets in `presets/`:

| Preset | Repeat | Warmup | Threshold | Use case |
|--------|--------|--------|-----------|----------|
| `standard.toml` | 7 | 1 | 20% | Regular PR checks |
| `release.toml` | 10 | 2 | 10% | Release branches, nightly |
| `tier1-fast.toml` | 3 | 1 | 30% | Draft PRs, fast feedback |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `PERFGATE_SERVER_URL` | Baseline server URL |
| `PERFGATE_API_KEY` | API key for server authentication |
| `PERFGATE_PROJECT` | Project name for multi-tenancy |

## CLI Flags

The `check` command accepts flags that override config:

```bash
perfgate check --config perfgate.toml --bench my-bench \
  --baseline gs://my-baselines/bench.json \
  --output-github \
  --mode cockpit \
  --profile-on-regression \
  --md-template .github/perfgate-comment.hbs \
  --bench-regex "^service/"
```
