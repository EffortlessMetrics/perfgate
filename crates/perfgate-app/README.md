# perfgate-app

Application-layer orchestration for perfgate workflows.

## Responsibilities

- Implements use-cases:
  - `RunBenchUseCase`
  - `CompareUseCase`
  - `CheckUseCase`
  - `ReportUseCase`
  - `PromoteUseCase`
  - `PairedRunUseCase`
  - `ExportUseCase`
- Coordinates `perfgate-domain` logic with `perfgate-adapters` runners/probes.
- Renders markdown summaries and GitHub annotation lines.
- Builds cockpit-mode sensor envelopes (`sensor.report.v1`) and structured findings.
- Exposes stable request/response structs for CLI and other integrations.

## Boundaries

- No CLI flag parsing (that belongs in `perfgate`).
- No direct filesystem artifact writing (done by CLI callers).
- No low-level process/OS primitives (handled by `perfgate-adapters`).

## Export Support

`ExportUseCase` supports `csv`, `jsonl`, `html`, and `prometheus` output for run/compare receipts.

## Workspace Role

`perfgate-app` is the orchestration layer above domain + adapters and below the CLI:

`perfgate-types` -> `perfgate-domain` -> `perfgate-adapters` -> `perfgate-app` -> `perfgate`

## License

Licensed under either Apache-2.0 or MIT.
