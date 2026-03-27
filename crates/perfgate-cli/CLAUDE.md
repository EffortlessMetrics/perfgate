# perfgate-cli

CLI interface — clap argument parsing, JSON I/O, and command dispatch. This is the **outermost crate** (published as the `perfgate` binary).

## Build and Test

```bash
cargo test -p perfgate              # all integration tests
cargo test -p perfgate -- cockpit   # cockpit tests only
cargo test -p perfgate -- abi       # ABI conformance tests only
cargo run -p perfgate-cli -- --help     # see all commands
```

Mutation testing target: **70% kill rate**.

## What This Crate Contains

A single `src/main.rs` with clap command definitions and dispatch logic.

### Commands

| Command | Purpose |
|---------|---------|
| `run` | Execute a benchmark command, emit `perfgate.run.v1` |
| `compare` | Compare baseline vs current runs, emit `perfgate.compare.v1` |
| `md` | Render markdown from a compare receipt |
| `github-annotations` | Emit `::error::`/`::warning::` for GitHub Actions |
| `report` | Generate `perfgate.report.v1` from a compare receipt |
| `promote` | Copy a run receipt as baseline |
| `export` | Export run/compare to CSV, JSONL, HTML, Prometheus, or JUnit |
| `check` | Config-driven workflow (standard or cockpit mode) |
| `paired` | Paired A/B benchmarking |
| `baseline` | Manage baselines on baseline server (list, upload, download, delete, history, verdicts) |
| `summary` | Summarize compare receipts in terminal table |
| `aggregate` | Aggregate multiple run receipts (fleet) into one |
| `bisect` | Automated performance regression bisection via git bisect |
| `blame` | Analyze Cargo.lock dependency changes causing regressions |
| `explain` | AI-ready regression diagnostic prompts |

### Output Modes

- **Standard mode** — Exit codes reflect verdict (0=pass, 1=error, 2=fail, 3=warn)
- **Cockpit mode** — Always writes sensor.report.v1 envelope; exit 0 unless catastrophic. Used for CI cockpit integration.

### Cockpit Mode Details

Cockpit mode (`--mode cockpit`) wraps everything in a `sensor.report.v1` envelope:
- Single bench: extras at `extras/perfgate.run.v1.json`, etc.
- Multi-bench (`--all`): extras at `extras/{bench-name}/perfgate.run.v1.json`, etc.
- Error recovery: If any stage fails, emits an error sensor report with `tool.runtime` check_id and structured `{stage, error_kind}` data.

### Structured Error Handling

Errors carry metadata for cockpit:
- **Stage constants**: `config_parse`, `baseline_resolve`, `run_command`, `write_artifacts`
- **Error kinds**: `io_error`, `parse_error`, `exec_error`

## Integration Tests

16 test files in `tests/`:

| File | Coverage |
|------|----------|
| `cli_run_tests.rs` | Run command, output capture, timeouts |
| `cli_compare_tests.rs` | Compare, budget thresholds |
| `cli_md_tests.rs` | Markdown rendering |
| `cli_annotations_tests.rs` | GitHub annotations |
| `cli_promote_tests.rs` | Baseline promotion |
| `cli_report_tests.rs` | Report generation |
| `cli_check_tests.rs` | Config-driven check |
| `cli_export_tests.rs` | CSV/JSONL export |
| `cli_paired_tests.rs` | Paired sampling |
| `cli_host_mismatch_tests.rs` | Host detection, policy handling |
| `cli_cpu_time_tests.rs` | CPU time metrics |
| `cli_abi_conformance_tests.rs` | ABI contract validation |
| `cli_cockpit_tests.rs` | Cockpit mode artifacts |
| `cli_help_snapshot_tests.rs` | Help text snapshot validation |
| `cli_mock_server_tests.rs` | CLI client behavior with wiremock |
| `cli_server_tests.rs` | Real server integration tests |

Golden fixtures at `tests/fixtures/golden/sensor_report_*.json`.

## Platform Notes

- Tests use `cmd /c exit 0` on Windows instead of `true` for success commands
- The `cpu_work_command` warning in `cli_cpu_time_tests.rs` is pre-existing (Windows-only dead code)

## Design Rules

- **Thin layer** — CLI only parses args and delegates to `perfgate-app` use cases. No business logic here.
- **JSON pretty-printing** — Controlled by `--pretty` flag, not hardcoded.
- **Exit code contract** — Must match the documented exit code semantics (0/1/2/3).
