# Getting Started: GitHub Actions

This guide shows a minimal GitHub Actions setup for `perfgate` with:
- config-driven checks (`perfgate check`)
- PR artifact upload
- sticky PR comments via `artifacts/perfgate/comment.md`
- workflow branching via `--output-github`
- optional one-line use of the official `perfgate-action`

## 1) Repository layout

Expected files:
- `perfgate.toml`
- `baselines/<bench>.json` (or `defaults.baseline_pattern`)

Example `perfgate.toml`:

```toml
[defaults]
repeat = 5
warmup = 1
threshold = 0.20
warn_factor = 0.90
baseline_dir = "baselines"

[[bench]]
name = "api"
command = ["bash", "-lc", "cargo test -p mycrate --release -- --nocapture"]
```

## 2) Zero-config workflow with `perfgate-action`

Use the official composite action at the root of this repository:

```yaml
name: perfgate

on:
  pull_request:
  workflow_dispatch:

jobs:
  performance:
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
    steps:
      - uses: actions/checkout@v4

      - name: Run perfgate
        id: perfgate
        uses: EffortlessMetrics/perfgate@v0.15.1
        with:
          config: perfgate.toml
          all: "true"
          out_dir: artifacts/perfgate
          upload_artifact: "true"
          comment: "true"
```

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

When `comment: "true"` is enabled on pull requests, the action reuses
`artifacts/perfgate/comment.md` and upserts a single sticky PR comment by
attaching a hidden perfgate marker to the posted body.

## 3) Manual PR performance gate workflow

Create `.github/workflows/perfgate.yml`:

```yaml
name: perfgate

on:
  pull_request:
  workflow_dispatch:

jobs:
  performance:
    runs-on: ubuntu-latest
    permissions:
      pull-requests: write
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

      - name: Post sticky PR comment
        if: always() && github.event_name == 'pull_request'
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          set -euo pipefail

          if [ -f artifacts/perfgate/comment.md ]; then
            if ! perfgate comment \
              --body-file artifacts/perfgate/comment.md \
              --pr "${{ github.event.pull_request.number }}"; then
              echo "warning: failed to post perfgate PR comment" >&2
            fi
          else
            echo "No comment.md artifact found; skipping PR comment." >&2
          fi

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

## 4) Comment markdown artifact

If you want a custom PR comment body:

```bash
perfgate check \
  --config perfgate.toml \
  --all \
  --out-dir artifacts/perfgate \
  --md-template .github/perfgate-comment.hbs
```

This writes `artifacts/perfgate/comment.md`.

Post that artifact as a sticky GitHub PR comment:

```bash
perfgate comment --body-file artifacts/perfgate/comment.md --repo owner/repo --pr 123
```

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
