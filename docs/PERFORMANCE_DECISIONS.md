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
  decision.index.json
```

`decision.md` is the human review surface. It summarizes the weighted workload,
probe evidence, accepted or rejected tradeoff rules, policy reasons, evidence
files, and the local reproduction command.
`decision.index.json` is the machine-readable artifact manifest for actions,
servers, dashboards, and agents that need to find the generated evidence set.

Export a portable JSON bundle when the decision evidence needs to travel with a
release, issue, audit, or agent handoff:

```bash
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

The bundle uses `perfgate.decision_bundle.v1` and embeds the indexed scenario,
tradeoff, decision markdown, probe-compare, and compare artifacts with SHA-256
hashes plus git metadata when available. It is a transport artifact; the
original receipts remain the source of truth.

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

Rust projects can emit the same JSONL with the optional facade helper:

```toml
[dependencies]
perfgate = { version = "0.15", features = ["probe"] }
```

```rust,no_run
use perfgate::probe::{ProbeJsonlWriter, probe_event};
use perfgate::types::ProbeScope;

fn main() -> std::io::Result<()> {
    let mut probes = ProbeJsonlWriter::create("artifacts/probes.jsonl")?;
    probes.record(
        &probe_event("parser.tokenize")
            .scope(ProbeScope::Local)
            .items(10_000)
            .metric("wall_ms", 12.4, "ms")
            .metric("alloc_bytes", 184_320.0, "bytes"),
    )?;
    Ok(())
}
```

The helper writes language-agnostic JSONL; `perfgate ingest probes` remains the
receipt-producing step.

Projects that already use `tracing` can enable the optional span adapter:

```toml
[dependencies]
perfgate = { version = "0.15", features = ["probe-tracing"] }
tracing = "0.1"
tracing-subscriber = "0.3"
```

```rust,no_run
use perfgate::probe::TracingProbeLayer;
use tracing::{span, Level};
use tracing_subscriber::prelude::*;

fn main() -> std::io::Result<()> {
    let layer = TracingProbeLayer::create("artifacts/probes.jsonl")?;
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::subscriber::with_default(subscriber, || {
        let span = span!(
            Level::INFO,
            "parser.tokenize",
            scope = "local",
            items = 10_000_u64,
            alloc_bytes = 184_320.0,
            phase = "tokenize"
        );
        let _guard = span.enter();
    });

    Ok(())
}
```

The tracing layer writes one JSONL probe event when a span closes. Span active
time becomes `wall_ms`; numeric fields become metrics; `scope`, `parent`,
`items`, and `iteration` map to probe metadata.

Criterion benchmarks can use the optional measurement adapter when they want
Criterion samples to also become probe JSONL:

```toml
[dev-dependencies]
criterion = "0.8"
perfgate = { version = "0.15", features = ["probe-criterion"] }
```

```rust,no_run
use criterion::{criterion_group, criterion_main, Criterion};
use perfgate::probe::CriterionProbeMeasurement;
use perfgate::types::ProbeScope;

fn criterion_with_probes() -> Criterion<CriterionProbeMeasurement<std::fs::File>> {
    Criterion::default().with_measurement(
        CriterionProbeMeasurement::append("parser.batch_loop", "artifacts/probes.jsonl")
            .expect("open probe JSONL")
            .scope(ProbeScope::Dominant)
            .items(10_000)
            .attribute("harness", "criterion"),
    )
}

fn bench_parser(c: &mut Criterion<CriterionProbeMeasurement<std::fs::File>>) {
    c.bench_function("parser/batch_loop", |b| b.iter(|| parser_batch_loop()));
}

fn parser_batch_loop() {}

criterion_group! {
    name = benches;
    config = criterion_with_probes();
    targets = bench_parser
}
criterion_main!(benches);
```

The Criterion adapter preserves Criterion's normal wall-clock measurement while
writing one probe JSONL event for each closed measurement sample. The emitted
events use the configured probe name and record `wall_ms`, sample `iteration`,
and any configured probe metadata.

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

[decision_policy]
require_low_noise_for_acceptance = true
max_cv = 0.10
missing_noise = "needs_review"
```

When `probe` is present, the requirement is satisfied only by that named probe's
delta from scenario-attached probe comparison evidence. `[[tradeoff.allow]]`
keeps the local regression bounded; if `parser.tokenize` regresses by more than
3%, the tradeoff is rejected even if `parser.batch_loop` improves enough.
`[decision_policy]` can require the accepted evidence to stay below a CV cap
before perfgate automatically accepts the tradeoff.

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
probe, local regression cap, or required low-noise signal is missing or too
noisy, perfgate marks the decision as review required:

```text
Decision: warn, review required
Reason: required tradeoff evidence is incomplete
```

The machine verdict remains `warn`, and the `perfgate.tradeoff.v1` receipt sets
`decision.review_required = true` with `review_reasons`. Present evidence that
disproves the tradeoff, such as a local probe exceeding `max_regression`, still
rejects the tradeoff and preserves the failing verdict.

Noise-aware review is opt-in through `[decision_policy]`. With
`require_low_noise_for_acceptance = true`, any otherwise accepted tradeoff is
review-required when a required metric or local cap has `cv > max_cv`. Missing
CV evidence follows `missing_noise`; the conservative default is
`"needs_review"`.

## Server Ledger

Local receipts remain the default workflow, but teams that run the baseline
server can store decisions as audit-backed ledger entries:

```bash
perfgate decision upload --file artifacts/perfgate/tradeoff.json --index artifacts/perfgate/decision.index.json
perfgate decision latest
perfgate decision history --limit 20
perfgate decision debt --days 30
```

The server returns `perfgate.decision_record.v1` records. Each record stores the
tradeoff receipt, optional scenario receipt, optional artifact index, accepted
rule names, final status/verdict, review state, git metadata, and creation
time. Uploads emit an audit event with resource type `decision`.

Use `decision history` filters to inspect a specific part of the ledger:

```bash
perfgate decision history --accepted true --rule memory_for_probe_speed
perfgate decision history --review-required true
perfgate decision history --scenario large_file_parse --verdict warn
```

The dashboard exposes the same drilldowns for status, verdict, review state,
accepted-tradeoff presence, scenario, and accepted rule.

`decision debt` summarizes accepted tradeoff records by scenario so teams can
spot repeated exceptions before they become invisible performance debt. When a
tradeoff rule used local regression caps, the summary reports the highest cap
usage observed in the selected window. When the uploaded tradeoff receipt still
contains its configured rule and weighted deltas, the summary also reports the
largest accepted failed-metric regression, such as `max_rss_kb +3.0%`. Budget
headroom usage is reported as `n/a` until receipts include the original budget
threshold denominator; perfgate does not infer that value from status alone.

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
