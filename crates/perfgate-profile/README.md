# perfgate-profile

Automatic flamegraph profiling for `perfgate` regression diagnostics.

This crate detects an available system profiler and captures a flamegraph SVG
when a regression needs deeper diagnosis.

Supported profiler backends:

- `perf` + `inferno` on Linux
- `dtrace` + `inferno` on macOS
- `cargo flamegraph` as a cross-platform fallback

The library is used by `perfgate-cli` for `--profile-on-regression`, but it can
also be embedded directly when you want profiler detection and flamegraph
capture as a reusable library.
