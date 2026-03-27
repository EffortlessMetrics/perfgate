# perfgate-sensor

Cockpit mode integration via `sensor.report.v1` envelopes.

CI dashboards and fleet-monitoring tools need a uniform envelope around
every tool's output. This crate wraps perfgate's typed receipts into a
generic `SensorReport` that any cockpit system can ingest without
knowing anything about performance budgets.

## What It Does

A `SensorReport` envelope contains:

- **Tool metadata** -- name, version
- **Run metadata** -- timestamps, duration, capability flags (baseline available, engine features)
- **Verdict** -- pass/warn/fail with counts
- **Findings** -- individual check results, each with a stable SHA-256 fingerprint
- **Artifacts** -- links to detailed run/compare/report receipts
- **Data** -- opaque summary payload for downstream consumers

## Key API

```rust
use perfgate_sensor::SensorReportBuilder;

let report = SensorReportBuilder::new(tool_info, started_at)
    .ended_at(ended_at, duration_ms)
    .baseline(true, None)
    .artifact("extras/perfgate.run.v1.json".into(), "run_receipt".into())
    .build(&perfgate_report);
```

### Builder Methods

| Method | Purpose |
|--------------------|------------------------------------------------|
| `new(tool, start)` | Create builder with tool info and start time |
| `ended_at(t, ms)` | Set end time and duration |
| `baseline(ok, reason)` | Declare baseline availability |
| `max_findings(n)` | Cap findings (default 100, adds truncation notice) |
| `artifact(path, type)` | Attach an artifact link |
| `build(report)` | Single-bench envelope from a `PerfgateReport` |
| `build_error(msg, stage, code)` | Error envelope when the tool itself fails |
| `build_aggregated(outcomes)` | Multi-bench envelope from `Vec<BenchOutcome>` |

### Other Exports

| Item | Purpose |
|-----------------------------|----------------------------------------------|
| `sensor_fingerprint(findings)` | SHA-256 fingerprint over a set of findings |
| `default_engine_capability()` | Platform-aware capability detection |
| `BenchOutcome` | Success or Error outcome for aggregation |

## License

Licensed under either Apache-2.0 or MIT.
