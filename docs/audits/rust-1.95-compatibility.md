# Rust 1.95 Compatibility Audit

Date: 2026-05-12 UTC (2026-05-11 America/New_York local run)

Branch: `chore/rust-1.95-compatibility`

Base: `main` at `9092c66` (`docs(policy): map Rust 1.95 and 0.17.0 governance rollout`)

## Scope

This audit probes the current 0.16.0 codebase under Rust 1.95 before changing
declared MSRV, toolchain pins, CI pins, Clippy policy, or package versions.

No source, manifest, workflow, lockfile, schema, or generated-artifact changes
were required by this compatibility probe.

## Toolchain

| Tool | Version |
|------|---------|
| `rustc +1.95.0` | `rustc 1.95.0 (59807616e 2026-04-14)` |
| `cargo +1.95.0` | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| `rustfmt +1.95.0` | `rustfmt 1.9.0-stable (59807616e1 2026-04-14)` |

## Validation

The main compatibility commands passed:

| Command | Result |
|---------|--------|
| `cargo +1.95.0 fmt --all -- --check` | Pass |
| `cargo +1.95.0 check --workspace --all-targets --all-features --locked` | Pass |
| `cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings` | Pass |
| `cargo +1.95.0 test --workspace --all-targets --all-features --locked` | Pass |
| `cargo +1.95.0 run -p xtask -- ci` | Pass with target directories moved to `C:\perfgate-target-rust195` |

`git diff --check` is run after this audit file is added.

## Environment Note

The workspace drive `D:` did not have enough free space for generated build
artifacts during the audit. Direct Rust 1.95 commands used:

```powershell
$env:CARGO_TARGET_DIR = 'C:\perfgate-target-rust195'
```

`xtask ci` also needs:

```powershell
$env:PERFGATE_CI_TARGET_DIR = 'C:\perfgate-target-rust195'
```

An earlier `xtask ci` attempt with only the outer `CARGO_TARGET_DIR` moved
failed when the nested `xtask-self` target wrote to `D:\Code\Rust\perfgate\target`.
That failure was disk-space/PDB related, not a Rust 1.95 compatibility failure.

## Result

Current `main` is compatible with Rust 1.95. The next PR can raise the declared
MSRV/toolchain pins without bundling source compatibility fixes.
