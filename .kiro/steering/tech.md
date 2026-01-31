# Tech Stack

## Language & Toolchain
- Rust (stable channel)
- Edition 2021
- Components: rustfmt, clippy

## Key Dependencies
- `clap` (derive) - CLI parsing
- `serde` / `serde_json` - serialization
- `schemars` - JSON schema generation
- `anyhow` / `thiserror` - error handling
- `time` - timestamp formatting
- `uuid` - unique IDs
- `humantime` - duration parsing
- `libc` - Unix process metrics (conditional)

## Test Dependencies
- `assert_cmd` - CLI integration tests
- `predicates` - assertion helpers
- `insta` - snapshot testing

## Common Commands

### Build & Test
```bash
cargo build                    # Build all crates
cargo test --all               # Run all tests
cargo run -p perfgate -- --help  # Run CLI
```

### CI Workflow (all checks)
```bash
cargo run -p xtask -- ci
```
This runs: fmt check, clippy, tests, schema generation

### Generate JSON Schemas
```bash
cargo run -p xtask -- schema
```
Outputs to `schemas/` directory

### Mutation Testing
```bash
cargo install cargo-mutants
cargo run -p xtask -- mutants
```

### Fuzzing (requires nightly)
```bash
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
cargo fuzz run parse_run_receipt
```

## Licensing
Dual-licensed: MIT OR Apache-2.0
