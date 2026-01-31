# perfgate fuzzing

This directory is excluded from the main workspace so `cargo test` works on stable.

To fuzz (requires nightly + cargo-fuzz):

```bash
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
cargo fuzz list
cargo fuzz run parse_run_receipt
```

Targets:
- `parse_run_receipt`: JSON bytes -> `RunReceipt`
- `parse_compare_receipt`: JSON bytes -> `CompareReceipt`
- `render_markdown`: valid `CompareReceipt` -> `render_markdown()` (should never panic)
