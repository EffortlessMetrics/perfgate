# v0.18.0 Publish Packet

Date: 2026-05-18

Branch: `main` after the release-candidate pointer-sync PR merges

Prepared from main state: through #495
(`c4a8e16afc33a38f4ee431d49bceeba6ca4bde65`) plus this
release-candidate pointer-sync PR.
The release operator must publish from the pulled `main` commit that contains
this packet, and must record that exact `git rev-parse HEAD` value in the
publication audit.

Purpose: give the release operator a single copy-ready packet for the first
irreversible 0.18.0 release step: publishing the five public crates. This packet
does not publish crates, create tags, create a GitHub release, move action
aliases, or prove public install.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

Linked proof:

- [`v0.18.0 Publish Readiness Proof`](release-0.18.0-publish-readiness.md)
- [`v0.18.0 Staged Release Artifact Smoke`](release-0.18.0-artifact-smoke.md)
- [`v0.18.0 Final Pre-Publish Proof`](release-0.18.0-final-prepublish-proof.md)
- [`v0.18.0 Restored Coverage Proof`](release-0.18.0-restored-coverage-proof.md)
- [`v0.18.0 Final Proof After Restored Coverage`](release-0.18.0-final-proof-after-restored-coverage.md)
- [`v0.18.0 Final Proof After Init Extraction`](release-0.18.0-final-proof-after-init-extraction.md)
- [`v0.18.0 Install And Action Example Audit`](release-0.18.0-install-action-example-audit.md)
- [`v0.18.0 Release-Candidate Readiness Closeout`](../handoffs/2026-05-18-0-18-release-candidate-readiness-closeout.md)

Current-main sync notes:

- #485 refreshed the broad release-candidate proof after #484 extracted
  `perfgate init` into `crates/perfgate-cli/src/init.rs`.
- #490 tightened the first-hour user path to show `doctor`,
  `init --suggest-benches`, local `check`, baseline promotion, and the
  CI-equivalent `check --require-baseline` confirmation.
- #492 audited install/action examples so public refs do not imply unpublished
  `0.18.0` crates, tags, aliases, assets, or public install smoke.
- #493 synced product claims to the final proof and readiness boundaries.
- #495 closed release-candidate readiness while leaving the publication lane
  active at release-operator-gated publication.
- No crates were published, no tags or releases were created, no aliases moved,
  and no public install smoke was claimed by these PRs.

## Release Boundary

This packet is non-mutating. It is not approval to publish.

Run the publish commands only after explicit release-operator approval. If any
publish command fails, stop immediately. Do not tag, create a GitHub release,
move aliases, or run public install smoke until the partial public state is
recorded and reconciled.

## Expected Crates

| Crate | Expected version | Expected URL after publish |
| --- | --- | --- |
| `perfgate-types` | `0.18.0` | `https://crates.io/crates/perfgate-types/0.18.0` |
| `perfgate` | `0.18.0` | `https://crates.io/crates/perfgate/0.18.0` |
| `perfgate-client` | `0.18.0` | `https://crates.io/crates/perfgate-client/0.18.0` |
| `perfgate-server` | `0.18.0` | `https://crates.io/crates/perfgate-server/0.18.0` |
| `perfgate-cli` | `0.18.0` | `https://crates.io/crates/perfgate-cli/0.18.0` |

## Publish Order

Publish in dependency order:

```text
perfgate-types
perfgate
perfgate-client
perfgate-server
perfgate-cli
```

Do not change this order in the release shell. Downstream crates depend on the
earlier crates being visible in the registry/index.

## Pre-Flight Check

Run these checks in the release shell immediately before publishing:

```bash
git switch main
git pull --ff-only
git status --short
git rev-parse HEAD
cargo +1.95.0 --version
cargo +1.95.0 run -p xtask -- publish-check --package-list
```

Stop if:

- `git status --short` prints any tracked or untracked release-relevant files.
- `git rev-parse HEAD` is not the pulled `main` commit that contains this
  packet and all intended release-candidate changes.
- any release-relevant code, public-surface, schema, action, version, or proof
  change landed after this packet without a reviewed refresh.
- `publish-check --package-list` does not list exactly the five expected public
  crates.

## Publish Commands

Run one command at a time. After each successful publish, verify that crates.io
can resolve the just-published version before continuing.

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
- version `0.18.0` is present,
- same-release dependencies point to `0.18.0` where expected.

## Publication Audit Fields

Record the publication result in the follow-up audit:

| Field | Required value |
| --- | --- |
| source commit | exact `git rev-parse HEAD` used for publishing |
| operator | person/account that ran the publish commands |
| crate name | one row per crate |
| version | `0.18.0` |
| crates.io URL | canonical crate version URL |
| published timestamp | UTC timestamp from the operator shell or registry |
| command result | pass/fail and any warning text |
| retry notes | only if a command was retried after registry reconciliation |

## Partial Publish Handling

If any crate publish fails:

1. Stop immediately.
2. Do not publish later crates.
3. Do not create `v0.18.0`.
4. Do not create a GitHub release.
5. Do not move `v0.18` or `v0`.
6. Do not run public install smoke as if the release completed.
7. Check registry truth with `cargo +1.95.0 info <crate>`.
8. Record the exact partial state in the publication audit or a repair audit.
9. Resume only from a reviewed repair plan.

If Cargo reports that a crate already exists, treat crates.io as the authority.
Verify with `cargo +1.95.0 info <crate>` before deciding whether the crate was
actually published.

## What Not To Do

- Do not add `--dry-run`; this packet is for the real publish step after
  approval.
- Do not add `--allow-dirty`.
- Do not publish from a branch that is not the intended release commit.
- Do not tag from this packet.
- Do not create or upload GitHub release assets from this packet.
- Do not move `v0.18` or `v0` from this packet.
- Do not archive `.codex/goals/active.toml`.

## Next Step After Success

After all five crates are visible on crates.io at `0.18.0`, open the follow-up
publication verification PR. It should record:

```bash
cargo +1.95.0 search perfgate-cli --limit 3
cargo +1.95.0 info perfgate-types
cargo +1.95.0 info perfgate
cargo +1.95.0 info perfgate-client
cargo +1.95.0 info perfgate-server
cargo +1.95.0 info perfgate-cli
```

Only after that verification should the release move to the `v0.18.0` tag,
GitHub release/assets, action aliases, public install smoke, and publication
closeout steps.
