# perfgate Adoption Levels

perfgate is easiest to adopt when each team starts with the smallest useful
loop, then adds richer evidence only when the review question needs it.

Use the highest level that answers the current question:

| Level | User question | Main surface |
|-------|---------------|--------------|
| 1. Local benchmark gate | Did this local change regress a benchmark? | `perfgate check` |
| 2. GitHub Action gate | Can CI reproduce and explain the same gate? | repository action |
| 3. Structured decision | Did a local regression buy a larger workload improvement? | decision receipts |
| 4. Server ledger | What performance debt are we accepting over time? | baseline server |

You do not need to configure every level on day one. The normal path is level 1
first, level 2 when the repo wants branch protection, level 3 when simple pass
or fail is not enough, and level 4 when a team needs retained history.

## Level 1: Local Benchmark Gate

Use this when a repository has one or more repeatable benchmark commands and
needs a local performance budget before CI or server setup.

### Commands

```bash
perfgate init --ci github --profile standard
perfgate doctor --config perfgate.toml
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
```

### Config

Keep the first config boring: one benchmark, local baselines, and a budget wide
enough to avoid making noise look like policy.

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
name = "parser"
command = ["cargo", "bench", "--bench", "parser"]
```

### Artifacts

`check --all` writes benchmark receipts under the configured artifact directory:

```text
artifacts/perfgate/<bench>/
  run.json
  compare.json
  report.json
  comment.md
```

`compare.json` exists after a baseline exists. The durable files to review and
commit are usually `perfgate.toml`, the generated workflow, and `baselines/`.

### Failure Example

A local budget failure exits `2` and means the current receipt exceeded the
configured policy relative to the committed baseline. Reproduce it with:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

If the change is intended, rerun until the measurement is representative, then
promote the new baseline in a separate reviewable commit.

### Next Level

Move to level 2 when the same gate should run in pull requests.

## Level 2: GitHub Action Gate

Use this when branch protection should run the same checked-in baseline policy
that developers can reproduce locally.

### Commands

Run the CI-equivalent command locally before pushing:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

### Config

The generated workflow calls the repository action:

```yaml
- uses: EffortlessMetrics/perfgate@v0
  with:
    config: perfgate.toml
    all: "true"
    require_baseline: "true"
    upload_artifact: "true"
```

Use `@v0.17.0` for an exact patch pin, or `@v0.17` / `@v0` to follow the
current compatible action tag.

### Artifacts

The action uploads the configured perfgate artifact directory and surfaces the
local reproduction command in the job output. The checked-in baseline remains
the source of truth.

### Failure Example

If CI fails because a benchmark exceeded policy, copy the local reproduction
command from the action output and run it from the same branch:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

Do not promote a baseline from a noisy CI-only result unless the team has
explicitly decided that runner is the baseline authority.

### Next Level

Move to level 3 when a simple budget failure is too blunt, such as when one
workload regresses but a more important workload improves.

## Level 3: Structured Decision

Use this when reviewers need to know what moved, where it moved, and whether a
configured tradeoff policy accepts it.

### Commands

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate decision evaluate --config perfgate.toml
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

### Config

Scenarios encode workload importance. Tradeoff rules encode what exchange is
acceptable.

```toml
[[scenario]]
name = "large_file_parse"
bench = "large-file"
weight = 0.75

[[scenario]]
name = "small_edit"
bench = "small-edit"
weight = 0.25

[[tradeoff]]
name = "memory-for-batch-speed"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
probe = "parser.batch_loop"
min_improvement_ratio = 1.10
```

### Artifacts

Decision mode writes a review surface and machine-readable receipts:

```text
artifacts/perfgate/
  scenario.json
  tradeoff.json
  decision.md
  decision.index.json
  decision-bundle.json
```

Probe comparison receipts are added when scenarios configure probe baseline and
current paths.

### Failure Example

A structured decision can still reject the change when compensating evidence is
missing, too noisy, or outside a configured local cap. A review-required result
is intentionally different from a hard pass: it tells reviewers which evidence
was not trustworthy enough for automatic acceptance.

### Next Level

Add probes when reviewers need internal phase movement. Move to level 4 when
the team needs retained decisions, audit export, or debt summaries.

## Level 4: Server Ledger

Use this when decision history should outlive a single pull request or release
artifact.

### Commands

Preflight local server mode:

```bash
perfgate serve --doctor
```

Run the server and upload decision receipts:

```bash
perfgate serve
perfgate decision upload --file artifacts/perfgate/tradeoff.json --index artifacts/perfgate/decision.index.json
perfgate decision history --project default
perfgate decision debt --project default
perfgate decision export --project default --days 90 --out artifacts/perfgate/decision-history.jsonl
```

Preview retention before deleting anything:

```bash
perfgate decision prune --project default --older-than 365d --dry-run
```

### Config

Keep the server optional. The correctness contract remains local receipts; the
server is for shared history, audit, dashboard review, and debt summaries.

### Artifacts

The server stores `perfgate.decision_record.v1` records and emits audit events
for decision uploads and destructive retention changes. Export JSONL when
release notes, audits, or incidents need a portable ledger snapshot.

### Failure Example

If CI upload fails, the benchmark gate and local decision receipts should still
explain the performance result. Treat server upload failure as an operations
problem unless branch policy explicitly requires ledger persistence before
merge.

### Next Level

Once a team relies on the ledger, add an operations runbook for storage,
backups, API keys, prune policy, and dashboard expectations.

