# xtask

Developer automation crate for the perfgate workspace.

## What It Does

- Generates and checks JSON schemas (`schema`, `schema-check`).
- Runs the standard CI command bundle (`ci`).
- Validates fixtures against vendored contracts (`conform`).
- Syncs golden fixtures into `contracts/fixtures` (`sync-fixtures`).
- Runs mutation testing helpers (`mutants`).

## Why It Exists

`xtask` keeps project maintenance flows in typed Rust code instead of shell scripts, so local dev and CI use the same logic.

## Usage

```bash
cargo run -p xtask -- schema
cargo run -p xtask -- schema-check
cargo run -p xtask -- ci
cargo run -p xtask -- conform
cargo run -p xtask -- mutants --crate perfgate-domain --summary
```

## License

Licensed under either Apache-2.0 or MIT.
