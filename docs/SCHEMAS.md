# Output Schemas

perfgate uses versioned JSON receipts at every stage of the pipeline.

## Receipt Types

| Schema | Produced by | Description |
|--------|-------------|-------------|
| `perfgate.run.v1` | `run`, `check` | Raw measurement data from a benchmark execution |
| `perfgate.compare.v1` | `compare`, `check`, `paired` | Comparison of current run against baseline |
| `perfgate.report.v1` | `report`, `check` | Cockpit-compatible report envelope |
| `sensor.report.v1` | `check --mode cockpit` | Sensor integration envelope for dashboards |

## JSON Schema Generation

Auto-generated schemas (via `schemars`):

```bash
# Generate to schemas/
cargo run -p xtask -- schema

# Verify committed schemas match generated output
cargo run -p xtask -- schema-check
```

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
