# perfgate (CLI crate)

Command-line entrypoint for perfgate.

This crate wires Clap commands to application use-cases in `perfgate-app`, handles JSON/file I/O, and enforces exit code policy for CI usage.

## Commands

- `run`: execute a benchmark command and emit `perfgate.run.v1`.
- `compare`: compare current vs baseline and emit `perfgate.compare.v1`.
- `md`: render markdown from a compare receipt.
- `github-annotations`: emit GitHub Actions annotation lines.
- `report`: generate `perfgate.report.v1` (optionally markdown too).
- `promote`: copy/normalize a run receipt into baseline storage.
- `export`: export run/compare data (`csv`, `jsonl`, `html`, `prometheus`).
- `check`: config-driven workflow for artifacts and gating.
- `paired`: interleaved baseline/current benchmarking for noise reduction.
- `baseline`: manage baselines on a centralized baseline server.

## Quick Usage

```bash
perfgate run --name my-bench --out run.json -- ./my-benchmark
perfgate compare --baseline baseline.json --current run.json --out compare.json
perfgate md --compare compare.json --out comment.md
perfgate check --config perfgate.toml --bench my-bench
```

## Exit Codes

- `0`: success (or warn without `--fail-on-warn`)
- `1`: tool/runtime error
- `2`: policy fail
- `3`: warn treated as failure (`--fail-on-warn`)

## Scope

- This crate owns CLI UX, argument validation, and artifact file handling.
- It does not implement core policy math (domain) or process primitives (adapters).

## More Documentation

- Workspace overview and CI examples: [`README.md`](../../README.md)
- Testing strategy: [`TESTING.md`](../../TESTING.md)

## License

Licensed under either Apache-2.0 or MIT.
