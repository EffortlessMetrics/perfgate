# External Canary: shipper Larger Rust Workspace

Date: 2026-05-13

Status: observed

Linked proposal: [`PERFGATE-PROP-0003`](../proposals/PERFGATE-PROP-0003-external-adoption-canaries.md)

Linked specs:
[`PERFGATE-SPEC-0007`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md),
[`PERFGATE-SPEC-0004`](../specs/PERFGATE-SPEC-0004-user-devex-paved-road.md)

Purpose: record an external adoption canary against a larger Rust workspace.
This canary used a temporary clone and did not modify the source repository.

## Canary Target

| Field | Value |
| --- | --- |
| Repository shape | Larger Rust workspace |
| Source repo | `H:\Code\Rust\shipper` |
| Canary clone | `C:\perfgate-canaries\shipper-large-rust-workspace-20260513` |
| Source branch | `release/0.4.0-rc.1-readiness-proof` |
| Source commit | `472ee016028852c6270b731eaec0aad7b2b689e2` |
| perfgate source commit | `207fa95edde6e9880d744036e244fc15e957e87a` |
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
git clone --local --no-hardlinks H:/Code/Rust/shipper C:/perfgate-canaries/shipper-large-rust-workspace-20260513
git rev-parse HEAD
```

Result:

```text
472ee016028852c6270b731eaec0aad7b2b689e2
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

This confirmed the zero-benchmark follow-up from the first canary: the next
step is now to add a `[[bench]]` entry before running `check --all`.

Two workspace-level command benches were added manually:

```toml
[[bench]]
name = "shipper-rust-files"
command = ["powershell", "-NoProfile", "-Command", "$ErrorActionPreference='Stop'; (Get-ChildItem -Path crates -Recurse -Filter *.rs -File | Measure-Object).Count"]

[[bench]]
name = "shipper-workspace-members"
command = ["powershell", "-NoProfile", "-Command", "$ErrorActionPreference='Stop'; (Get-Content Cargo.toml | Select-String -Pattern 'crates/').Count"]
```

First check after adding the benchmarks:

```bash
perfgate check --config perfgate.toml --all
```

Result:

```text
warning: [shipper-rust-files] markdown template ignored for no-baseline bench
warning: [shipper-rust-files] no baseline found for bench 'shipper-rust-files', skipping comparison
warning: [shipper-rust-files] high noise detected (CV > 30%): consider using `perfgate paired` for more reliable results
warning: [shipper-workspace-members] markdown template ignored for no-baseline bench
warning: [shipper-workspace-members] no baseline found for bench 'shipper-workspace-members', skipping comparison
warning: [shipper-workspace-members] high noise detected (CV > 30%): consider using `perfgate paired` for more reliable results
```

Baseline promotion:

```bash
perfgate baseline promote --config perfgate.toml --all
```

Result:

```text
Promoted baseline for shipper-rust-files
  current: artifacts/perfgate\shipper-rust-files\run.json
  baseline: baselines\shipper-rust-files.json
Promoted baseline for shipper-workspace-members
  current: artifacts/perfgate\shipper-workspace-members\run.json
  baseline: baselines\shipper-workspace-members.json

Promoted 2 baselines from perfgate.toml
```

Required-baseline rerun:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

Result:

```text
warning: [shipper-rust-files] high noise detected (CV > 30%): consider using `perfgate paired` for more reliable results
warning: [shipper-workspace-members] high noise detected (CV > 30%): consider using `perfgate paired` for more reliable results
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
baselines/shipper-rust-files.json
baselines/shipper-workspace-members.json
```

The transient artifacts were:

```text
artifacts/perfgate/shipper-rust-files/run.json
artifacts/perfgate/shipper-rust-files/compare.json
artifacts/perfgate/shipper-rust-files/report.json
artifacts/perfgate/shipper-rust-files/comment.md
artifacts/perfgate/shipper-rust-files/repair_context.json
artifacts/perfgate/shipper-workspace-members/run.json
artifacts/perfgate/shipper-workspace-members/compare.json
artifacts/perfgate/shipper-workspace-members/report.json
artifacts/perfgate/shipper-workspace-members/comment.md
artifacts/perfgate/shipper-workspace-members/repair_context.json
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

- `doctor` pointed directly from missing config to `perfgate init`.
- `init` handled zero discovered benchmarks with the corrected add-a-benchmark
  next step.
- Multiple benchmarks produced separate artifact directories, baselines, and
  promotion output.
- `baseline promote --all` clearly mapped each current run receipt to its
  baseline file.
- `check --all --require-baseline` succeeded after promotion and preserved the
  same per-benchmark noise guidance.
- Post-setup `doctor` summarized the workspace state cleanly: two runnable
  commands and two local baselines.

What was confusing or operationally important:

- A compile-heavy canary command using `cargo +1.95.0 run -q -p shipper-cli -- --help`
  was not a good first-hour benchmark on this machine; it timed out after ten
  minutes while Cargo was already busy with other repository builds.
- The timeout was a canary finding, not a perfgate correctness failure: larger
  workspaces need quick first benches before they move to compile-heavy checks.
- The high-noise warnings were useful because the PowerShell startup-heavy
  commands are intentionally poor timing signals. The output pointed to paired
  mode instead of silently implying confidence.
- The generated `.perfgate/README.md` and stdout made it clear what files are
  durable setup files and which artifacts are transient outputs.

## Follow-Up Decision

No product change is required from this canary. It reinforces guidance already
captured by [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md): start with
short, stable commands; avoid compile-heavy first-hour gates unless the team is
ready for runner and cache variability; use paired mode or threshold tuning
when startup noise dominates.

## What This Canary Proves

- A larger Rust workspace can reach a clean configured state with multiple
  command benches, promoted baselines, required-baseline rerun, and generated
  GitHub Action wiring.
- The zero-benchmark init guidance now works for real repos without
  discoverable benchmark files.
- Per-benchmark artifacts and baseline promotion remain understandable when
  the workspace has more than one benchmark.
- Noisy command output points the user toward paired mode rather than hiding
  unstable timing.

## What This Canary Does Not Prove

- Hosted GitHub Action behavior in the external repo.
- Non-Rust command benchmark behavior.
- Probe-backed structured decision behavior.
- Server ledger operations in an external team repo.
- That compile-heavy workspace commands are suitable first-hour gates.
