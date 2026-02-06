# xtask

Repo automation — schema generation, CI pipeline, and mutation testing. Not published.

## Usage

```bash
cargo run -p xtask -- schema              # generate JSON schemas
cargo run -p xtask -- ci                   # full CI check
cargo run -p xtask -- mutants             # run mutation testing
cargo run -p xtask -- mutants --crate perfgate-domain --summary
```

## What This Crate Contains

A single `src/main.rs` with three commands.

### Commands

**`schema`** — Generates JSON Schema files for all receipt types into `schemas/`:
- `perfgate.run.v1.schema.json`
- `perfgate.compare.v1.schema.json`
- `perfgate.config.v1.schema.json`
- `perfgate.report.v1.schema.json`
- `sensor.report.v1.schema.json` (copied from `contracts/schemas/`, not generated)

**`ci`** — Runs the full CI pipeline in order:
1. `cargo fmt --all --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test --all`
4. `cargo run -p xtask -- schema`

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
