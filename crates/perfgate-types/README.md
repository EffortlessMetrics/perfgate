# perfgate-types

Versioned data contracts for perfgate.

## Responsibilities

- Defines receipt and report schemas:
  - `perfgate.run.v1`
  - `perfgate.compare.v1`
  - `perfgate.report.v1`
  - `sensor.report.v1`
  - paired benchmarking schema (`perfgate.paired.v1`)
- Defines config types (`ConfigFile`, `BenchConfigFile`, defaults/budget overrides).
- Defines shared enums/tokens used across crates (metrics, verdicts, finding codes, reason tokens).
- Provides JSON Schema derivation support through `schemars`.

## Boundaries

- No process execution or host probing.
- No statistics math or budget decision logic.
- No CLI parsing or filesystem I/O.

## Feature Flags

- `arbitrary`: enables structure-aware fuzzing derives for core types.

## Workspace Role

`perfgate-types` is the innermost crate and the shared contract layer:

`perfgate-types` -> `perfgate-domain` -> `perfgate-adapters` -> `perfgate-app` -> `perfgate` (CLI)

## License

Licensed under either Apache-2.0 or MIT.
