# perfgate-selfbench

Internal benchmarking workloads for perfgate self-dogfooding.

## Overview

`perfgate-selfbench` is a small binary crate that ships four deterministic
workloads. These workloads are executed by perfgate's own CI dogfooding lanes
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

`perfgate-cli` spawns **`perfgate-selfbench`** as the benchmark command

## License

Licensed under either Apache-2.0 or MIT.
