# External Canary: droid-action Non-Rust Command Benchmark

Date: 2026-05-13

Status: observed

Linked proposal: [`PERFGATE-PROP-0003`](../proposals/PERFGATE-PROP-0003-external-adoption-canaries.md)

Linked specs:
[`PERFGATE-SPEC-0007`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md),
[`PERFGATE-SPEC-0004`](../specs/PERFGATE-SPEC-0004-user-devex-paved-road.md)

Purpose: record an external adoption canary against a non-Rust repository using
plain command benchmarks. This canary used a temporary clone and did not modify
the source repository.

## Canary Target

| Field | Value |
| --- | --- |
| Repository shape | TypeScript GitHub Action repository |
| Source repo | `H:\Code\Typescript\droid-action` |
| Canary clone | `C:\perfgate-canaries\droid-action-non-rust-command-20260513` |
| Source branch | `sz/99-fork-smoke-harness` |
| Source commit | `3f325c127dad1e3909e090f0447a5669fe023a9e` |
| perfgate source commit | `4bb12b0a8bf5a80692004affe62c371640a72cdc` |
| perfgate binary | `C:\perfgate-target\server-key-rotation\debug\perfgate.exe` |
| Platform | Windows x86_64 |

## Commands

The canary used the current workspace-built perfgate binary:

```bash
perfgate --version
```

Result:

```text
perfgate 0.17.0
```

The external repo was cloned into an isolated canary directory:

```bash
git clone --local --no-hardlinks H:/Code/Typescript/droid-action C:/perfgate-canaries/droid-action-non-rust-command-20260513
git rev-parse HEAD
```

Result:

```text
3f325c127dad1e3909e090f0447a5669fe023a9e
```

First contact:

```bash
perfgate doctor
```

Result:

```text
FAIL config             perfgate.toml not found; run `perfgate init` or pass --config
WARN benchmarks         skipped because config could not be loaded
WARN baselines          skipped because config could not be loaded
Summary: 1 failed, 2 warnings
```

Initialization:

```bash
perfgate init --ci github --profile standard
```

Result:

```text
Scanning ... for benchmarks...
No benchmarks discovered. The generated config will have no [[bench]] entries.
You can add them manually to perfgate.toml.
Wrote perfgate.toml
Wrote baselines\.gitkeep
Wrote .github/workflows/perfgate.yml
Wrote .perfgate\README.md

Next:
  1. Add at least one [[bench]] entry to perfgate.toml.
     Example:
       [[bench]]
       name = "my-command"
       command = ["cargo", "run", "--", "--help"]
  2. Run: perfgate check --config perfgate.toml --all
  3. Promote a trusted first baseline:
     perfgate baseline promote --config perfgate.toml --all
  4. Commit perfgate.toml, .github/workflows/perfgate.yml, baselines/.gitkeep, and .perfgate/README.md
```

Two non-Rust command benches were added manually:

```toml
[[bench]]
name = "droid-node-version"
command = ["node", "-e", "console.log(process.version)"]

[[bench]]
name = "droid-typescript-files"
command = ["powershell", "-NoProfile", "-Command", "$ErrorActionPreference='Stop'; Get-ChildItem -Path src,test -Recurse -File | Where-Object { $_.Extension -eq '.ts' } | Measure-Object | Select-Object -ExpandProperty Count"]
```

First check after adding the benchmarks:

```bash
perfgate check --config perfgate.toml --all
```

Result:

```text
warning: [droid-node-version] markdown template ignored for no-baseline bench
warning: [droid-node-version] no baseline found for bench 'droid-node-version', skipping comparison
warning: [droid-typescript-files] markdown template ignored for no-baseline bench
warning: [droid-typescript-files] no baseline found for bench 'droid-typescript-files', skipping comparison
```

Baseline promotion:

```bash
perfgate baseline promote --config perfgate.toml --all
```

Result:

```text
Promoted baseline for droid-node-version
  current: artifacts/perfgate\droid-node-version\run.json
  baseline: baselines\droid-node-version.json
Promoted baseline for droid-typescript-files
  current: artifacts/perfgate\droid-typescript-files\run.json
  baseline: baselines\droid-typescript-files.json

Promoted 2 baselines from perfgate.toml
```

Required-baseline rerun:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

Result:

```text
success with no warnings
```

Post-setup doctor:

```bash
perfgate doctor --config perfgate.toml
```

Result:

```text
OK   config             perfgate.toml found (2 benchmarks)
OK   benchmarks         2/2 commands runnable
OK   baselines          2/2 local baselines found
OK   artifact directory artifacts/perfgate writable
Summary: 0 failed, 0 warnings
```

## Generated Files

The generated and promoted files were:

```text
perfgate.toml
.github/workflows/perfgate.yml
.perfgate/README.md
baselines/.gitkeep
baselines/droid-node-version.json
baselines/droid-typescript-files.json
```

The transient artifacts were:

```text
artifacts/perfgate/droid-node-version/run.json
artifacts/perfgate/droid-node-version/compare.json
artifacts/perfgate/droid-node-version/report.json
artifacts/perfgate/droid-node-version/comment.md
artifacts/perfgate/droid-node-version/repair_context.json
artifacts/perfgate/droid-typescript-files/run.json
artifacts/perfgate/droid-typescript-files/compare.json
artifacts/perfgate/droid-typescript-files/report.json
artifacts/perfgate/droid-typescript-files/comment.md
artifacts/perfgate/droid-typescript-files/repair_context.json
```

## CI Wiring

`perfgate init --ci github --profile standard` generated a workflow using the
public action alias and required-baseline mode:

```yaml
uses: EffortlessMetrics/perfgate@v0
with:
  config: perfgate.toml
  all: "true"
  require_baseline: "true"
  upload_artifact: "true"
```

This canary did not push the temporary clone or run hosted CI.

## Observations

What worked:

- `doctor` and `init` worked without Rust-specific repository assumptions.
- Plain command benchmarks using `node` and PowerShell ran through check,
  baseline promotion, required-baseline rerun, and post-setup doctor.
- Multiple non-Rust benchmarks produced the same artifact and baseline layout
  as Rust workspace benchmarks.
- The required-baseline rerun succeeded without warnings once baselines were
  promoted.
- The generated workflow used the public action alias and did not require
  Rust-specific action inputs.

What was confusing:

- The zero-benchmark `init` example uses `cargo run -- --help` even in a
  non-Rust repository. The surrounding text is correct, but the first example
  should be language-neutral or show both command shapes.
- The generated `.perfgate/README.md` also starts with "add a `[[bench]]`"
  but does not show a non-Rust example.

## Follow-Up Decision

This canary supports the non-Rust command benchmark path and exposes a narrow
copy improvement:

```text
When init discovers zero benchmarks, the example bench should be
language-neutral or include a non-Rust command example.
```

Recommended follow-up PR:

```text
init: use language-neutral zero-benchmark example
```

Potential acceptance:

- `perfgate init` still writes `perfgate.toml`, workflow, baseline placeholder,
  and `.perfgate/README.md`.
- The zero-benchmark stdout example works in a generic repo without Cargo.
- `.perfgate/README.md` gives a non-Rust command example or clearly labels the
  Cargo example as optional.
- Existing Rust-friendly setup remains easy to discover.

## What This Canary Proves

- perfgate can be initialized and run in a non-Rust repository using plain
  command benchmarks.
- Non-Rust benches use the same durable setup files, transient artifact paths,
  baseline promotion flow, and required-baseline behavior as Rust benches.
- The first-hour local adoption path is not tied to Cargo once a benchmark
  command is configured.

## What This Canary Does Not Prove

- Hosted GitHub Action behavior in the external repo.
- Probe-backed structured decision behavior.
- Server ledger operations in an external team repo.
- Non-Windows shell command portability for the PowerShell example used in this
  local canary.
