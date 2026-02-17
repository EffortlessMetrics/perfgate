# Getting Started: GitLab CI

This guide shows a minimal GitLab CI setup for `perfgate` with:
- config-driven checks (`perfgate check`)
- saved artifacts for review in pipeline UI

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

## 2) Add `.gitlab-ci.yml`

```yaml
stages:
  - perf

perfgate:
  stage: perf
  image: rust:1.92
  script:
    - cargo install --path crates/perfgate-cli --force
    - perfgate check --config perfgate.toml --all --out-dir artifacts/perfgate
    - cat artifacts/perfgate/comment.md
  artifacts:
    when: always
    expire_in: 14 days
    paths:
      - artifacts/perfgate/
```

`perfgate check` exit codes map directly to CI job status:
- `0` pass (or warn when `--fail-on-warn` is not set)
- `2` budget failure
- `3` warn treated as failure (when `--fail-on-warn`)
- `1` tool/runtime errors

## 3) Optional strict warning policy

If you want warnings to fail the pipeline:

```yaml
script:
  - perfgate check --config perfgate.toml --all --fail-on-warn --out-dir artifacts/perfgate
```

## 4) Optional cockpit envelope output

For sensor ingestion flows:

```yaml
script:
  - perfgate check --config perfgate.toml --all --mode cockpit --out-dir artifacts/perfgate
```

This writes `artifacts/perfgate/report.json` as `sensor.report.v1`.
