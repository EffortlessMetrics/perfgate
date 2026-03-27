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
