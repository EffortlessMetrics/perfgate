# Configuration Reference

perfgate uses TOML configuration files for the `check` command.

## Full Example

```toml
[defaults]
repeat = 7                                    # iterations per benchmark
warmup = 1                                    # warmup iterations (discarded)
threshold = 0.20                              # fail if regression exceeds 20%
warn_factor = 0.90                            # warn at 90% of threshold
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
```

## Budget Configuration

Each metric can have its own budget:

```toml
[budgets.wall_ms]
threshold = 0.20        # 20% regression = fail
warn_factor = 0.90      # warn at 18% (0.90 * 0.20)
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

## Presets

Bundled presets in `presets/`:

| Preset | Repeat | Warmup | Threshold | Use case |
|--------|--------|--------|-----------|----------|
| `standard.toml` | 5 | 1 | 20% | Regular PR checks |
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
