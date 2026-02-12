# perfgate-types

Shared types and schemas for perfgate receipts. This is the **innermost crate** in the dependency graph — every other crate depends on it.

## Build and Test

```bash
cargo test -p perfgate-types
cargo test -p perfgate-types -- proptest   # property-based tests only
```

## What This Crate Contains

All versioned receipt structs, config types, and sensor report types. Everything here is a plain data type with `Serialize`/`Deserialize`/`JsonSchema` derives.

### Source Layout

- `src/lib.rs` — All core types: run receipts, compare receipts, reports, config, sensor report, constants
- `src/paired.rs` — Paired benchmarking types (`PairedRunReceipt`, `PairedSample`, `PairedStats`)

### Key Type Groups

**Schema identifiers** (string constants):
- `RUN_SCHEMA_V1`, `COMPARE_SCHEMA_V1`, `REPORT_SCHEMA_V1`, `CONFIG_SCHEMA_V1`

**Check ID / finding code constants**:
- `CHECK_ID_BUDGET`, `CHECK_ID_BASELINE`, `CHECK_ID_HOST`, `CHECK_ID_TOOL_RUNTIME`
- `FINDING_CODE_METRIC_WARN`, `FINDING_CODE_METRIC_FAIL`, `FINDING_CODE_BASELINE_MISSING`, etc.

**Run types** (`perfgate.run.v1`):
- `RunReceipt`, `RunMeta`, `BenchMeta`, `Sample`, `Stats`, `U64Summary`, `F64Summary`

**Compare types** (`perfgate.compare.v1`):
- `CompareReceipt`, `Metric`, `Direction`, `Budget`, `MetricStatus`, `Delta`, `Verdict`, `VerdictCounts`

**Report types** (`perfgate.report.v1`):
- `PerfgateReport`, `ReportFinding`, `FindingData`, `ReportSummary`, `Severity`

**Config types** (`perfgate.config.v1`):
- `ConfigFile`, `DefaultsConfig`, `BenchConfigFile`, `BudgetOverride`

**Sensor report types** (cockpit integration):
- `SensorReport`, `SensorVerdict`, `SensorFinding`, `SensorArtifact`, `SensorCapabilities`
- `ToolInfo`, `HostInfo`, `HostMismatchPolicy`

**Paired types**:
- `PairedRunReceipt`, `PairedBenchMeta`, `PairedSample`, `PairedSampleHalf`, `PairedStats`

### The `Metric` Enum

Central to budget evaluation. Each variant has metadata:
- `Metric::as_str()` — canonical key name (`wall_ms`, `cpu_ms`, `max_rss_kb`, `throughput_per_s`)
- `Metric::parse_key()` — reverse lookup
- `Metric::default_direction()` — `Lower` or `Higher` (for regression detection)
- `Metric::default_warn_factor()` — default warning threshold

## Feature Flags

- `arbitrary` — Enables `Arbitrary` derive for structure-aware fuzzing with `cargo-fuzz`

## Design Rules

- **No I/O, no logic** — This crate is pure data. Statistics, policy, and rendering belong in `perfgate-domain` or `perfgate-app`.
- **Sensor report uses `serde_json::Value`** for opaque data fields (ABI hardening). This means `SensorReport` and related types cannot derive `JsonSchema` or `Arbitrary`.
- **All collections use `BTreeMap`** for deterministic serialization order.
- **Backward compatibility matters** — Adding fields must use `#[serde(default)]`. Removing or renaming fields is a breaking change. Existing tests validate backward compat for `HostInfo` and other types.

## Testing

- **Property-based tests** (proptest): Serialization round-trips for `RunReceipt`, `CompareReceipt`, `ConfigFile`, `Budget`, `BudgetOverride` — 100+ cases each
- **Snapshot tests** (insta): Backward compatibility for `HostInfo` defaults
- **TOML serialization tests**: Config file round-trips
- **JSON round-trip tests**: All receipt types
