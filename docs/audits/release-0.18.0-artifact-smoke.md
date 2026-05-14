# v0.18.0 Staged Release Artifact Smoke

Date: 2026-05-14

Branch: `release/0-18-artifact-smoke`

Purpose: prove a release-like archive and first-hour flow before crates.io
publication. This is staged artifact proof only. It does not publish crates,
create tags, create a GitHub release, move action aliases, or prove public
install from crates.io or GitHub release assets.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Version under test | `0.18.0` |
| Target | `x86_64-pc-windows-msvc` |
| Staged archive | `%TEMP%\perfgate-0.18.0-artifact-smoke\perfgate-x86_64-pc-windows-msvc.zip` |
| Archive size | `11098367` bytes |
| Archive SHA256 | `9a3d1ebd8772f812da8c0dcc201cf327d70fa39b46f4eafb4382159d6a73d86d` |

## Build And Package Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 build --release --locked --target x86_64-pc-windows-msvc -p perfgate-cli` | Pass | Built the release `perfgate.exe` binary for the Windows release target. |
| Staged archive creation with `Compress-Archive` | Pass | Created `perfgate-x86_64-pc-windows-msvc.zip` containing `perfgate.exe`, matching the release workflow archive shape for Windows. |
| Archive unpack smoke | Pass | Expanded the staged archive, found `perfgate.exe`, verified `perfgate --version` reported `perfgate 0.18.0`, and verified `perfgate doctor --help` exited successfully. |

## First-Hour Smoke From Staged Binary

The unpacked staged binary was used for both smoke paths below.

| Path | Result | Evidence summary |
| --- | --- | --- |
| Zero-benchmark repository | Pass | `perfgate init --ci github --profile standard` created `perfgate.toml`, `.github/workflows/perfgate.yml`, and `.perfgate/README.md`; stderr reported no discovered benchmarks and showed the `your-benchmark-command` manual benchmark guidance. |
| Manual benchmark repository | Pass | Added a language-neutral PowerShell benchmark command after `init`, then ran `doctor`, `check --all`, missing-baseline `check --require-baseline`, `baseline status`, `baseline promote --all`, and final `check --require-baseline`. |
| Artifact assertions | Pass | Verified `baselines/manual-bench.json`, `artifacts/perfgate/manual-bench/run.json`, `compare.json`, `report.json`, and `comment.md` existed after promotion and rerun. |

## Non-Inferences

- This is not public install smoke.
- This does not prove crates.io `cargo install` or `cargo binstall` from public
  release assets.
- This does not prove Linux, macOS, or cross-target release archives.
- This does not create or move `v0.18.0`, `v0.18`, or `v0`.
- This does not create a GitHub release.
- This does not authorize publication.

## Follow-Up Boundary

The next non-irreversible release step is public documentation cutover that
continues to distinguish `0.18.0` readiness proof from public release state.
Public install smoke remains blocked until public crates and release assets
exist.
