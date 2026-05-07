# perfgate-app

Workspace-only compatibility wrapper for `perfgate::app`.

Application-layer orchestration now lives under the public facade path
`perfgate::app`. This package remains in the workspace as a private migration
shim and is marked `publish = false`.

## Responsibilities

- Implements use-cases:
  - `RunBenchUseCase`
  - `CompareUseCase`
  - `CheckUseCase`
  - `ReportUseCase`
  - `PromoteUseCase`
  - `PairedRunUseCase`
  - `ExportUseCase`
- Coordinates `perfgate::domain` logic with runtime runners/probes.
- Renders markdown summaries and GitHub annotation lines.
- Builds cockpit-mode sensor envelopes (`sensor.report.v1`) and structured findings.
- Exposes stable request/response structs for CLI and other integrations.

## Boundaries

- No CLI flag parsing (that belongs in `perfgate-cli`).
- No direct filesystem artifact writing (done by CLI callers).
- Low-level process/OS primitives stay isolated in the `runtime` module.

## Export Support

`ExportUseCase` supports `csv`, `jsonl`, `html`, and `prometheus` output for run/compare receipts.

## Workspace Role

`perfgate::app` is the orchestration layer above domain + runtime and below the CLI:

`perfgate-types` -> `perfgate::domain` -> `perfgate::runtime` -> `perfgate::app` -> `perfgate-cli`

## License

Licensed under either Apache-2.0 or MIT.
