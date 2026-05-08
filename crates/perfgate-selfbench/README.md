# perfgate-selfbench

Internal benchmarking workloads for perfgate self-dogfooding.

## Overview

`perfgate-selfbench` is a small binary crate that ships deterministic
workloads and Rust-native wrappers for perfgate dogfooding commands. These workloads are executed by perfgate's own CI dogfooding lanes
so that perfgate can gate its own performance — eating its own dog food.

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
| `perfgate-compare-small` | Runs the small fixture comparison through the release `perfgate` binary. |
| `perfgate-compare-large` | Runs the large fixture comparison through the release `perfgate` binary. |
| `perfgate-check-single` | Runs the single-benchmark check fixture through the release `perfgate` binary. |
| `perfgate-check-no-baseline` | Runs the no-baseline check fixture through the release `perfgate` binary. |
| `perfgate-render-md` | Renders the compare fixture to markdown through the release `perfgate` binary. |
| `perfgate-render-report` | Renders the compare fixture to a cockpit report through the release `perfgate` binary. |

## Usage

```bash
# Run directly
cargo run -p perfgate-selfbench -- cpu-fixed

# Use with perfgate CLI for dogfooding
cargo run -p perfgate-cli -- run \
    --name selfbench-cpu \
    --repeat 5 \
    --out cpu-run.json \
    -- cargo run -p perfgate-selfbench -- cpu-fixed
```

## Workspace Role

`perfgate-selfbench` is a leaf binary used exclusively by CI:

`perfgate-cli` spawns **`perfgate-selfbench`** as the benchmark command. The dogfooding wrappers resolve `./target/release/perfgate` first and then fall back to `perfgate` on `PATH`, allowing the same Rust binary to work in both direct CI lanes and the composite action smoke lane.

## License

Licensed under either Apache-2.0 or MIT.
