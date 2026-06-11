# v0.18.1 Release Notes Draft

Date: 2026-06-11

Status: draft, unpublished

Purpose: prepare patch-release notes for v0.18.1 (first-use trust hardening).
This document does not imply publication or action alias movement.

## Public State

As of this draft:

- latest published release remains `v0.18.0`;
- crates.io latest public versions remain `0.18.0`;
- no `v0.18.1`, `v0.18`, or `v0` alias movement is performed by this PR;
- workspace version is prepared for `0.18.1` verification.

## Highlights

- Updated first-run and first-check failure messaging to make missing-baseline and
  missing-compare states easier to recover from.
- Improved check-artifact framing so users can identify `run.json`, `compare.json`,
  and `report.json` outputs earlier.
- Refreshed baseline bootstrap and promotion guidance for sparse or newly adopted
  repos.
- Added explicit no-panic release-trust posture (advisory/deferred at
  `240` policy issues) so release claims match proof.
- Refreshed release-readiness evidence and example references for the patch lane.

## Remaining Release Proof Still Required

Before this draft can become a publication closeout, the lane must still record:

- public release artifacts and tag movement (`v0.18.1`, `v0.18`, `v0`);
- public install smoke from public assets;
- artifact and action alias verification after publication.

## Non-Inferences

- This draft does not publish crates.
- This draft does not create or move tags.
- This draft does not create a GitHub release.
- This draft does not move `v0`, `v0.18`, or `v0.18.1`.
