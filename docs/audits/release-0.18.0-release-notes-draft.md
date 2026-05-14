# v0.18.0 Release Notes Draft

Date: 2026-05-14

Status: draft, unpublished

Linked proposal: [`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

Purpose: prepare release notes for the 0.18.0 release candidate. This document
does not mean 0.18.0 has been published, tagged, released, or made the default
GitHub Action alias.

## Public State

As of this draft:

- latest published release remains `v0.17.0`;
- crates.io latest remains `0.17.0`;
- no `v0.18.0`, `v0.18`, or `v0` alias movement is performed by this PR;
- the workspace version is prepared for `0.18.0` release validation.

## Highlights

- Added the source-of-truth governance stack for proposals, specs, ADRs, plans,
  active goals, product claims, policy ledgers, and handoffs.
- Added guided adoption docs and proof for first-hour setup, adoption levels,
  decision outcome examples, probe quickstart, action failure copy, and server
  ledger operations.
- Absorbed production compatibility wrappers while preserving the five public
  crates as the durable public surface.
- Added product-claim and source-doc checkers to keep release claims and linked
  source-of-truth artifacts reviewable.
- Added external canary receipts for a small Rust CLI, a larger Rust workspace,
  and a non-Rust command benchmark.
- Improved zero-benchmark `perfgate init` guidance, including a
  language-neutral benchmark example for non-Rust repositories.
- Added signal/noise calibration guidance, probe design patterns, platform
  metric support boundaries, and action failure archaeology examples.
- Extended server-ledger operations proof with API key create/list/rotate smoke
  while keeping the server optional for correctness.
- Added a release cutover proposal and plan so publishing, tags, GitHub release
  assets, action aliases, public install smoke, and closeout stay explicit.

## Release Proof Still Required

Before this draft can become a publication closeout, the release lane must
still record:

- publish dry-runs for all five public crates;
- release artifact smoke before publication;
- public documentation cutover that distinguishes ready from released;
- explicit release-operator approval before crates.io publish;
- tag, GitHub release, and action alias proof if publication happens;
- public install smoke from public artifacts;
- publication or deferral closeout.

## Non-Inferences

- This draft does not publish crates.
- This draft does not create or move tags.
- This draft does not create a GitHub release.
- This draft does not move `v0`, `v0.18`, or `v0.18.0`.
- This draft does not claim hosted external canary CI has run.
