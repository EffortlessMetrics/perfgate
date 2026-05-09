# Getting Started: GitHub Actions

This guide shows a minimal GitHub Actions setup for `perfgate` with:
- config-driven checks (`perfgate check`)
- PR artifact upload
- workflow branching via `--output-github`
- optional one-line use of the official `perfgate-action`

## 1) Generate the paved-road setup

Run this from the repository root:

```bash
perfgate init --ci github --profile standard
```

This creates:

- `perfgate.toml`
- `.github/workflows/perfgate.yml`
- `baselines/.gitkeep`
- `.perfgate/README.md`

The generated setup uses local in-repo baselines first. After a trusted local
run, promote each first baseline and commit it:

```bash
perfgate check --config perfgate.toml --all
perfgate baseline status --config perfgate.toml
perfgate baseline promote --config perfgate.toml --all
```

## 2) Repository layout

Expected files:
- `perfgate.toml`
- `baselines/<bench>.json` (or `defaults.baseline_pattern`)

Example `perfgate.toml`:

```toml
[defaults]
repeat = 7
warmup = 1
threshold = 0.20
warn_factor = 0.50
noise_threshold = 0.10
noise_policy = "warn"
out_dir = "artifacts/perfgate"
baseline_dir = "baselines"

[[bench]]
name = "api"
command = ["bash", "-lc", "cargo test -p mycrate --release -- --nocapture"]
```

## 3) Zero-config workflow with `perfgate-action`

Use the official composite action at the root of this repository:

```yaml
name: perfgate

on:
  pull_request:
  workflow_dispatch:

jobs:
  performance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run perfgate
        id: perfgate
        uses: EffortlessMetrics/perfgate@v0.15.1
        with:
          config: perfgate.toml
          all: "true"
          require_baseline: "true"
          upload_artifact: "true"
```

Omit `out_dir` to let the action use `[defaults].out_dir` from
`perfgate.toml`. Set `out_dir` only when the workflow should override the
config.

Use `@v0.15.1` when you want an exact patch pin. If you prefer a moving tag,
the published action aliases `@v0.15` and `@v0` now track the current
compatible release.

Action outputs are available as:
- `${{ steps.perfgate.outputs.verdict }}`
- `${{ steps.perfgate.outputs.pass_count }}`
- `${{ steps.perfgate.outputs.warn_count }}`
- `${{ steps.perfgate.outputs.fail_count }}`
- `${{ steps.perfgate.outputs.bench_count }}`
- `${{ steps.perfgate.outputs.exit_code }}`

When `perfgate check` exits nonzero, the action writes a local reproduction
command and the resolved artifact paths to the job log and step summary. With
`all: "true"`, inspect per-benchmark artifacts under
`artifacts/perfgate/<bench>/` unless `out_dir` or `[defaults].out_dir`
overrides the location.

To surface structured scenario/tradeoff decisions in the job summary and PR
comment, opt in to decision mode:

```yaml
      - name: Run perfgate
        id: perfgate
        uses: EffortlessMetrics/perfgate@v0.15.1
        with:
          config: perfgate.toml
          all: "true"
          require_baseline: "true"
          decision: "true"
          comment: "true"
```

Decision mode runs `perfgate decision evaluate --config perfgate.toml` after
`check`, writes `scenario.json`, `tradeoff.json`, and `decision.md` under the
resolved artifact directory, and appends `decision.md` to the GitHub step
summary. If `check` reports a policy failure with exit code `2`, the action
defers the final policy result to the decision receipt so an accepted tradeoff
can downgrade the result according to configured policy. Runtime errors and
`--fail-on-warn` failures still stop the action.

Use decision mode when `perfgate.toml` contains `[[scenario]]` weights,
`[[tradeoff]]` policy, or probe comparison evidence. The lower-level
`scenario evaluate`, `tradeoff evaluate`, and `md --tradeoff` commands remain
available for custom pipelines, but the action path should usually be the
single `decision: "true"` input.

If scenarios configure `probe_baseline`, `probe_current`, and `probe_compare`,
decision mode also creates the probe comparison receipt before evaluating the
scenario and tradeoff receipts.

When evidence is incomplete but otherwise supports a configured tradeoff,
`decision.md` marks the result as review required. The action still treats the
machine verdict as `warn`; reviewers should inspect the listed
`review_reasons` before treating the tradeoff as accepted.

## 4) Manual PR performance gate workflow

Create `.github/workflows/perfgate.yml`:

```yaml
name: perfgate

on:
  pull_request:
  workflow_dispatch:

jobs:
  performance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build perfgate
        run: cargo install perfgate-cli --locked

      - name: Run perfgate checks
        id: perfgate
        run: |
          perfgate check \
            --config perfgate.toml \
            --all \
            --out-dir artifacts/perfgate \
            --output-github

      - name: Upload perfgate artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: perfgate-artifacts
          path: artifacts/perfgate

      - name: Print verdict
        if: always()
        run: |
          echo "verdict=${{ steps.perfgate.outputs.verdict }}"
          echo "pass=${{ steps.perfgate.outputs.pass_count }}"
          echo "warn=${{ steps.perfgate.outputs.warn_count }}"
          echo "fail=${{ steps.perfgate.outputs.fail_count }}"
```

`--output-github` writes these outputs to `$GITHUB_OUTPUT`:
- `verdict`
- `pass_count`
- `warn_count`
- `fail_count`
- `bench_count`
- `exit_code`

## 5) Optional: comment markdown artifact

If you want a custom PR comment body:

```bash
perfgate check \
  --config perfgate.toml \
  --all \
  --out-dir artifacts/perfgate \
  --md-template .github/perfgate-comment.hbs
```

This writes `artifacts/perfgate/comment.md`.

## Common Pitfalls

**Warning: perfgate exits with code 2 on budget violations.** This is intentional
(exit code 2 = policy fail), but it means any subsequent steps in the same job will
be skipped unless you guard them. Steps that must always run -- especially artifact
uploads and verdict printing -- need `if: always()`:

```yaml
      - name: Upload perfgate artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: perfgate-artifacts
          path: artifacts/perfgate
```

Without `if: always()`, the upload step only runs on exit 0 (success). A budget
violation (exit 2) will cause artifacts to be silently lost, making failures much
harder to diagnose.

**Warning: artifact uploads default to `on_success`.** GitHub Actions'
`upload-artifact` action only runs when prior steps succeed. Since perfgate uses
non-zero exit codes to signal policy outcomes (not just errors), always add
`if: always()` to any artifact upload or reporting step that follows a perfgate
command.

**Warning: understand the exit code semantics.** perfgate uses three distinct
non-zero exit codes:
- **1** -- tool/runtime error (I/O failure, parse error, spawn failure)
- **2** -- policy fail (budget violated)
- **3** -- warn treated as failure (`--fail-on-warn`)

If you need to distinguish these in later steps, capture the exit code:

```yaml
      - name: Run perfgate checks
        id: perfgate
        run: |
          perfgate check --config perfgate.toml --all --output-github || true
        # The actual exit code is available via outputs.exit_code
```
