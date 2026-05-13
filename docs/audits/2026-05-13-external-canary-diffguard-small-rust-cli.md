# External Canary: diffguard Small Rust CLI

Date: 2026-05-13

Status: observed

Linked proposal: [`PERFGATE-PROP-0003`](../proposals/PERFGATE-PROP-0003-external-adoption-canaries.md)

Linked specs:
[`PERFGATE-SPEC-0007`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md),
[`PERFGATE-SPEC-0004`](../specs/PERFGATE-SPEC-0004-user-devex-paved-road.md)

Purpose: record the first external adoption canary against a real Rust CLI
workspace. This canary used a temporary clone and did not modify the source
repository.

## Canary Target

| Field | Value |
| --- | --- |
| Repository shape | Small Rust CLI workspace |
| Source repo | `D:\Code\Rust\diffguard` |
| Canary clone | `C:\perfgate-canaries\diffguard-small-rust-cli-20260513` |
| Source branch | `feat/v0.2-enhancements-v2` |
| Source commit | `804b4ce41b65919fd78a1dd57e4131f14d8d596e` |
| perfgate source commit | `5c0f861bf13febbf22296da804a70c7e81c205af` |
| perfgate binary | `C:\perfgate-target\external-canary-cli\debug\perfgate.exe` |
| Platform | Windows x86_64 |

## Commands

The canary used the current workspace-built perfgate binary:

```bash
cargo +1.95.0 build -p perfgate-cli --locked
perfgate --version
```

Result:

```text
perfgate 0.17.0
```

The external repo was cloned into an isolated canary directory:

```bash
git clone --local --no-hardlinks D:/Code/Rust/diffguard C:/perfgate-canaries/diffguard-small-rust-cli-20260513
git rev-parse HEAD
```

Result:

```text
804b4ce41b65919fd78a1dd57e4131f14d8d596e
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
  1. Run: perfgate check --config perfgate.toml --all
  2. Promote a trusted first baseline:
     perfgate baseline promote --config perfgate.toml --all
  3. Commit perfgate.toml, .github/workflows/perfgate.yml, baselines/.gitkeep, and .perfgate/README.md
```

The generated config had no `[[bench]]` entries, so the suggested next command
failed:

```bash
perfgate check --config perfgate.toml --all
```

Result:

```text
error: no benchmarks defined in config file
```

To continue the canary, one manual CLI-help benchmark was added:

```toml
[[bench]]
name = "diffguard-help"
command = ["cargo", "+1.95.0", "run", "-q", "-p", "diffguard", "--", "--help"]
```

First check after adding the benchmark:

```bash
perfgate check --config perfgate.toml --all
```

Result:

```text
warning: [diffguard-help] markdown template ignored for no-baseline bench
warning: [diffguard-help] no baseline found for bench 'diffguard-help', skipping comparison
warning: [diffguard-help] high noise detected (CV > 30%): consider using `perfgate paired` for more reliable results
```

Baseline promotion:

```bash
perfgate baseline promote --config perfgate.toml --all
```

Result:

```text
Promoted baseline for diffguard-help
  current: artifacts/perfgate\diffguard-help\run.json
  baseline: baselines\diffguard-help.json

Promoted 1 baseline from perfgate.toml
```

Required-baseline rerun:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

Result:

```text
warning: [diffguard-help] high noise detected (CV > 30%): consider using `perfgate paired` for more reliable results
```

Post-setup doctor:

```bash
perfgate doctor --config perfgate.toml
```

Result:

```text
OK   config             perfgate.toml found (1 benchmark)
OK   benchmarks         1/1 command runnable
OK   baselines          1/1 local baseline found
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
baselines/diffguard-help.json
```

The transient artifacts were:

```text
artifacts/perfgate/diffguard-help/run.json
artifacts/perfgate/diffguard-help/compare.json
artifacts/perfgate/diffguard-help/report.json
artifacts/perfgate/diffguard-help/comment.md
artifacts/perfgate/diffguard-help/repair_context.json
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

- `doctor` clearly identified the missing config and pointed to `perfgate init`.
- `init` clearly listed generated files and the next first-hour commands.
- The generated `.perfgate/README.md` explained what to commit and identified
  `artifacts/perfgate/` as local/CI output.
- After a benchmark was added, the local check, baseline promotion, and
  required-baseline rerun worked.
- The post-setup doctor showed the repo was ready: one runnable benchmark and
  one local baseline.

What was confusing:

- `init` correctly said no benchmarks were discovered, but still printed
  `perfgate check --config perfgate.toml --all` as step one. That command then
  failed with `no benchmarks defined in config file`.
- A repo without Criterion or discoverable benchmark files needs a more direct
  next-step hint: add a `[[bench]]` entry before running `check --all`.
- The CLI-help benchmark was intentionally simple, but the result was noisy.
  The warning correctly suggested `perfgate paired`, which should be covered by
  the follow-up signal calibration guide.

## Follow-Up Decision

This canary supports the first-hour path after a benchmark exists, but it also
exposes a cold-repo friction point:

```text
When init discovers zero benchmarks, the generated next steps should make
"add a [[bench]] entry" the next command before check/promote.
```

Recommended follow-up PR:

```text
init: clarify zero-benchmark next steps
```

Potential acceptance:

- `perfgate init` still writes `perfgate.toml`, workflow, baseline placeholder,
  and `.perfgate/README.md`.
- When no benchmarks are discovered, stdout and `.perfgate/README.md` explain
  that the user must add a `[[bench]]` entry before `check --all`.
- `perfgate check --config perfgate.toml --all` may continue to fail for an
  empty benchmark list; the issue is the next-step copy, not the validation.

## What This Canary Proves

- A real Rust CLI workspace can reach a clean configured state with local
  receipts, a promoted baseline, required-baseline rerun, and generated GitHub
  Action wiring.
- The first-hour docs and generated setup files are directionally correct.
- Zero-discovery repos need sharper next-step copy before this path is truly
  boring for cold users.

## What This Canary Does Not Prove

- Hosted GitHub Action behavior in the external repo.
- Larger Rust workspace behavior.
- Non-Rust command benchmark behavior.
- Probe-backed structured decision behavior.
- Server ledger operations in an external team repo.
