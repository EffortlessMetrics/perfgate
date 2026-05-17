# 0.18.0 Pre-Release Readiness Handoff

Status: release-operator-gated
Owner: perfgate maintainers
Created: 2026-05-17
Milestone: 0.18.0
Linked proposal: [`PERFGATE-PROP-0004-0-18-release-cutover`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)
Linked specs: [`PERFGATE-SPEC-0005-release-proof-contract`](../specs/PERFGATE-SPEC-0005-release-proof-contract.md), [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Linked ADRs: [`PERFGATE-ADR-0001-public-crates-are-contracts`](../adr/PERFGATE-ADR-0001-public-crates-are-contracts.md), [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)
Linked policy: [`public_crates.txt`](../../policy/public_crates.txt), [`absorbed_crates.txt`](../../policy/absorbed_crates.txt)
Support/status impact: [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md), and [`CHANGELOG.md`](../../CHANGELOG.md) distinguish release-candidate readiness from public release state.
Proof commands: docs-check; doc-test; docs-source-check; product-claims-check; git diff --check

## Summary

The 0.18 release lane is ready at the operator boundary. Generated
release-adjacent queue items are resolved, restored post-SRP coverage is present
on current `main`, and the final proof after restored coverage is recorded.

This handoff does not close the release lane. `.codex/goals/active.toml` remains
active with `current_work_item = "release-operator-gated-publication"`.

## Evidence

Durable proof records:

- [`v0.18.0 Publish Readiness Proof`](../audits/release-0.18.0-publish-readiness.md)
- [`v0.18.0 Staged Release Artifact Smoke`](../audits/release-0.18.0-artifact-smoke.md)
- [`v0.18.0 Final Pre-Publish Proof`](../audits/release-0.18.0-final-prepublish-proof.md)
- [`v0.18.0 Restored Coverage Proof`](../audits/release-0.18.0-restored-coverage-proof.md)
- [`v0.18.0 Final Proof After Restored Coverage`](../audits/release-0.18.0-final-proof-after-restored-coverage.md)
- [`v0.18.0 Publish Packet`](../audits/release-0.18.0-publish-packet.md)

Resolved queue inputs:

- #480 restored the #473-#475 post-SRP coverage tranche onto current `main`.
- #481 refreshed generated public badge endpoint data.
- #477 refreshed generated nightly baselines and trend data.
- #482 reran the full final proof after #480, #481, and #477 landed.

The final proof after restored coverage passed fmt, workspace check, Clippy,
workspace tests, public-surface, arch, schema-compat, action-check,
docs-source-check, product-claims-check, docs-check, doc-test, and
`git diff --check`.

## Remaining Work

The remaining steps are intentionally operator-gated and irreversible:

1. Publish the five public crates in dependency order.
2. Verify crates.io publication for all five crates at `0.18.0`.
3. Create the exact `v0.18.0` GitHub release with assets and checksums.
4. Move `v0.18` only after release proof.
5. Decide whether `v0` should move to 0.18.0.
6. Run public install smoke from public artifacts.
7. Close publication with an audit and archived active goal.

## What Not To Infer

- `0.18.0` is not published to crates.io.
- `v0.18.0` does not exist as a release tag from this handoff.
- No GitHub release or release assets are created here.
- `v0.18` and `v0` are not moved here.
- Public install smoke from crates.io, cargo-binstall, or GitHub release assets
  has not run for 0.18.0.
- The active release goal is not archived.

## Next Operator Notes

Start from the publish packet and active goal:

- [`release-0.18.0-publish-packet.md`](../audits/release-0.18.0-publish-packet.md)
- [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)
- [`.codex/goals/active.toml`](../../.codex/goals/active.toml)

If a publish command fails, stop before tags, releases, aliases, or public smoke
and record the partial public state before repairing forward.
