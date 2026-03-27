# perfgate-types

Canonical types and versioned schemas that every perfgate crate depends on.

`perfgate-types` is the innermost crate in the dependency graph. It defines all
receipt structs, configuration types, enums, and constants shared across the
workspace. Everything here is a plain data type -- no I/O, no statistics, no
policy logic.

## Schema Versions

| Schema                | Struct            | Purpose                           |
|-----------------------|-------------------|-----------------------------------|
| `perfgate.run.v1`     | `RunReceipt`      | Single benchmark execution result |
| `perfgate.compare.v1` | `CompareReceipt`  | Baseline vs current comparison    |
| `perfgate.report.v1`  | `PerfgateReport`  | Findings and verdict summary      |
| `perfgate.config.v1`  | `ConfigFile`      | Budget and benchmark definitions  |
| `perfgate.paired.v1`  | `PairedRunReceipt`| Paired A/B benchmark result       |
| `sensor.report.v1`    | `SensorReport`    | Cockpit integration envelope      |

## Key Types

**Run pipeline:** `RunReceipt`, `RunMeta`, `BenchMeta`, `Sample`, `Stats`,
`U64Summary`, `F64Summary`

**Comparison:** `CompareReceipt`, `Metric`, `Direction`, `Budget`, `Delta`,
`MetricStatus`, `Verdict`, `VerdictCounts`

**Reporting:** `PerfgateReport`, `ReportFinding`, `FindingData`, `Severity`

**Configuration:** `ConfigFile`, `DefaultsConfig`, `BenchConfigFile`,
`BudgetOverride`

**Sensor (cockpit):** `SensorReport`, `SensorVerdict`, `SensorFinding`,
`SensorCapabilities`, `ToolInfo`, `HostInfo`, `HostMismatchPolicy`

**Paired:** `PairedRunReceipt`, `PairedSample`, `PairedSampleHalf`,
`PairedStats`, `PairedDiffSummary`

## The `Metric` Enum

Central to budget evaluation. Each variant carries metadata:

- `as_str()` / `parse_key()` -- canonical snake_case key (`wall_ms`, `cpu_ms`, ...)
- `default_direction()` -- `Lower` or `Higher` (throughput is the only `Higher`)
- `default_warn_factor()` -- default warning threshold multiplier

## Feature Flags

| Flag        | Effect                                                  |
|-------------|---------------------------------------------------------|
| `arbitrary` | Enables `Arbitrary` derive for structure-aware fuzzing  |

JSON Schema generation via `schemars` is always available (not feature-gated).

## Design Constraints

- **All collections use `BTreeMap`** for deterministic serialization order.
- **Backward-compatible evolution:** new fields use `#[serde(default)]`;
  removing or renaming fields is a breaking change.
- **`SensorReport` uses `serde_json::Value`** for opaque data (ABI hardening),
  so it cannot derive `JsonSchema` or `Arbitrary`.
- **No logic:** statistics, policy, and rendering belong in downstream crates.

## Testing

- Property-based (proptest): serialization round-trips for `RunReceipt`,
  `CompareReceipt`, `ConfigFile`, `Budget`, `BudgetOverride`
- Snapshot (insta): backward compatibility for `HostInfo` defaults
- TOML and JSON round-trip tests for all receipt and config types

## License

Licensed under either Apache-2.0 or MIT.
