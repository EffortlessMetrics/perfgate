# PERFGATE-PROP-0004: 0.18 release cutover

Status: proposed
Owner: perfgate maintainers
Created: 2026-05-14
Target milestone: 0.18.0
Linked specs: PERFGATE-SPEC-0005-release-proof-contract, PERFGATE-SPEC-0007-guided-adoption-contract, PERFGATE-SPEC-0003-performance-decision-contract
Linked ADRs: PERFGATE-ADR-0001-public-crates-are-contracts, PERFGATE-ADR-0002-receipts-first-performance-decisions
Linked plan:
Support/status impact: docs/status/PRODUCT_CLAIMS.md must distinguish public release proof from pre-release readiness and external canary evidence
Policy impact: no new policy rows by default; release must keep the five-crate public surface and existing policy ledgers authoritative

## Problem

perfgate has finished the source-of-truth, guided-adoption, wrapper-absorption,
adoption-readiness, and external-trust lanes. The repo now has external canary
receipts for a small Rust CLI, a larger Rust workspace, and a non-Rust command
benchmark. It also has signal calibration guidance, probe design guidance,
platform metric boundaries, action failure examples, server-ledger key rotation
smoke, and product-claim proof maps.

That makes the next release decision operational rather than conceptual:

```text
Is 0.18.0 ready to become the public default, or is it intentionally deferred?
```

The current verified public state on 2026-05-14 is:

- crates.io reports `perfgate-cli = "0.17.0"`;
- the latest GitHub release is `v0.17.0`;
- local release tags include `v0`, `v0.17`, and `v0.17.0`;
- no `v0.18*` tag is present in the checked local tag set.

Until a release operator cuts 0.18.0, docs must not imply that 0.18.0 is
published, installable from crates.io, or the target of action aliases.

## Users and surfaces

- CLI users need the public install path to match README and first-hour docs.
- GitHub Action users need `EffortlessMetrics/perfgate@v0`, `@v0.18`, and
  `@v0.18.0` semantics to be explicit before aliases move.
- Release operators need dependency order, dry-run proof, tag proof, release
  asset proof, and public install smoke in one durable trail.
- Maintainers need product claims to distinguish in-repo readiness, external
  canaries, hosted CI proof, release artifacts, and public install proof.
- Agents need repo files that answer which version is public, which release is
  deferred, which commands prove readiness, and which operator steps remain
  forbidden without explicit release approval.

## Success criteria

- The release lane has a PR-sized plan before versions, tags, crates, or action
  aliases move.
- Release readiness records whether 0.18.0 is cut or explicitly deferred.
- If 0.18.0 is cut, all five public crates are published in dependency order:
  `perfgate-types`, `perfgate`, `perfgate-client`, `perfgate-server`,
  `perfgate-cli`.
- If 0.18.0 is cut, `v0.18.0`, `v0.18`, and `v0` action/tag state is verified
  and recorded, with `v0` moved only after public smoke supports it.
- If 0.18.0 is deferred, docs and product claims continue to say `v0.17.0` is
  the latest public release.
- Public install smoke proves the user path from public artifacts, not only a
  workspace-built binary.
- Product claims link release proof and canary evidence without implying that
  canaries proved every hosted runner or repository shape.
- A publication or deferral closeout records what changed, what did not change,
  what remains unproven, and what should happen next.

## Proposed shape

Use a release cutover lane with explicit non-mutating preparation before any
operator action:

1. add a release cutover plan and active goal manifest;
2. prepare 0.18.0 versions and release notes;
3. run package-list and per-package publish dry-runs;
4. prove release archive or staged artifact smoke before publication;
5. prepare public docs while clearly marking release state;
6. publish crates only after explicit operator approval;
7. create tags, GitHub release, and action aliases only after publish proof;
8. run public install smoke from published artifacts;
9. close the release with a publication or deferral audit.

The lane should preserve the established release-proof contract rather than
inventing new release semantics. `docs/RELEASE_READINESS.md` remains the
canonical release snapshot; `docs/status/PRODUCT_CLAIMS.md` remains the claim
proof map; canary notes remain external adoption evidence.

## Alternatives considered

### Treat adoption readiness as release approval

Rejected. The 0.18 adoption-readiness and external-trust lanes prove user
paths and repo coherence. They do not publish crates, move tags, create a
GitHub release, or prove public install smoke.

### Move `v0` immediately after version prep

Rejected. `v0` is the default public action alias. It should move only after
the release proof says 0.18.0 is the intended default and public smoke has
passed.

### Publish first and document later

Rejected. perfgate's release model is proof-first. Release claims should be
backed by dry-runs, artifact smoke, public install smoke, product claims, and a
closeout trail.

### Keep 0.18 permanently deferred

Rejected as a default. Deferral is acceptable only when explicit and honest.
The external-trust lane exists to make 0.18.0 releasable; this lane decides
whether the proof is sufficient to cut it.

## Specs to create or update

No new behavior spec is required at lane start. This lane exercises existing
contracts:

- `PERFGATE-SPEC-0005-release-proof-contract`;
- `PERFGATE-SPEC-0007-guided-adoption-contract`;
- `PERFGATE-SPEC-0003-performance-decision-contract`;
- `PERFGATE-SPEC-0002-package-surface-boundary`.

Create or update a spec only if release proof exposes a behavior or proof
contract gap.

## Architecture decisions needed

No new ADR is required at lane start. The lane relies on existing decisions:

- public crates are contracts;
- receipts-first performance decisions.

Add an ADR only if the release changes public crate boundaries, receipt
semantics, or the local-first/server-optional architecture.

## Evidence plan

Release preparation PRs should run the docs/status gates:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Release candidate PRs should add the broader release proof:

```bash
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- publish-check --package-list
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli
```

Publication proof must include public install smoke from the published source:

```bash
perfgate --version
perfgate doctor
perfgate init --ci github --profile standard
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
perfgate check --config perfgate.toml --all --require-baseline
```

## Risks

- Moving `v0` can trigger public action consumers immediately.
- Alias tags can trigger release workflows; the 0.17 lane showed this must be
  handled deliberately.
- Dry-run proof can pass while public install smoke still fails because public
  artifacts, tags, or release assets disagree.
- Product claims can drift if they say 0.18.0 is public before publication.
- External canaries can be over-read as hosted CI proof; the closeout explicitly
  says they did not push external PRs or run hosted CI in those repos.

## Non-goals

- Do not add new benchmarking primitives.
- Do not reopen source-of-truth governance, guided adoption, wrapper
  absorption, or external-trust canaries.
- Do not collapse the five public crates.
- Do not publish crates from proposal or planning PRs.
- Do not create or move tags from proposal or planning PRs.
- Do not move action aliases without explicit release approval and public
  smoke proof.
- Do not make server-ledger mode part of local correctness.

## Exit criteria

This proposal is complete when:

- a release cutover plan exists;
- 0.18.0 is either cut with publication proof or explicitly deferred with no
  ambiguity;
- release readiness and product claims match the public state;
- public install smoke is recorded if publication happens;
- tag, GitHub release, and action alias state are recorded if they move;
- a closeout records what was published or deferred, what canaries proved, what
  remains unproven, and what should happen next.
