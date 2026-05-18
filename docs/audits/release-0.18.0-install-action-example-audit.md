# v0.18.0 Install And Action Example Audit

Date: 2026-05-17

Branch: `docs/0-18-install-action-audit`

Purpose: verify that install and GitHub Action examples remain honest during
the 0.18.0 pre-publication release-candidate state. This audit checks that
public examples do not imply crates.io `0.18.0`, `v0.18.0`, `v0.18`, release
assets, moved `v0`, or public install smoke before those release-operator
steps happen.

Linked proposal:
[`PERFGATE-PROP-0004`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)

Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)

Linked proof:

- [`v0.18.0 Final Proof After Init Extraction`](release-0.18.0-final-proof-after-init-extraction.md)
- [`v0.18.0 Publish Packet`](release-0.18.0-publish-packet.md)

## Files Inspected

| Surface | Finding |
| --- | --- |
| `README.md` | Install examples use public installer/source install paths while explicitly stating the latest public release is `v0.17.0` and `0.18.0` is not public yet. |
| `docs/FIRST_HOUR.md` | First-hour install examples remain public-source examples and action guidance says `@v0.17.0`, `@v0.17`, and `@v0` are the current compatible public refs until the 0.18 publication closeout moves aliases. |
| `docs/ADOPTION_LEVELS.md` | Action-level guidance keeps the same `@v0.17.0` / `@v0.17` / `@v0` release-state wording. |
| `docs/GETTING_STARTED_GITHUB_ACTIONS.md` | Composite action examples pin `EffortlessMetrics/perfgate@v0.17.0` for exact public patch behavior. |
| `docs/PERFORMANCE_DECISIONS.md` | Decision-mode action example uses `@v0` and now says it tracks the current public compatible action release until the 0.18 publication closeout moves aliases. |
| `action.yml` | Optional `version` input example remains `0.17.0`; an empty version builds from the action source and does not claim crates.io `0.18.0` availability. |
| `docs/RELEASE_READINESS.md` | Current publication state still says `v0.17.0` is latest public and that no public `0.18.0` crates, tags, GitHub release, action aliases, or public install smoke exist yet. |

## Result

The install and action examples are aligned with the current release state:

- public install examples do not pin or claim `0.18.0`;
- exact action examples use `@v0.17.0`;
- moving action examples use `@v0` only with nearby wording that it follows the
  current public compatible release;
- `@v0.18` and `@v0.18.0` are not taught as public refs before release
  execution;
- public install smoke remains a blocked post-publication work item.

## Non-Inferences

- This audit does not publish crates.
- This audit does not create `v0.18.0`.
- This audit does not create a GitHub release or release assets.
- This audit does not move `v0.18` or `v0`.
- This audit does not prove public install from crates.io, cargo-binstall, or
  GitHub release assets.
- This audit does not close or archive the active release cutover goal.
