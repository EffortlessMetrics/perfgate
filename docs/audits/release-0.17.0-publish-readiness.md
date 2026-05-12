# v0.17.0 Publish Readiness Proof

Date: 2026-05-12

Branch: `release/validate-0.17.0-readiness`

Purpose: validate that the v0.17.0 release candidate is ready for the separate
tag and publish step. This proof does not publish crates, create tags, or create
GitHub releases.

## Environment

| Item | Value |
| --- | --- |
| Rust toolchain | `cargo +1.95.0` |
| Target directory | `C:\perfgate-target-msrv` |
| Version under test | `0.17.0` |
| Publishable crates | `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`, `perfgate-cli` |

## Local Proof

| Command | Result | Evidence summary |
| --- | --- | --- |
| `cargo +1.95.0 run -p xtask -- docs-check` | Pass | Documentation drift check reported up to date. |
| `cargo +1.95.0 run -p xtask -- doc-test` | Pass | Checked 70 CLI examples and 36 structured snippets. |
| `cargo +1.95.0 run -p xtask -- action-check` | Pass | GitHub Action install, release asset, and failure diagnostic wiring aligned. |
| `cargo +1.95.0 run -p xtask -- public-surface --strict` | Pass | Public surface remains the five allowed packages. |
| `cargo +1.95.0 run -p xtask -- arch` | Pass | Architecture dependency rules held. |
| `cargo +1.95.0 run -p xtask -- schema-compat` | Pass | 18 historical schema fixtures deserialize with current types. |
| `cargo +1.95.0 run -p xtask -- publish-check --package-list` | Pass | File-list proof passed for all five publishable crates. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types` | Pass | Packaged, verified, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate` | Pass | Packaged, verified, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client` | Pass | Packaged, verified, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server` | Pass | Packaged, verified, and aborted upload because of dry run. |
| `cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli` | Pass | Packaged, verified, and aborted upload because of dry run. |

## Known Gaps

- Crates were not published.
- No `v0.17.0` tag was created.
- No GitHub release was created.
- Post-publish install proof, including `cargo-binstall perfgate-cli --version
  0.17.0 --force`, remains blocked until the crates and release assets exist.

## Release Boundary

The next irreversible step is tag and publish. Run it only after the release
proof PR is merged to `main` and the required hosted checks are green.
