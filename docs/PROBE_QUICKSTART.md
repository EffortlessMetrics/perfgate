# Probe Instrumentation Quickstart

Probes explain where work moved inside a benchmark. They are useful when the
normal gate says "this changed" but reviewers need the product answer:

```text
this got slower here, but faster where it matters
```

Probes are not a profiler. Start with two or three named phases that already
matter to reviewers, such as `parser.tokenize`, `parser.ast_build`, and
`parser.batch_loop`.

## 1. Pick Stable Probe Names

Use names that describe durable work, not temporary function names:

```text
parser.tokenize
parser.ast_build
parser.batch_loop
```

Good probe names are:

- stable across refactors;
- specific enough to explain a tradeoff;
- few enough that reviewers can read the decision.

Avoid emitting one probe for every function, span, or allocation site. perfgate
needs review evidence, not a trace dump.

## 2. Emit JSONL From Any Language

`perfgate ingest probes` accepts newline-delimited JSON. Each line is one probe
observation.

```json
{"probe":"parser.tokenize","scope":"local","wall_ms":12.4,"alloc_bytes":184320,"items":10000}
{"probe":"parser.batch_loop","scope":"dominant","wall_ms":44.8,"items":10000}
```

Write that file wherever your benchmark can reach it, for example:

```text
artifacts/probes-current.jsonl
```

Then ingest it into a typed perfgate receipt:

```bash
perfgate ingest probes --file artifacts/probes-current.jsonl --out artifacts/perfgate/probes-current.json
```

The JSONL file is an input convenience. The receipt is the durable artifact
that downstream commands consume.

## 3. Use The Rust Helper When Convenient

Rust projects can use the optional helper to write the same JSONL shape without
hand-building JSON strings.

```toml
[dependencies]
perfgate = { version = "0.17", features = ["probe"] }
```

```rust,no_run
use perfgate::probe::{ProbeJsonlWriter, probe_event, probe_timer};
use perfgate::types::ProbeScope;

fn main() -> std::io::Result<()> {
    let mut probes = ProbeJsonlWriter::create("artifacts/probes-current.jsonl")?;

    probes.record(
        &probe_event("parser.tokenize")
            .scope(ProbeScope::Local)
            .items(10_000)
            .metric("wall_ms", 12.4, "ms")
            .metric("alloc_bytes", 184_320.0, "bytes"),
    )?;

    let timer = probe_timer("parser.batch_loop")
        .scope(ProbeScope::Dominant)
        .items(10_000);

    run_batch_loop();

    probes.record(&timer.finish())?;
    probes.flush()
}

fn run_batch_loop() {}
```

The helper is deliberately small: it writes JSONL, and
`perfgate ingest probes` turns that JSONL into a receipt.

## 4. Keep Baseline And Current Receipts

For decision workflows, compare a baseline probe receipt with a current probe
receipt.

```bash
perfgate ingest probes --file baselines/probes.jsonl --out baselines/probes.json
perfgate ingest probes --file artifacts/probes-current.jsonl --out artifacts/perfgate/probes-current.json
perfgate probe compare --baseline baselines/probes.json --current artifacts/perfgate/probes-current.json --out artifacts/perfgate/probe-compare.json
```

Usually commit:

- reviewed baseline probe JSONL or receipt files when they define the accepted
  comparison point;
- `perfgate.toml` entries that reference the probe evidence.

Usually do not commit:

- current-run probe JSONL from ordinary local experiments;
- generated `artifacts/perfgate/` output unless it is attached to a release,
  audit, issue, or review record on purpose.

## 5. Attach Probes To A Scenario

Add probe paths to the scenario that needs the evidence:

```toml
[[scenario]]
name = "large_file_parse"
bench = "large-file"
weight = 0.75
probe_baseline = "baselines/probes.json"
probe_current = "artifacts/perfgate/probes-current.json"
probe_compare = "artifacts/perfgate/probe-compare.json"
```

Then run the normal decision command:

```bash
perfgate decision evaluate --config perfgate.toml
```

`decision evaluate` writes the configured `probe-compare.json` before it
evaluates scenarios and tradeoffs.

## 6. Require A Probe In A Tradeoff

Use a probe-backed requirement when a local regression is acceptable only if a
named phase improved enough.

```toml
[[tradeoff]]
name = "memory-for-batch-loop-speed"
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

This reads as:

```text
Accept the memory warning only if parser.batch_loop improves by at least 10%,
and only while parser.tokenize stays within a 3% local regression cap.
```

If the required probe is missing or too noisy under decision policy, perfgate
marks the decision as review-required instead of pretending the evidence is
complete.

## 7. Review The Output

The reviewer-facing artifact is:

```text
artifacts/perfgate/decision.md
```

The machine-readable manifest is:

```text
artifacts/perfgate/decision.index.json
```

Bundle the evidence when it needs to travel:

```bash
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

For a deterministic fixture that exercises probe ingest, probe compare,
scenario evaluation, tradeoff evaluation, and decision rendering, see
[`examples/performance-decision`](../examples/performance-decision/README.md).

