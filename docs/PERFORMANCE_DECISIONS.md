# Performance Decisions

perfgate's normal gate answers whether configured benchmarks stayed inside
their budgets:

```bash
perfgate check --config perfgate.toml --all
```

The decision workflow answers a richer review question:

```text
What moved, where did it move, and is the tradeoff acceptable under policy?
```

Use it after `check` has produced compare receipts:

```bash
perfgate decision evaluate --config perfgate.toml
```

By default this writes:

```text
artifacts/perfgate/
  scenario.json
  tradeoff.json
  decision.md
```

`decision.md` is the human review surface. It summarizes the weighted workload,
probe evidence, accepted or rejected tradeoff rules, policy reasons, evidence
files, and the local reproduction command.

## Workflow Levels

### Normal Gate

Use `check` for the first CI gate:

```bash
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
```

This is still the default path. It is conservative, explicit, and works with
local checked-in baselines.

### Decision Workflow

Use `decision evaluate` when the repo has scenario weights or tradeoff rules:

```bash
perfgate check --config perfgate.toml --all
perfgate decision evaluate --config perfgate.toml
```

This command runs the structured evidence chain in one step:

```text
scenario evaluate
tradeoff evaluate
markdown rendering
```

It does not run benchmarks. It consumes the receipts already produced by
`check`.

### Probe Evidence

Named probes explain internal phase movement. Ingest probe observations from any
language or harness:

```bash
perfgate ingest probes --file probes.jsonl --out artifacts/perfgate/probes.json
```

Compare probe receipts when you want deltas such as `parser.tokenize +2.1%`:

```bash
perfgate probe compare --baseline baselines/probes.json --current artifacts/perfgate/probes.json --out artifacts/perfgate/probe-compare.json
```

Attach that receipt to a scenario with `probe_compare`:

```toml
[[scenario]]
name = "large_file_parse"
bench = "large-file"
weight = 0.75
probe_compare = "artifacts/perfgate/probe-compare.json"
```

Or let `decision evaluate` create it when baseline and current probe receipts
are configured:

```toml
[[scenario]]
name = "large_file_parse"
bench = "large-file"
weight = 0.75
probe_baseline = "baselines/large-file-probes.json"
probe_current = "artifacts/perfgate/large-file-probes.json"
probe_compare = "artifacts/perfgate/large-file-probe-compare.json"
```

Probe evidence is advisory until a tradeoff rule explicitly requires it.

## Config Shape

Scenarios model workload importance:

```toml
[[scenario]]
name = "large_file_parse"
bench = "large-file"
weight = 0.75

[[scenario]]
name = "small_edit"
bench = "small-edit"
weight = 0.25
```

Tradeoff rules make accepted exchanges explicit:

```toml
[[tradeoff]]
name = "tokenizer-cost-for-batch-loop-win"
if_failed = "max_rss_kb"
downgrade_to = "warn"

[[tradeoff.require]]
metric = "wall_ms"
probe = "parser.batch_loop"
min_improvement_ratio = 1.10

[[tradeoff.allow]]
metric = "wall_ms"
probe = "parser.tokenize"
max_regression = 0.03
```

When `probe` is present, the requirement is satisfied only by that named probe's
delta from scenario-attached probe comparison evidence. `[[tradeoff.allow]]`
keeps the local regression bounded; if `parser.tokenize` regresses by more than
3%, the tradeoff is rejected even if `parser.batch_loop` improves enough.

## GitHub Actions

Enable decision mode in the repository action:

```yaml
- uses: EffortlessMetrics/perfgate@v0
  with:
    config: perfgate.toml
    all: "true"
    require_baseline: "true"
    decision: "true"
```

Decision mode runs `perfgate decision evaluate --config perfgate.toml` after
`check`, uploads the decision artifacts, and appends `decision.md` to the job
summary. If `check` exits `2` for a policy failure, the action can defer the
final result to the decision receipt so an accepted tradeoff owns the final
policy outcome.

## Needs Review

Some evidence gaps should not silently pass or hard-fail the workflow. When a
tradeoff's compensating evidence is otherwise satisfied but a configured named
probe or local regression cap is missing, perfgate marks the decision as review
required:

```text
Decision: warn, review required
Reason: required tradeoff evidence is incomplete
```

The machine verdict remains `warn`, and the `perfgate.tradeoff.v1` receipt sets
`decision.review_required = true` with `review_reasons`. Present evidence that
disproves the tradeoff, such as a local probe exceeding `max_regression`, still
rejects the tradeoff and preserves the failing verdict.

## Primitive Commands

The lower-level commands are still useful for debugging and custom pipelines:

```bash
perfgate scenario evaluate --config perfgate.toml --out artifacts/perfgate/scenario.json
perfgate tradeoff evaluate --config perfgate.toml --scenario artifacts/perfgate/scenario.json --out artifacts/perfgate/tradeoff.json
perfgate md --tradeoff artifacts/perfgate/tradeoff.json --out artifacts/perfgate/decision.md
```

Prefer `perfgate decision evaluate` for the normal local and CI workflow.

## Example

For a deterministic runnable fixture that shows a memory warning accepted by a
probe-backed speed improvement, see
[`examples/performance-decision`](../examples/performance-decision/README.md).
