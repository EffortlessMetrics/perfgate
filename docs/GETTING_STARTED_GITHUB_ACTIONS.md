# Getting Started: GitHub Actions

This guide shows a minimal GitHub Actions setup for `perfgate` with:
- config-driven checks (`perfgate check`)
- PR artifact upload
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
    steps:
      - uses: actions/checkout@v4

      - name: Run perfgate
        id: perfgate
        uses: EffortlessMetrics/perfgate@main
        with:
          config: perfgate.toml
          all: "true"
          out_dir: artifacts/perfgate
          upload_artifact: "true"
```

Action outputs are available as:
- `${{ steps.perfgate.outputs.verdict }}`
- `${{ steps.perfgate.outputs.pass_count }}`
- `${{ steps.perfgate.outputs.warn_count }}`
- `${{ steps.perfgate.outputs.fail_count }}`
- `${{ steps.perfgate.outputs.bench_count }}`
- `${{ steps.perfgate.outputs.exit_code }}`

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
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Build perfgate
        run: cargo install --path crates/perfgate-cli --locked --force

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

## 4) Optional: comment markdown artifact

If you want a custom PR comment body:

```bash
perfgate check \
  --config perfgate.toml \
  --all \
  --out-dir artifacts/perfgate \
  --md-template .github/perfgate-comment.hbs
```

This writes `artifacts/perfgate/comment.md`.
