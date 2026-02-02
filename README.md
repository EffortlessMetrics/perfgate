# perfgate

A small Rust CLI for **performance budgets** and **baseline diffs**.

`perfgate` is designed for modern dev automation:
- emits stable **JSON receipts** for artifacts
- renders a compact **Markdown table** for PR comments
- can output **GitHub Actions annotations**
- uses boring policy defaults (median-based, thresholded)

## Install

From source:

```bash
cargo install --path crates/perfgate-cli
```

Or run in-repo:

```bash
cargo run -p perfgate -- --help
```

## Quickstart

### 1) Run a command and write a receipt

```bash
perfgate run \
  --name pst_extract \
  --repeat 7 \
  --warmup 1 \
  --work 1000 \
  --out artifacts/perf/current.json \
  -- \
  sh -c 'sleep 0.02'
```

### 2) Compare to a baseline

```bash
perfgate compare \
  --baseline baselines/perf/pst_extract.json \
  --current artifacts/perf/current.json \
  --threshold 0.20 \
  --warn-factor 0.90 \
  --out artifacts/perf/compare.json
```

Exit codes for `compare`:
- `0`: pass (or warn if you didn't set `--fail-on-warn`)
- `2`: fail (budget violated)
- `3`: warn treated as failure (only when `--fail-on-warn`)
- `1`: tool error (I/O, parse, spawn failures)

### 3) Render a PR-ready comment

```bash
perfgate md --compare artifacts/perf/compare.json --out artifacts/perf/comment.md
```

### 4) GitHub Actions annotations

```bash
perfgate github-annotations --compare artifacts/perf/compare.json
```

## Output schemas

Receipts are versioned:
- `perfgate.run.v1`
- `perfgate.compare.v1`

Generate JSON Schemas:

```bash
cargo run -p xtask -- schema
```

Schemas are written to `schemas/`.

## Design

Workspace structure:
- `perfgate-types`: receipt/config types + JSON schema support
- `perfgate-domain`: pure math/policy (stats + budget comparison)
- `perfgate-adapters`: process runner + best-effort system metrics
- `perfgate-app`: use-cases + Markdown/annotation rendering
- `perfgate` (cli): clap interface + JSON read/write
- `xtask`: schema generation

### Measurement model

- Uses wall-clock time (median) for gating.
- Supports optional `work_units` to compute `throughput_per_s`.
- On Unix, attempts to collect `ru_maxrss` via `wait4()`.

## Testing

See [TESTING.md](TESTING.md) for the comprehensive testing guide covering:
- Unit and integration tests
- BDD tests with Cucumber
- Property-based tests with proptest
- Fuzz testing with cargo-fuzz
- Mutation testing with cargo-mutants

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for release history.

## License

Dual-licensed under MIT or Apache-2.0.
