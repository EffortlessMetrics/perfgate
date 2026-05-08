# Output Schemas

perfgate uses versioned JSON receipts at every stage of the pipeline.

## Receipt And Service Types

| Schema | Produced by | Description |
|--------|-------------|-------------|
| `perfgate.run.v1` | `run`, `check` | Raw measurement data from a benchmark execution |
| `perfgate.compare.v1` | `compare`, `check`, `paired` | Comparison of current run against baseline |
| `perfgate.probe.v1` | `ingest probes` | Named probe observations from internal phases or external instrumentation |
| `perfgate.probe_compare.v1` | `probe compare` | Probe-level deltas between two probe receipts |
| `perfgate.scenario.v1` | `scenario evaluate` | Weighted workload-scenario evidence across benchmarks, phases, or probe groups |
| `perfgate.tradeoff.v1` | `tradeoff evaluate` | Structured decision evidence for accepted or rejected performance tradeoffs |
| `perfgate.report.v1` | `report`, `check` | Cockpit-compatible report envelope with findings, summary, and optional `profile_path` diagnostic |
| `sensor.report.v1` | `check --mode cockpit` | Sensor integration envelope for dashboards |
| `perfgate.baseline.v1` | baseline service | Stored baseline record returned by the server |
| `perfgate.verdict.v1` | baseline service | Stored verdict history, including optional noise history fields |
| `perfgate.audit.v1` | baseline service | Append-only audit event for baseline, verdict, and key mutations; inferred by fixture filename because current audit events do not include a `schema` field |
| `perfgate.health.v1` | baseline service | Health response for liveness and storage readiness; inferred by fixture filename because `/health` responses do not include a `schema` field |
| `perfgate.dependency_event.v1` | fleet API | Dependency-change event with performance impact |
| `perfgate.fleet_alert.v1` | fleet API | Fleet-wide dependency regression alert |

For the normal structured-decision workflow, users should not run these receipt
producers one by one. Run:

```bash
perfgate decision evaluate --config perfgate.toml
```

That command reads the configured compare receipts, evaluates scenarios,
evaluates tradeoff rules, and renders `decision.md` alongside
`scenario.json` and `tradeoff.json`.

## Additional Generated Schemas

perfgate also commits generated schemas for tooling and editor integration:

| File | Purpose |
|------|---------|
| `schemas/perfgate.config.v1.schema.json` | Validates `perfgate.toml` / JSON config shape, including optional per-benchmark scaling configuration |
| `schemas/perfgate.probe.v1.schema.json` | Validates probe receipts for named phase/span metrics from external instrumentation |
| `schemas/perfgate.probe_compare.v1.schema.json` | Validates probe delta receipts used to explain local phase movement |
| `schemas/perfgate.scenario.v1.schema.json` | Validates weighted scenario receipts used to explain workload-level outcomes |
| `schemas/perfgate.tradeoff.v1.schema.json` | Validates tradeoff receipts that explain why local regressions were accepted or rejected |
| `schemas/perfgate.report.v1.schema.json` | Validates report receipts, including additive diagnostics such as `profile_path` |

## JSON Schema Generation

Auto-generated schemas (via `schemars`):

```bash
# Generate to schemas/
cargo run -p xtask -- schema

# Verify committed schemas match generated output
cargo run -p xtask -- schema-check

# Verify old release fixtures still deserialize with current types
cargo run -p xtask -- schema-compat
```

## Probe JSONL Ingestion

`perfgate ingest probes` converts language-agnostic probe JSONL into a
`perfgate.probe.v1` receipt:

```bash
perfgate ingest probes --file probes.jsonl --out probe.json
```

Each non-empty JSONL line is one probe observation. Lines may use the full
`ProbeObservation` shape, or a compact flat shape where numeric top-level fields
become metrics:

```json
{"probe":"parser.tokenize","scope":"local","wall_ms":12.4,"alloc_bytes":184320,"items":10000}
```

## Probe Comparison

`perfgate probe compare` reads two `perfgate.probe.v1` receipts, matches
probes by name, compares shared numeric metrics, and writes a
`perfgate.probe_compare.v1` receipt:

```bash
perfgate probe compare --baseline baselines/probes.json --current artifacts/perfgate/probes.json --out artifacts/perfgate/probe-compare.json
```

Missing probes or missing metrics are recorded as warnings instead of policy
failures. This keeps early probe evidence advisory while still producing
durable deltas that scenario and tradeoff workflows can attach.

## Scenario Evaluation

`perfgate scenario evaluate` is the primitive command behind
`decision evaluate`. It reads configured `[[scenario]]` entries and their
benchmark compare receipts, then writes a `perfgate.scenario.v1` weighted
workload receipt:

```bash
perfgate scenario evaluate --config perfgate.toml --out artifacts/perfgate/scenario.json
```

By default, each scenario reads `[defaults].out_dir/<bench>/compare.json`.
Set `compare = "path/to/compare.json"` on a scenario to override that lookup.
Set `probe_compare = "path/to/probe-compare.json"` to attach advisory probe
delta evidence. Scenario receipts record the probe names and a
`probe_compare_ref`; consumers follow that reference for full probe deltas, and
probe evidence does not change the scenario verdict yet.

## Tradeoff Evaluation

`perfgate tradeoff evaluate` is the primitive command behind
`decision evaluate`. It reads configured `[[tradeoff]]` rules and a
`perfgate.scenario.v1` receipt, then writes a `perfgate.tradeoff.v1` decision
receipt:

```bash
perfgate tradeoff evaluate --config perfgate.toml --scenario artifacts/perfgate/scenario.json --out artifacts/perfgate/tradeoff.json
```

The receipt records configured rules, requirement outcomes, the final decision,
and the weighted deltas after any accepted downgrade.

When a tradeoff requirement includes `probe = "name"`, evaluation uses
scenario-attached `probe_compare_ref` receipts to find that probe's metric
delta. Probe-backed requirement outcomes record the probe name, and matching
probe deltas are copied into the tradeoff receipt's `probes` section for review.

Render the decision evidence for review:

```bash
perfgate md --tradeoff artifacts/perfgate/tradeoff.json
perfgate comment --tradeoff artifacts/perfgate/tradeoff.json --dry-run
```

For the paved local workflow, use `decision evaluate` instead of manually
chaining the primitive commands:

```bash
perfgate decision evaluate --config perfgate.toml
```

By default it writes:

```text
artifacts/perfgate/scenario.json
artifacts/perfgate/tradeoff.json
artifacts/perfgate/decision.md
```

For a runnable probe/scenario/tradeoff fixture, see
[`examples/performance-decision`](../examples/performance-decision/README.md).

## Fixture Validation

Validate JSON files against the vendored `sensor.report.v1` schema:

```bash
# Validate all known fixtures
cargo run -p xtask -- conform

# Validate a specific file
cargo run -p xtask -- conform --file path/to/report.json

# Validate all JSON files in a directory
cargo run -p xtask -- conform --fixtures path/to/dir
```

The vendored schema lives at `contracts/schemas/sensor.report.v1.schema.json`.
This schema is hand-written (not auto-generated) to maintain a stable contract
with external consumers.

Historical compatibility fixtures live under `fixtures/schema/<release>/`.
`schema-compat` checks v0.15 examples for `perfgate.run.v1`,
`perfgate.compare.v1`, `perfgate.report.v1`, `sensor.report.v1`,
and `perfgate.health.v1`.
It also checks v0.16 baseline-service and fleet contract fixtures for
`perfgate.baseline.v1`, `perfgate.verdict.v1`, `perfgate.audit.v1`,
`perfgate.health.v1`, `perfgate.dependency_event.v1`, and
`perfgate.fleet_alert.v1`, plus structured-evidence fixtures for
`perfgate.probe.v1`, `perfgate.probe_compare.v1`,
`perfgate.scenario.v1`, and `perfgate.tradeoff.v1`.

## Versioning Policy

Every receipt includes a `schema` field (e.g., `"perfgate.run.v1"`) that
identifies its type and major version. The full evolution policy is defined in
[ADR 0012](adrs/0012-schema-evolution-policy.md). Key guarantees:

- **Additive only within a major version.** New optional fields may be added to
  v1 at any time. Existing fields are never removed, renamed, or retyped.
- **Breaking changes require a new major version.** A v2 schema introduces a
  separate Rust struct and a new `schema` field value (e.g., `perfgate.run.v2`).
- **Coexistence window.** When v(N) ships, v(N-1) remains fully supported for
  at least 2 minor releases. The server serves both versions simultaneously via
  `/api/v1` and `/api/v2` route prefixes.
- **Migration tooling.** `perfgate migrate` converts stored artifacts between
  major versions offline. The server also exposes a migration endpoint for
  on-the-fly conversion.
- **Deprecation signals.** Deprecated versions emit CLI warnings and HTTP
  `Deprecation` headers before removal.
