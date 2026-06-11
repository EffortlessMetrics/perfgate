# v0.18.1 Public Install Smoke

Date: 2026-06-11
Source commit: `10f28c4e2a00711858100ea892655fe33080de8b` (v0.18.1 release source)
GitHub release: https://github.com/EffortlessMetrics/perfgate/releases/tag/v0.18.1

## Summary

The public 0.18.1 install path passed from public artifacts after crates.io publication
and the `v0.18.1` GitHub release. The smoke installed `perfgate-cli` 0.18.1 into an
isolated temporary root, verified the binary reported `perfgate 0.18.1`, initialized a fresh
Git repository, exercised first-use setup, added a minimal smoke benchmark entry, promoted
an initial baseline, and reran the gate with `--require-baseline`.

## Commands And Results

```bash
cargo binstall perfgate-cli@0.18.1 --install-path D:/Temp/perfgate-public-smoke-0.18.1 --no-confirm --force --disable-telemetry
perfgate --version
perfgate doctor
perfgate init --ci github --profile standard --suggest-benches
perfgate doctor --config perfgate.toml
# append a minimal benchmark to make smoke runnable on a repo with no detected bench
@'[[bench]]
name = "command-smoke"
command = ["cmd", "/c", "echo", "benchmark"]
'@ | Add-Content perfgate.toml
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

Observed evidence:

- `cargo binstall` resolved `perfgate-cli@0.18.1` and installed the expected
  `perfgate.exe` binary from GitHub release assets.
- `perfgate --version` printed `perfgate 0.18.1`.
- Initial `perfgate doctor` reported `State: no_config`, then `State: setup_missing_benchmarks`
  after config scaffolding.
- After adding one smoke benchmark entry, `perfgate check --all` generated first-run artifacts
  and reported missing-baseline setup for the smoke benchmark.
- `perfgate baseline promote --all` wrote `baselines/command-smoke.json`.
- `perfgate check --all --require-baseline` completed with status output that included
  configured benchmark results and performance guidance (no crash or installation failure).

Generated artifact paths included:

```text
artifacts/perfgate/command-smoke/comment.md
artifacts/perfgate/command-smoke/compare.json
artifacts/perfgate/command-smoke/report.json
artifacts/perfgate/command-smoke/repair_context.json
artifacts/perfgate/command-smoke/run.json
baselines/command-smoke.json
```

## Notes

- This smoke used a minimal temporary command benchmark in an ephemeral workspace.
  It verifies the public install path and bootstrap guidance flow rather than production
  benchmark quality.
- The generated workflow reference path remained `EffortlessMetrics/perfgate@v0` from the
  scaffolded config.
- A first ultra-fast command can still report noisy guidance by design; noisy output should be
  treated as expected for smoke-only thresholds.

## What Not To Infer

- This smoke does not prove every platform archive manually; the release workflow performs
  archive matrix builds and smoke checks for the full release matrix.
- This smoke does not prove hosted external repository CI after publication.
- This smoke does not make the baseline server mode required for local correctness.
