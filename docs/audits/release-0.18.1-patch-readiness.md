# v0.18.1 Patch Readiness Proof

Date: 2026-06-11

Branch: `main` on `perfgate-swarm`

Purpose:

Record the source-side proof for the 0.18.1 patch hardening lane before
promotion to the canonical publish repo.

## Scope

- First-run recovery and bootstrap guidance.
- Artifact layout and baseline bootstrap diagnostics.
- No-panic governance truthing posture.
- Action behavior and install/example hygiene.
- Publish-ready file-level/package scope checks prior to release authority migration.

## Proof Evidence

### Source-repo checks

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo run -p xtask -- pr` | Pass | PR validation command for release-lane source state passed. |
| `cargo run -p xtask -- docs-source-check` | Pass | Source-of-truth doc metadata and ID checks valid. |
| `cargo run -p xtask -- docs-check` | Pass | Documentation drift and link checks passed. |
| `cargo +1.95.0 run -p xtask -- policy check-no-panic-family` | Fail | `240 no-panic policy issue(s) found`; debt remains deferred. |
| `cargo run -p xtask -- product-claims-check` | Pass | Product claims map checks passed with deferred no-panic posture. |
| `cargo run -p xtask -- action-check` | Pass | GitHub Action wiring and reproduction command checks passed. |
| `cargo run -p xtask -- publish-check --package-list` | Pass | Five publishable crates are listed. |
| `cargo run -p xtask -- publish-check --dry-run --package perfgate-types` | Pass | Dry-run package verification passed. |
| `cargo run -p xtask -- publish-check --dry-run --package perfgate` | Pass | Dry-run package verification passed. |
| `cargo run -p xtask -- publish-check --dry-run --package perfgate-client` | Pass | Dry-run package verification passed. |
| `cargo run -p xtask -- publish-check --dry-run --package perfgate-server` | Pass | Dry-run package verification passed. |
| `cargo run -p xtask -- publish-check --dry-run --package perfgate-cli` | Pass | Dry-run package verification passed. |

## Governance posture

- `no-panic` remains advisory/deferred for this patch release and is explicitly
  called out in:
  - [`docs/RELEASE_READINESS.md`](../RELEASE_READINESS.md)
  - [`docs/status/PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md)
  - [`docs/audits/release-0.18.1-swarm-readiness.md`](release-0.18.1-swarm-readiness.md)

## Deferred / non-goals for this patch lane

- No public publication/tags/release artifacts are produced in swarm.
- No `review explain`, benchmark passport, baseline promote-plan, policy promote-plan,
  or hosted canary work is included.
- No `review` or `policy` automation for automatic gate promotion.
- Public install/action smoke is recorded only in canonical publish-lane or
  post-publication packets.

## Promotion note

This proof is a source-side handoff. Canonical publication authority remains
in `EffortlessMetrics/perfgate` and requires a normal merge commit from swarm
per `development/SWARM_PROMOTION.md`.
