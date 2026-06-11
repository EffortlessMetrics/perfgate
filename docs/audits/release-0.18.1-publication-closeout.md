# v0.18.1 Publication Closeout

Date: 2026-06-11
Source commit: `10f28c4e2a00711858100ea892655fe33080de8b`
Published release: `v0.18.1`
Published crates.io version: `0.18.1` for all public crates
Operator: public release operator through canonical `EffortlessMetrics/perfgate`

## Summary

perfgate 0.18.1 is published on crates.io and GitHub with the five allowed crates,
the exact `v0.18.1` tag and GitHub release assets, action alias tags, and public install
smoke evidence available from a public repository.

## Published Crates

Published in dependency order:

| Crate | Version | Crates.io URL | Verification |
| --- | --- | --- | --- |
| `perfgate-types` | `0.18.1` | https://crates.io/crates/perfgate-types/0.18.1 | `cargo +1.95.0 info perfgate-types` |
| `perfgate` | `0.18.1` | https://crates.io/crates/perfgate/0.18.1 | `cargo +1.95.0 info perfgate` |
| `perfgate-client` | `0.18.1` | https://crates.io/crates/perfgate-client/0.18.1 | `cargo +1.95.0 info perfgate-client` |
| `perfgate-server` | `0.18.1` | https://crates.io/crates/perfgate-server/0.18.1 | `cargo +1.95.0 info perfgate-server` |
| `perfgate-cli` | `0.18.1` | https://crates.io/crates/perfgate-cli/0.18.1 | `cargo +1.95.0 info perfgate-cli` |

## GitHub Release

GitHub release: https://github.com/EffortlessMetrics/perfgate/releases/tag/v0.18.1
Release workflow: https://github.com/EffortlessMetrics/perfgate/actions/runs/27332344546
Published at: 2026-06-11T08:04:05Z

The release workflow completed successfully and published these assets:

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
c20ab74914e9f08d91a734317fdf97548479787e949e73509e4e48ebe7a3dfca  perfgate-aarch64-apple-darwin.tar.gz
454e9acfcba109cbaad096a921312179a3da156ff7129091af20b4cf47b76a62  perfgate-aarch64-unknown-linux-gnu.tar.gz
cbcb1a2569a452b46566b9d76261aba8119b697e211a049e836d1fe75af4d3df  perfgate-x86_64-apple-darwin.tar.gz
9081941d77e7afd48dd24ca3a04fb98926d3a9623c3ac3c7bc950ddb5092cd60  perfgate-x86_64-unknown-linux-gnu.tar.gz
8676a522308df7263c9c21b28cba757a17cf2bb7fffa681cc14bf6be263a472f  perfgate-x86_64-unknown-linux-musl.tar.gz
4d8a8bc9aba5476b9ddfdb9bee91ffcf0311e65b6fd2930db1c1c945198d56ee  perfgate-x86_64-pc-windows-msvc.zip
```

## Tags And Action Aliases

All three action tags peel to the same release commit:

```text
v0.18.1^{} -> 10f28c4e2a00711858100ea892655fe33080de8b
v0.18^{}    -> 10f28c4e2a00711858100ea892655fe33080de8b
v0^{}       -> 10f28c4e2a00711858100ea892655fe33080de8b
```

The exact workflow for `v0.18.1` was `https://github.com/EffortlessMetrics/perfgate/actions/runs/27332332129`.
Alias-triggered release workflows were intentionally cancelled after the exact release artifact build:
https://github.com/EffortlessMetrics/perfgate/actions/runs/27332342290
https://github.com/EffortlessMetrics/perfgate/actions/runs/27332342394

## Public Install Smoke

Public install smoke is recorded in
[`release-0.18.1-public-install-smoke.md`](release-0.18.1-public-install-smoke.md).

## Prior Proof Inputs

- [v0.18.0 Publication Closeout](release-0.18.0-publication-closeout.md)
- [v0.18.0 Final Pre-Publish Proof](release-0.18.0-final-prepublish-proof.md)
- [v0.18.0 Final Proof After Init Extraction](release-0.18.0-final-proof-after-init-extraction.md)
- [v0.18.0 Publish Packet](release-0.18.0-publish-packet.md)
- [v0.18.0 Install And Action Example Audit](release-0.18.0-install-action-example-audit.md)

## What Remains Unproven

- Hosted external canaries were not rerun from `v0.18.1` in this packet.
- Action alias workflows were intentionally cancelled after exact-release completion.
- Server ledger mode remains optional and is not required for local correctness.
