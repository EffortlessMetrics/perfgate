# v0.18.0 Cutover Decision

Date: 2026-05-13

Status: deferred

Decision: do not cut, tag, publish, or move action aliases for `v0.18.0` in
this lane. Treat the 0.18 adoption-readiness work as pre-release proof until a
release operator explicitly starts the release PR.

## Current Public State

- Latest published release remains `v0.17.0`.
- The GitHub release `v0.17.0` is published and not marked prerelease.
- Workspace package versions remain `0.17.0`.
- No `v0.18*` tag exists in the local tag set checked for this decision.
- Action aliases remain owned by the published `v0.17.0` release state recorded
  in [`RELEASE_READINESS.md`](../RELEASE_READINESS.md).

## Why Defer

The repository has strong 0.18 readiness proof, but readiness is not the same as
publication. The recent work intentionally improved external trust and adoption
friction after the adoption-readiness snapshot:

- external adoption canary proposal;
- first real small-Rust-CLI canary;
- zero-benchmark init next-step fix from that canary;
- signal calibration guidance;
- probe design patterns;
- platform metric support boundaries.

Those changes make the eventual 0.18 release stronger, but they do not by
themselves authorize publishing crates, moving tags, creating a GitHub release,
or changing action aliases.

## Non-Actions In This Decision

This decision does not:

- change versions;
- create or move tags;
- publish crates;
- create a GitHub release;
- move `v0`, `v0.17`, or future `v0.18` action aliases;
- claim `0.18.0` is publicly installable.

## Release Start Criteria

Start the actual 0.18 release only when a release operator explicitly asks for
it. The release PR should then:

1. update versions and release notes;
2. run the full release proof matrix from
   [`RELEASE_READINESS.md`](../RELEASE_READINESS.md);
3. run package-list and per-package publish dry-runs for the five public crates;
4. publish in dependency order only after dry-runs pass;
5. create the exact release tag and GitHub release;
6. move action aliases intentionally;
7. run public install smoke from the published source;
8. add a publication closeout audit.

## Until Then

Docs should continue to describe `v0.17.0` as the latest published release.
0.18 artifacts should be described as readiness, adoption, or pre-release proof
unless and until the release PR publishes them.

