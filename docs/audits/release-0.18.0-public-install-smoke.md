# v0.18.0 Public Install Smoke

Date: 2026-05-18
Source commit: `f4f40dc5374ef3f389ea530e373da1c3e573bfe8`
GitHub release: https://github.com/EffortlessMetrics/perfgate/releases/tag/v0.18.0
Install source: public GitHub release asset resolved by `cargo binstall`

## Summary

The public 0.18.0 install path passed from public artifacts after crates.io
publication and the `v0.18.0` GitHub release. The smoke installed
`perfgate-cli` 0.18.0 into an isolated temporary root, verified the binary
reported `perfgate 0.18.0`, initialized a fresh Git repository, exercised the
first-hour setup path, promoted a baseline, and reran the gate with
`--require-baseline`.

This smoke used the public Windows release archive downloaded by
`cargo-binstall`; it did not use a workspace-built binary.

## Commands And Results

```bash
cargo binstall perfgate-cli --version 0.18.0 --install-path %TEMP%/perfgate-public-smoke-0.18.0-pass/bin -y --force --disable-telemetry
perfgate --version
perfgate doctor
perfgate init --ci github --profile standard --suggest-benches
perfgate doctor --config perfgate.toml
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

Observed evidence:

- `cargo binstall` resolved `perfgate-cli@=0.18.0` and downloaded the
  `x86_64-pc-windows-msvc` package from `github.com`.
- `perfgate --version` printed `perfgate 0.18.0`.
- Initial `perfgate doctor` reported `State: no_config` with the expected
  `perfgate init --ci github --profile standard` next step.
- `perfgate init --ci github --profile standard --suggest-benches` wrote
  `perfgate.toml`, `.github/workflows/perfgate.yml`, `baselines/.gitkeep`, and
  `.perfgate/README.md`.
- `doctor --config perfgate.toml` reported `State: benches_no_baselines` after
  a reviewed manual benchmark was added to the generated config.
- The first `check --all` wrote first-run artifacts and reported
  `missing_baseline` as setup, not as a regression.
- `baseline promote --all` wrote `baselines/command-smoke.json`.
- `check --all --require-baseline` exited `0`.
- The generated workflow referenced `EffortlessMetrics/perfgate@v0`.

Generated artifact paths included:

```text
artifacts/perfgate/command-smoke/run.json
artifacts/perfgate/command-smoke/compare.json
artifacts/perfgate/command-smoke/report.json
artifacts/perfgate/command-smoke/comment.md
artifacts/perfgate/command-smoke/repair_context.json
baselines/command-smoke.json
```

## Notes

The temporary smoke benchmark used a simple Windows command and deliberately
relaxed smoke-only threshold/noise settings so the test proved public install,
init, artifact, baseline, and require-baseline plumbing rather than CI host
benchmark quality. A first ultra-fast command also produced the expected
high-noise/regression guidance, which confirms the failure-copy path remains
visible when the workload is unsuitable.

## What Not To Infer

- This smoke does not prove every platform archive manually; the release
  workflow performed archive build and smoke checks for the release matrix.
- This smoke does not prove hosted external repository CI after publication.
- This smoke does not make server ledger mode required for local correctness.
- This smoke does not calibrate a production benchmark threshold.

