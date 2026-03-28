# xtask

Repo automation — schema generation, CI pipeline, and mutation testing. Not published.

## Usage

```bash
cargo run -p xtask -- schema              # generate JSON schemas
cargo run -p xtask -- schema-check        # verify committed schemas are locked
cargo run -p xtask -- ci                   # full CI check
cargo run -p xtask -- publish-check       # validate publish metadata before release
cargo run -p xtask -- conform             # validate fixtures against schema
cargo run -p xtask -- conform --file f.json  # validate a single file
cargo run -p xtask -- mutants             # run mutation testing
cargo run -p xtask -- mutants --crate perfgate-domain --summary
```

## What This Crate Contains

A single `src/main.rs` with automation commands.

### Commands

**`schema`** — Generates JSON Schema files for all receipt types into `schemas/`:
- `perfgate.run.v1.schema.json`
- `perfgate.compare.v1.schema.json`
- `perfgate.config.v1.schema.json`
- `perfgate.report.v1.schema.json`
- `sensor.report.v1.schema.json` (copied from `contracts/schemas/`, not generated)

**`schema-check`** — Verifies `schemas/` is byte-for-byte identical to fresh generated output:
- Detects missing schema files
- Detects modified/drifted schema files
- Detects extra stale `*.json` files in `schemas/`
- Exits non-zero with remediation hint (`xtask schema`)

**`ci`** — Runs the full CI pipeline in order:
1. `cargo fmt --all --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all`
4. `cargo run -p xtask -- schema-check`
5. `cargo run -p xtask -- conform`
6. `cargo run -p xtask -- publish-check`

**`publish-check`** — Performs fast static preflight checks for crates.io packaging:
- Fails if a publishable workspace crate depends on a `publish = false` workspace crate
- Fails if a publishable crate declares a `readme` file that does not exist
- Intended as a release guardrail before `cargo publish`

**`conform`** — Validates JSON fixtures against the vendored `sensor.report.v1` schema:
- Default: validates all `sensor_report_*.json` files in golden fixture directories
- `--file path/to/file.json`: validate a single file
- `--fixtures path/to/dir`: validate all `*.json` files in that directory (third-party mode)
- Exits non-zero if any fixture fails validation

**`mutants`** — Runs `cargo-mutants` with per-crate kill rate targets:

| Crate | Target Kill Rate |
|-------|-----------------|
| `perfgate-domain` | 100% |
| `perfgate-types` | 95% |
| `perfgate-app` | 90% |
| `perfgate-adapters` | 80% |
| `perfgate-cli` | 70% |

Parses `mutants.out/outcomes.json` to calculate actual rates.

## Design Rules

- **`sensor.report.v1.schema.json` is vendored** — It lives in `contracts/schemas/` and is hand-written. The `schema` command copies it; it does not generate it.
- **Schema generation uses `schemars`** — Types must derive `JsonSchema` to be included.
