# v0.18.0 Deferral Closeout

Date: 2026-05-14

Status: deferred

Purpose: close the 0.18 release cutover lane without ambiguity. The repository
has release-candidate proof for 0.18.0, but this closeout deliberately stops
before crates.io publication, tags, GitHub release assets, action alias
movement, and public install smoke because those steps require explicit
release-operator approval.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

Linked goal archive:
[`perfgate-0-18-release-cutover.toml`](../../.codex/goals/archive/perfgate-0-18-release-cutover.toml)

## Public State Verified

| Surface | State | Evidence |
| --- | --- | --- |
| crates.io | Latest public `perfgate-cli` is `0.17.0` | `cargo search perfgate-cli --limit 3` reported `perfgate-cli = "0.17.0"`. |
| GitHub release | Latest release is `v0.17.0` | `gh release list --limit 5` reported `v0.17.0` as latest, published 2026-05-12. |
| Exact 0.18 tags | Not present | `git ls-remote --tags origin "v0*"` listed no `v0.18` or `v0.18.0` refs. |
| Action aliases | Still on 0.17 release line | Remote `v0`, `v0.17`, and `v0.17.0` tags peel to `71bdc33117d515d95885deb2d9350d9d67905265`. |
| Workspace source | Prepared for 0.18.0 validation | `Cargo.toml` workspace version is `0.18.0`; this is source/release-candidate state, not public publication state. |

## Landed Cutover Work

| Area | Evidence |
| --- | --- |
| Release cutover proposal | PR #415 added the proposal and release criteria. |
| Release cutover plan | PR #416 added the implementation plan and active goal. |
| Version prep | PR #417 set workspace source to `0.18.0`, updated changelog state, and added the release notes draft. |
| Publish dry-run matrix | PR #418 recorded package-list and per-package dry-run proof for the five public crates. |
| Staged artifact smoke | PR #419 recorded Windows archive smoke, zero-benchmark guidance, manual benchmark check, baseline promotion, and required-baseline rerun from the unpacked binary. |
| Public docs cutover | PR #420 kept public install/action guidance on `v0.17.0` while linking 0.18 readiness proof and stating that 0.18 is not public. |

## Proof Commands

The lane recorded these proof commands:

```bash
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- publish-check --package-list
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli
cargo +1.95.0 build --release --locked --target x86_64-pc-windows-msvc -p perfgate-cli
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
git diff --check
```

The staged artifact smoke also ran `perfgate --version`, `perfgate doctor
--help`, `perfgate init --ci github --profile standard`, `perfgate doctor
--config perfgate.toml`, `perfgate check --config perfgate.toml --all`,
`perfgate baseline status --config perfgate.toml`, `perfgate baseline promote
--config perfgate.toml --all`, and `perfgate check --config perfgate.toml --all
--require-baseline` from the unpacked staged binary.

## Non-Actions

This closeout did not:

- publish crates;
- create `v0.18.0`;
- create or move `v0.18`;
- move `v0`;
- create a GitHub release;
- upload release assets;
- run public install smoke from crates.io or GitHub release assets;
- claim hosted external canary CI for 0.18.0.

## What To Do Next

When a release operator explicitly approves publication, start a new release
operator PR or release run from this proof trail:

1. re-run the publish dry-run matrix from
   [`release-0.18.0-publish-readiness.md`](release-0.18.0-publish-readiness.md);
2. publish in dependency order: `perfgate-types`, `perfgate`,
   `perfgate-client`, `perfgate-server`, `perfgate-cli`;
3. create `v0.18.0` and the GitHub release with assets;
4. move `v0.18`, and move `v0` only if 0.18.0 is intended as the default
   action release;
5. run public install smoke from public artifacts;
6. add a publication closeout that records crate URLs, tags, action aliases,
   release assets, public install smoke, and remaining non-inferences.
