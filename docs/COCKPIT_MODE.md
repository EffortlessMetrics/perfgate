# Cockpit Mode

Cockpit mode wraps perfgate output in a `sensor.report.v1` envelope for
integration with monitoring dashboards. In cockpit mode, perfgate always exits
`0` (unless catastrophic) and writes structured error reports instead of failing.

## Usage

```bash
perfgate check --config perfgate.toml --bench my-bench --mode cockpit
```

## Artifact Layout

Single bench:

```
artifacts/perfgate/
├── report.json                         # sensor.report.v1 envelope
├── comment.md                          # PR comment markdown
└── extras/
    ├── perfgate.run.v1.json
    ├── perfgate.compare.v1.json        # omitted if no baseline
    └── perfgate.report.v1.json
```

Multi-bench (`--all`):

```
artifacts/perfgate/
├── report.json                         # aggregated sensor.report.v1
├── comment.md
└── extras/
    ├── bench-a/perfgate.run.v1.json
    ├── bench-a/perfgate.compare.v1.json
    ├── bench-a/perfgate.report.v1.json
    ├── bench-b/perfgate.run.v1.json
    └── ...
```

## Error Handling

If any stage fails, cockpit mode emits an error sensor report with:
- `check_id`: `tool.runtime`
- Structured data: `{ "stage": "run_command", "error_kind": "exec_error" }`

Stage constants: `config_parse`, `baseline_resolve`, `run_command`, `write_artifacts`.

## Schema

The `sensor.report.v1` schema is hand-written and vendored at
`contracts/schemas/sensor.report.v1.schema.json`. It is not auto-generated.

Validate cockpit output:

```bash
cargo run -p xtask -- conform --file artifacts/perfgate/report.json
```
