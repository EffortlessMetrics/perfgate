# Probe Design Patterns

Probes are review interfaces for performance decisions. They should explain a
tradeoff that a benchmark result alone cannot explain:

```text
the outside benchmark changed here, and the internal work moved there
```

Do not use probes as a profiler replacement. A good probe set is small, stable,
and tied to the workload question reviewers already care about.

## Pick The First Three Probes

Start with three probes at most:

| Probe type | Question it answers | Example |
|------------|---------------------|---------|
| boundary phase | Which major phase changed? | `parser.tokenize` |
| dominant loop | Did the hot path improve? | `parser.batch_loop` |
| local cost cap | Did the accepted regression stay bounded? | `parser.ast_build` |

If reviewers cannot explain what action a probe would trigger, do not add it
yet.

## Naming Rules

Use stable workload names, not implementation details.

Good names:

```text
parser.tokenize
parser.ast_build
parser.batch_loop
client.request_encode
client.response_decode
render.layout
export.write_csv
```

Avoid:

```text
parse_file_v2
ParserImpl::parse_inner
loop_17
span_48912
alloc_site_a
```

Names should survive refactors. If a refactor preserves the same performance
responsibility, keep the probe id. If the responsibility changes, rename the
probe and update baselines, scenarios, and tradeoff policy in the same PR.

## Scope Patterns

Use scope to tell reviewers how the probe participates in the decision:

| Scope | Use for | Example |
|-------|---------|---------|
| `dominant` | the workload improvement that justifies a tradeoff | `parser.batch_loop` |
| `local` | a bounded local regression or supporting phase | `parser.tokenize` |
| parent/child names | related phases that should be read together | `parser.total` -> `parser.tokenize` |

Do not make every probe `dominant`. A dominant probe should be the reason a
tradeoff can be accepted, not just another measurement.

## Parser Pipeline Example

Use this shape when a change moves work between phases:

```text
parser.tokenize
parser.ast_build
parser.batch_loop
```

Good review question:

```text
Did tokenization get slightly slower while batch parsing improved enough to
matter for the large-file scenario?
```

Good tradeoff rule:

```text
Require parser.batch_loop to improve by at least 10%, and allow
parser.tokenize to regress by at most 3%.
```

## Client/Network Example

Probe local client work, not the remote service:

```text
client.request_encode
client.response_decode
client.retry_backoff
```

Avoid making the network round trip the deciding probe unless the test owns a
controlled local server. Remote services create availability and routing noise
that probes cannot make trustworthy.

## Render/Export Example

Use probes to separate data preparation from output writing:

```text
render.layout
render.paint
export.serialize
export.write_csv
```

This helps reviewers distinguish "we spent more time preparing a richer report"
from "the output path got slower without a user-facing benefit."

## Batch Loop Example

For batch processing, prefer stable workload units:

```text
batch.read_inputs
batch.transform
batch.write_outputs
```

Include an `items` count when possible so reviewers can tell whether the probe
represents the same amount of work across baseline and current receipts.

## Bad Probe Shapes

| Bad shape | Why it fails review |
|-----------|---------------------|
| one probe per function | creates trace noise instead of decision evidence |
| generated ids | cannot be tracked across reviews |
| private type names | churns when implementation changes |
| mixed responsibilities | hides which phase moved |
| remote service timing | confuses product performance with dependency noise |
| probes with no policy consumer | adds ceremony without changing the decision |

If a probe is useful only while debugging one incident, keep it out of the
durable decision path or remove it after the investigation.

## Refactor Safety

Treat a probe id like a small public contract inside the repo:

- keep the id when the user-visible workload responsibility is unchanged;
- rename only when the responsibility changed;
- update `perfgate.toml`, probe baselines, scenario references, and tradeoff
  rules together;
- mention the rename in the PR so reviewers know old and new receipts are not
  directly comparable;
- avoid using function names that churn during normal cleanup.

## Connecting To Decisions

After a probe set is stable, attach it to scenarios and tradeoff rules:

```toml
[[scenario]]
name = "large_file_parse"
bench = "large-file"
weight = 0.75
probe_baseline = "baselines/probes.json"
probe_current = "artifacts/perfgate/probes-current.json"
probe_compare = "artifacts/perfgate/probe-compare.json"

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

Use [`PROBE_QUICKSTART.md`](PROBE_QUICKSTART.md) for the mechanics of emitting,
ingesting, comparing, and bundling probe evidence. Use
[`SIGNAL_CALIBRATION.md`](SIGNAL_CALIBRATION.md) when the probe evidence is too
noisy to accept automatically.

