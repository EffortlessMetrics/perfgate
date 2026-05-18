# v0.18.0 Publication Closeout

Date: 2026-05-18
Source commit: `f4f40dc5374ef3f389ea530e373da1c3e573bfe8`
Operator: Codex local release shell using configured maintainer credentials
Milestone: 0.18.0
Linked proposal: [`PERFGATE-PROP-0004-0-18-release-cutover`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)
Linked spec: [`PERFGATE-SPEC-0005-release-proof-contract`](../specs/PERFGATE-SPEC-0005-release-proof-contract.md)
Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

## Summary

perfgate 0.18.0 is now publicly released. The five public crates are visible on
crates.io at `0.18.0`, the exact `v0.18.0` tag and GitHub release exist, release
assets and checksums are uploaded, `v0.18` and `v0` action aliases peel to the
same release commit, and public install smoke passed from public artifacts.

## Published Crates

Published in dependency order:

| Crate | Version | crates.io URL | Verification |
| --- | --- | --- | --- |
| `perfgate-types` | `0.18.0` | https://crates.io/crates/perfgate-types/0.18.0 | `cargo +1.95.0 info perfgate-types`; `cargo +1.95.0 search perfgate-types --limit 5` |
| `perfgate` | `0.18.0` | https://crates.io/crates/perfgate/0.18.0 | `cargo +1.95.0 info perfgate`; `cargo +1.95.0 search perfgate --limit 1` |
| `perfgate-client` | `0.18.0` | https://crates.io/crates/perfgate-client/0.18.0 | `cargo +1.95.0 info perfgate-client`; `cargo +1.95.0 search perfgate-client --limit 5` |
| `perfgate-server` | `0.18.0` | https://crates.io/crates/perfgate-server/0.18.0 | `cargo +1.95.0 info perfgate-server`; `cargo +1.95.0 search perfgate-server --limit 5` |
| `perfgate-cli` | `0.18.0` | https://crates.io/crates/perfgate-cli/0.18.0 | `cargo +1.95.0 info perfgate-cli`; `cargo +1.95.0 search perfgate-cli --limit 5` |

Each `cargo publish` command completed and Cargo reported the crate published
to registry `crates-io`. No partial-publish repair was needed.

## GitHub Release

GitHub release: https://github.com/EffortlessMetrics/perfgate/releases/tag/v0.18.0
Release workflow: https://github.com/EffortlessMetrics/perfgate/actions/runs/26016756334
Published at: 2026-05-18T06:23:24Z

The release workflow completed successfully. It built and packaged:

```text
perfgate-aarch64-apple-darwin.tar.gz
perfgate-aarch64-unknown-linux-gnu.tar.gz
perfgate-x86_64-apple-darwin.tar.gz
perfgate-x86_64-pc-windows-msvc.zip
perfgate-x86_64-unknown-linux-gnu.tar.gz
perfgate-x86_64-unknown-linux-musl.tar.gz
sha256sums.txt
```

Recorded checksums:

```text
95562fd23400e207969731fa918db2717fcd0943d0f9bd8bbed056d79a5f5ecc  perfgate-aarch64-apple-darwin.tar.gz
5f7c17f53e3260595da34379e8e700aeab1dca5e965a313ab53736900c0d84f9  perfgate-aarch64-unknown-linux-gnu.tar.gz
4bcfaa9a89bf1b2f99bf006209785cc09299a2faed4ea3c96a9ffca7b239a8f6  perfgate-x86_64-apple-darwin.tar.gz
596e068a1050c0bd9cb7693c5dcdca3a06a7e061b047353109651f8f09d27acb  perfgate-x86_64-unknown-linux-gnu.tar.gz
a18aad72b6fa04cd525c08a67e0200e8c0e9f03027016e67391aabed8fc3938e  perfgate-x86_64-unknown-linux-musl.tar.gz
f9e5996baf21a82de25bd5167add0f74b6e348a1288083d67668014ab8940e35  perfgate-x86_64-pc-windows-msvc.zip
```

## Tags And Action Aliases

All three action tags peel to the release commit
`f4f40dc5374ef3f389ea530e373da1c3e573bfe8`:

```text
v0.18.0^{} -> f4f40dc5374ef3f389ea530e373da1c3e573bfe8
v0.18^{}   -> f4f40dc5374ef3f389ea530e373da1c3e573bfe8
v0^{}      -> f4f40dc5374ef3f389ea530e373da1c3e573bfe8
```

The exact `v0.18.0` release workflow completed successfully. The alias-triggered
release workflows were intentionally cancelled after the exact release assets
were created:

```text
v0.18 alias workflow: https://github.com/EffortlessMetrics/perfgate/actions/runs/26017227706
v0 alias workflow:    https://github.com/EffortlessMetrics/perfgate/actions/runs/26017398157
```

## Public Install Smoke

Public install smoke is recorded in
[`release-0.18.0-public-install-smoke.md`](release-0.18.0-public-install-smoke.md).

Verified path:

```bash
cargo binstall perfgate-cli --version 0.18.0
perfgate --version
perfgate doctor
perfgate init --ci github --profile standard --suggest-benches
perfgate doctor --config perfgate.toml
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

The installed binary reported `perfgate 0.18.0`, generated the expected files,
used `EffortlessMetrics/perfgate@v0` in the workflow, wrote run/compare/report/
comment/repair-context artifacts, promoted a local baseline, and passed the
require-baseline check.

## Prior Proof Inputs

- [`v0.18.0 Publish Readiness Proof`](release-0.18.0-publish-readiness.md)
- [`v0.18.0 Staged Release Artifact Smoke`](release-0.18.0-artifact-smoke.md)
- [`v0.18.0 Final Pre-Publish Proof`](release-0.18.0-final-prepublish-proof.md)
- [`v0.18.0 Restored Coverage Proof`](release-0.18.0-restored-coverage-proof.md)
- [`v0.18.0 Final Proof After Restored Coverage`](release-0.18.0-final-proof-after-restored-coverage.md)
- [`v0.18.0 Final Proof After Init Extraction`](release-0.18.0-final-proof-after-init-extraction.md)
- [`v0.18.0 Install And Action Example Audit`](release-0.18.0-install-action-example-audit.md)
- [`v0.18.0 Release-Candidate Readiness Closeout`](../handoffs/2026-05-18-0-18-release-candidate-readiness-closeout.md)

## What Remains Unproven

- Hosted external canaries were not rerun from `v0.18.0` in this closeout.
- The public install smoke was run on Windows; other platform archives are
  covered by the release workflow's archive smoke, not by manual install smoke.
- Server ledger mode remains optional team history and is not required for
  local correctness.
- Docs.rs page rendering may lag crates.io publication.

## Next Recommended Lane

The next non-release lane should focus on adoption intelligence and signal
maturity: benchmark recipe selection, baseline maturity, signal doctor output,
decision example packs, canary freshness, server backup/restore drills, and
agent-operable repair context.

