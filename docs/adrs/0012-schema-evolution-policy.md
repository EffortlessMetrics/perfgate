# ADR 0012: Schema Evolution Policy

## Status
Accepted

## Context
All perfgate receipt schemas are at v1 (`perfgate.run.v1`, `perfgate.compare.v1`, `perfgate.report.v1`, `sensor.report.v1`). Adoption is growing across CI pipelines and dashboard integrations. ADR 0007 established that schemas are append-only within a version but explicitly deferred the question of how a v2 migration would work.

Before any breaking change ships, we need a documented policy that:
- Defines what constitutes a breaking vs. non-breaking change.
- Guarantees a coexistence window so consumers are never forced into an instant upgrade.
- Provides tooling for offline migration of stored baselines and receipts.
- Ensures the server can route requests by schema version.

## Decision

### 1. Additive changes within a major version

Within a given major version (e.g., v1), the following changes are permitted without a version bump:

- **Adding optional fields** with `#[serde(default)]` — existing consumers ignore unknown fields.
- **Adding new enum variants** to open enums (e.g., new severity levels) — consumers that encounter an unknown variant should treat it as a warning, not a parse failure.
- **Adding new receipt types** (e.g., `perfgate.aggregate.v1`) — these are independent schemas and do not affect existing ones.

The following changes are **forbidden** within a major version:

- Removing or renaming an existing field.
- Changing the type of an existing field (e.g., `string` to `integer`).
- Changing the semantics of an existing field (e.g., `duration_ms` switching from wall-clock to CPU time).
- Making an optional field required.
- Removing an enum variant.

### 2. Breaking changes require a new major version

Any change that violates the rules above triggers a new major version (`v2`). When a v2 schema is introduced:

- A new constant is added in `perfgate-types` (e.g., `RUN_SCHEMA_V2`).
- The new schema type is a separate Rust struct (e.g., `RunReceiptV2`), not a modification of the v1 struct. This keeps v1 deserialization stable forever.
- The `schema` field value changes to `perfgate.run.v2`, allowing consumers to branch on the version string.

### 3. Coexistence: v(N) and v(N-1) served simultaneously

When a new major version ships:

- The CLI **emits the latest version** by default but accepts `--schema-version v1` to produce the old format.
- The server **accepts and stores both versions**. API routes are namespaced: `/api/v1/...` continues to serve v1 receipts, while `/api/v2/...` serves v2 receipts. The `/api/v1/` endpoints remain fully functional during the coexistence window.
- Readers **detect the version** by inspecting the `schema` field (already present in every receipt per ADR 0007) and dispatch to the appropriate deserializer.

### 4. Deprecation timeline

- **v(N-1) is supported for at least 2 minor releases** after v(N) is declared stable.
- Deprecation is announced in the changelog and via a CLI warning when `--schema-version` selects a deprecated version.
- After the deprecation window, v(N-1) endpoints emit HTTP `299` deprecation warnings (via the `Deprecation` header) for one additional minor release before removal.
- The removal release drops the old API routes and struct definitions. The old JSON Schema files are moved to `schemas/archived/` for reference.

Example timeline for a hypothetical v2 introduction:

| Release | v1 status | v2 status |
|---------|-----------|-----------|
| 0.20.0  | current   | beta (opt-in via `--schema-version v2`) |
| 0.21.0  | deprecated | stable (default) |
| 0.22.0  | deprecated | stable |
| 0.23.0  | removed   | stable |

### 5. Migration tooling: `perfgate migrate`

A `perfgate migrate` CLI command will be provided for offline conversion of stored artifacts:

```bash
# Convert a single file from v1 to v2
perfgate migrate --from v1 --to v2 --file baselines/bench.json

# Convert all JSON files in a directory
perfgate migrate --from v1 --to v2 --dir baselines/

# Dry-run: show what would change without writing
perfgate migrate --from v1 --to v2 --dir baselines/ --dry-run
```

The migration command:
- Reads the `schema` field to confirm the source version.
- Applies a deterministic transform (field renames, restructures, new required fields with default values).
- Writes the output with the new `schema` field value.
- Is idempotent: running it on an already-migrated file is a no-op.

The server will also expose a `/api/v2/migrate` endpoint that accepts a v1 receipt body and returns the v2 equivalent, enabling CI pipelines to migrate on-the-fly without local tooling.

### 6. Version detection and routing

All receipts already contain a `schema` field as the first key (per ADR 0007). Consumers should use this field to determine the version:

```rust
let value: serde_json::Value = serde_json::from_str(&json)?;
match value["schema"].as_str() {
    Some("perfgate.run.v1") => { /* deserialize as RunReceipt */ }
    Some("perfgate.run.v2") => { /* deserialize as RunReceiptV2 */ }
    Some(other) => { /* unknown schema — warn and skip */ }
    None => { /* legacy file without schema field */ }
}
```

The server routes by URL prefix (`/api/v1`, `/api/v2`) for write operations and by `schema` field for read operations, ensuring that stored baselines are returned in their original version unless the client requests conversion.

### 7. JSON Schema files

- Auto-generated schemas (via `schemars`) are published per version: `schemas/perfgate.run.v1.schema.json`, `schemas/perfgate.run.v2.schema.json`.
- The hand-written `sensor.report.v1.schema.json` remains vendored at `contracts/schemas/`. A `sensor.report.v2.schema.json` would follow the same pattern.
- `xtask conform` validates fixtures against the schema version declared in each fixture's `schema` field.

## Consequences

- **Stability guarantee**: Consumers on v1 are protected from breakage for at least 2 minor releases after v2 ships.
- **No flag day**: CI pipelines can migrate at their own pace during the coexistence window.
- **Tooling overhead**: The `migrate` command and dual-version server routes add implementation cost, but this is preferable to silent data loss or parse failures in production pipelines.
- **Struct duplication**: Maintaining separate v1 and v2 Rust types means some code duplication. Shared logic should be extracted into version-agnostic helper functions in `perfgate-domain`.
- **Schema field as router**: The existing `schema` field (ADR 0007) is sufficient for version detection — no new wire-format changes are needed to support this policy.
- **Archived schemas**: Removed versions remain available in `schemas/archived/` for forensic analysis of old artifacts.
