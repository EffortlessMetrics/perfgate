# ADR 0007: Versioned JSON Receipts

## Status
Accepted

## Context
perfgate produces artifacts at every stage of its pipeline (run, compare, report). These artifacts need to be:
- Machine-readable for downstream tooling (CI, dashboards, export)
- Stable enough that consumers don't break on upgrades
- Self-describing so a reader can determine the schema without external context

## Decision
Every perfgate output file includes a `schema` field identifying its type and version:
- `perfgate.run.v1` — raw measurement data from a benchmark execution
- `perfgate.compare.v1` — comparison of current run against baseline
- `perfgate.report.v1` — cockpit-compatible report envelope
- `sensor.report.v1` — sensor integration envelope for dashboard ingestion

Key design choices:
1. **Schema field is always the first key** in the JSON object for easy identification.
2. **Schemas are append-only** — new fields can be added to a version, but existing fields are never removed or retyped within a version.
3. **Auto-generated schemas** via `schemars` for run, compare, and report types. The `sensor.report.v1` schema is hand-written and vendored at `contracts/schemas/` because it must remain stable for external consumers.
4. **Conformance testing** via `xtask conform` validates all fixtures against the vendored schema in CI.

## Consequences
- Consumers can rely on `schema` field to route parsing logic.
- Adding new metrics (e.g., `io_read_bytes`) doesn't break existing consumers.
- Schema evolution to v2 will require a documented migration path (not yet defined).
- The hand-written sensor schema requires manual maintenance when the report structure changes.
