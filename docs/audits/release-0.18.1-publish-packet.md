# v0.18.1 Publish Packet

Date: 2026-06-11

Purpose: define the canonical publishing command packet for v0.18.1. This packet
is non-mutating and does not publish crates, move aliases, tag versions, create a
GitHub release, or run public install smoke.

Scope: patch-release candidate for first-use trust hardening only.

## Release Boundary

This packet is a pre-publish artifact and does not trigger irreversible release
steps. Publish commands must only run after explicit release approval.

## Expected Crates

| Crate | Expected version | Expected URL after publish |
| --- | --- | --- |
| `perfgate-types` | `0.18.1` | `https://crates.io/crates/perfgate-types/0.18.1` |
| `perfgate` | `0.18.1` | `https://crates.io/crates/perfgate/0.18.1` |
| `perfgate-client` | `0.18.1` | `https://crates.io/crates/perfgate-client/0.18.1` |
| `perfgate-server` | `0.18.1` | `https://crates.io/crates/perfgate-server/0.18.1` |
| `perfgate-cli` | `0.18.1` | `https://crates.io/crates/perfgate-cli/0.18.1` |

## Publish Order

```text
perfgate-types
perfgate
perfgate-client
perfgate-server
perfgate-cli
```

## Publish Commands

```bash
cargo +1.95.0 publish -p perfgate-types --locked
cargo +1.95.0 info perfgate-types

cargo +1.95.0 publish -p perfgate --locked
cargo +1.95.0 info perfgate

cargo +1.95.0 publish -p perfgate-client --locked
cargo +1.95.0 info perfgate-client

cargo +1.95.0 publish -p perfgate-server --locked
cargo +1.95.0 info perfgate-server

cargo +1.95.0 publish -p perfgate-cli --locked
cargo +1.95.0 info perfgate-cli
```

For each `cargo info` result, confirm:

- the crate resolves from crates.io,
- version `0.18.1` is present,
- same-release dependencies are visible where expected.

## Non-Goals

- No canary rewrites.
- No hosted Action alias cutover until publication is complete.
- No public install smoke claims before public artifacts exist.

