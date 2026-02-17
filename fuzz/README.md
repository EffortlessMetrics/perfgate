# perfgate-fuzz

Fuzz harnesses for perfgate contracts, domain logic, and rendering.

This crate is intentionally excluded from the main workspace so standard CI/test workflows can run on stable Rust.

## Requirements

- nightly toolchain
- `cargo-fuzz`

## Run

```bash
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
cd fuzz
cargo +nightly fuzz list
cargo +nightly fuzz run parse_run_receipt
```

## Targets

- `parse_run_receipt`: JSON bytes -> `RunReceipt`
- `parse_compare_receipt`: JSON bytes -> `CompareReceipt`
- `parse_report_receipt`: JSON bytes -> `PerfgateReport`
- `parse_config`: TOML text -> `ConfigFile`
- `parse_duration`: free-form text -> duration parser
- `compare_stats`: generated stats/budgets -> domain compare logic
- `render_markdown`: generated compare receipt -> markdown renderer (panic safety)
