# perfgate-selfbench

Internal benchmarking workloads for perfgate self-dogfooding.

## Overview

`perfgate-selfbench` is a small binary crate that ships deterministic
workloads and Rust-native wrappers for perfgate's CI dogfooding lanes. These
workloads are executed by perfgate's own CI so that perfgate can gate its own
performance — eating its own dog food.

The binary is invoked by `perfgate run` the same way any user benchmark would
be, making it a realistic end-to-end test of the entire pipeline.

> **Note:** This crate is `publish = false` and is not published to crates.io.
> It exists solely for internal CI use.

## Workloads

| Command | What it does |
|---------|--------------|
| `noop` | Exits immediately. Measures baseline overhead of process spawning and measurement. |
| `cpu-fixed` | Performs 10 million wrapping additions. Deterministic CPU-bound workload. |
| `io-fixed` | Writes and reads back 1 MB to a temp file. Deterministic I/O-bound workload. |
| `json-read` | Parses a JSON string (or file if a path argument is given). Exercises serde_json. |
| `cli-compare-small` | Runs the release `perfgate compare` binary against the small comparison fixtures. |
| `cli-compare-large` | Runs the release `perfgate compare` binary against the large comparison fixtures. |
| `cli-check-single` | Runs the release `perfgate check` binary against the single-bench fixture. |
| `cli-check-no-baseline` | Runs the release `perfgate check` binary against the no-baseline fixture. |
| `render-md` | Runs the release `perfgate md` renderer against the comparison fixture. |
| `render-report` | Runs the release `perfgate report` renderer against the comparison fixture. |

## Usage

```bash
# Run directly
cargo run -p perfgate-selfbench -- cpu-fixed

# Run a dogfooding wrapper after building release binaries
cargo build --release -p perfgate-cli --bin perfgate
cargo build --release -p perfgate-selfbench
./target/release/perfgate-selfbench cli-compare-small

# Use with perfgate CLI for dogfooding
cargo run -p perfgate-cli -- run \
    --name selfbench-cpu \
    --repeat 5 \
    --out cpu-run.json \
    -- cargo run -p perfgate-selfbench -- cpu-fixed
```

## Workspace Role

`perfgate-selfbench` is a leaf binary used exclusively by CI:

`perfgate-cli` spawns **`perfgate-selfbench`** as the benchmark command

## License

Licensed under either Apache-2.0 or MIT.
