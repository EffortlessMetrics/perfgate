# perfgate-app

Application / use-case layer — orchestrates adapters and domain logic into workflows. Also contains rendering (markdown, annotations) and the sensor report builder.

## Build and Test

```bash
cargo test -p perfgate-app
cargo test -p perfgate-app -- proptest   # property-based tests only
```

Mutation testing target: **90% kill rate**.

## What This Crate Contains

Use-case structs, rendering helpers, and the sensor report builder for cockpit integration.

### Source Layout

- `src/lib.rs` — `RunBenchUseCase`, `CompareUseCase`, `Clock` trait, markdown/annotation rendering
- `src/check.rs` — `CheckUseCase` (config-driven end-to-end workflow)
- `src/report.rs` — `ReportUseCase` (derive report from compare receipt)
- `src/sensor_report.rs` — `SensorReportBuilder` (cockpit sensor.report.v1 envelope)
- `src/export.rs` — `ExportUseCase` (CSV/JSONL flattening)
- `src/promote.rs` — `PromoteUseCase` (copy run to baseline, optional normalization)
- `src/paired.rs` — `PairedRunUseCase` (paired benchmarking workflow)

### Use-Case Pattern

Each workflow follows request/outcome types with a generic use-case struct:

```rust
struct RunBenchUseCase<R: ProcessRunner, H: HostProbe, C: Clock> { ... }
impl RunBenchUseCase {
    fn execute(&self, request: RunBenchRequest) -> Result<RunBenchOutcome>
}
```

Dependency injection via trait bounds (`ProcessRunner`, `HostProbe`, `Clock`) enables testing without real processes.

### Key Use Cases

| Use Case | Input | Output | What It Does |
|----------|-------|--------|-------------|
| `RunBenchUseCase` | `RunBenchRequest` | `RunBenchOutcome` | Runs warmup+measured samples, computes stats |
| `CompareUseCase` | `CompareRequest` | `CompareResult` | Compares baseline vs current with budgets |
| `CheckUseCase` | `CheckRequest` | `CheckOutcome` | End-to-end: run, compare, report, artifacts |
| `ReportUseCase` | `ReportRequest` | `ReportResult` | Derives report from compare receipt |
| `ExportUseCase` | run/compare receipt | CSV or JSONL | Flattens receipts for external tools |
| `PromoteUseCase` | `PromoteRequest` | `PromoteResult` | Copies run receipt as new baseline |
| `PairedRunUseCase` | `PairedRunRequest` | `PairedRunOutcome` | Paired A/B sampling with interleaved runs |

### Rendering Functions

- `render_markdown(compare) -> String` — Table with metrics, budgets, status icons
- `github_annotations(compare) -> Vec<String>` — `::error::` and `::warning::` lines for GitHub Actions
- `format_metric()`, `format_value()`, `format_pct()` — Display helpers
- `parse_reason_token()`, `render_reason_line()` — Contextual threshold info

### Sensor Report Builder

`SensorReportBuilder` wraps a `PerfgateReport` into a `sensor.report.v1` envelope for cockpit integration:
- Maps `Fail` verdict to `Error` (cockpit vocabulary)
- Collects artifacts sorted by `(type, path)` for deterministic output
- Adds capabilities, run metadata, tool info

## Design Rules

- **No CLI concerns** — This crate doesn't parse CLI args or write to stdout. It returns structured data that the CLI layer formats.
- **Use-case structs are generic over traits** — Enables fake runners/probes/clocks in tests.
- **CheckOutcome determines exit codes** — The `exit_code` field maps to the CLI exit code (0/2/3).
- **Sensor report uses `serde_json::Value`** for opaque data — Do not add `JsonSchema`/`Arbitrary` derives to types that flow through the sensor report.

## Testing

- **Property-based tests** (proptest): Markdown rendering completeness, annotation generation — 100+ cases
- **Unit tests**: Markdown output, metric formatting, sensor report mapping
