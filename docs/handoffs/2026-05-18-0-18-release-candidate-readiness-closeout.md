# 0.18.0 Release-Candidate Readiness Closeout

Status: release candidate ready; publication still operator-gated
Owner: perfgate maintainers
Created: 2026-05-18
Milestone: 0.18.0
Linked proposal: [`PERFGATE-PROP-0004-0-18-release-cutover`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)
Linked specs: [`PERFGATE-SPEC-0005-release-proof-contract`](../specs/PERFGATE-SPEC-0005-release-proof-contract.md), [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Linked ADRs: [`PERFGATE-ADR-0001-public-crates-are-contracts`](../adr/PERFGATE-ADR-0001-public-crates-are-contracts.md), [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)
Linked policy: [`public_crates.txt`](../../policy/public_crates.txt), [`absorbed_crates.txt`](../../policy/absorbed_crates.txt)
Support/status impact: [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md), and [`release-0.18.0-publish-packet.md`](../audits/release-0.18.0-publish-packet.md)
Proof commands: docs-check; doc-test; docs-source-check; product-claims-check; git diff --check

## Summary

The 0.18 release candidate is ready as an unreleased candidate. The repo now
records fresh proof after the `init.rs` extraction, tightened first-hour user
path guidance, current-main publish-packet instructions, install/action example
truth, and product-claim links to the latest proof.

This closeout does not close the publication lane. `.codex/goals/active.toml`
remains active with `current_work_item = "release-operator-gated-publication"`.
The only remaining release work is explicit release-operator execution.

## What Is Ready

- First-use docs teach the paved path: `doctor`, `init --suggest-benches`,
  `check`, `baseline promote`, `check --require-baseline`, then GitHub Action
  wiring.
- Benchmark suggestions are described as commented, reviewable candidates, not
  auto-selected policy.
- Setup and missing-baseline states remain distinct from regressions.
- Direction-aware metric semantics are documented so users can tell why higher
  throughput is good while higher latency is bad.
- The publish packet records the five-crate order, exact publish commands,
  registry verification commands, stop conditions, partial-publish handling,
  and tag/alias policy.
- Product claims link to the final proof after init extraction, install/action
  audit, canaries, tests, and support boundaries without claiming public
  `0.18.0` availability.

## Proof Records

- [`v0.18.0 Publish Readiness Proof`](../audits/release-0.18.0-publish-readiness.md)
- [`v0.18.0 Staged Release Artifact Smoke`](../audits/release-0.18.0-artifact-smoke.md)
- [`v0.18.0 Final Pre-Publish Proof`](../audits/release-0.18.0-final-prepublish-proof.md)
- [`v0.18.0 Restored Coverage Proof`](../audits/release-0.18.0-restored-coverage-proof.md)
- [`v0.18.0 Final Proof After Restored Coverage`](../audits/release-0.18.0-final-proof-after-restored-coverage.md)
- [`v0.18.0 Final Proof After Init Extraction`](../audits/release-0.18.0-final-proof-after-init-extraction.md)
- [`v0.18.0 Install And Action Example Audit`](../audits/release-0.18.0-install-action-example-audit.md)
- [`v0.18.0 Publish Packet`](../audits/release-0.18.0-publish-packet.md)

The final broad proof after init extraction passed fmt, workspace check,
Clippy, workspace tests, public-surface, arch, schema-compat, action-check,
docs-source-check, product-claims-check, docs-check, doc-test, and
`git diff --check`.

## Queue State

At closeout, no release-candidate PR is intentionally left open. Generated
baseline/trend refresh #494 was closed instead of merged because it would have
changed `main` after the final proof and publish-packet sync. A scheduled run
can regenerate that data after publication or after an explicitly refreshed
proof.

## Remaining Operator-Gated Work

1. Publish the five public crates in dependency order.
2. Verify crates.io publication for all five crates at `0.18.0`.
3. Create the exact `v0.18.0` GitHub release with assets and checksums.
4. Move `v0.18` only after release proof.
5. Decide whether `v0` should move to `0.18.0`.
6. Run public install smoke from public artifacts.
7. Close publication with a public-state audit and archived active goal.

## What Not To Infer

- `0.18.0` is not published to crates.io.
- `v0.18.0` does not exist as a release tag from this closeout.
- No GitHub release or release assets are created here.
- `v0.18` and `v0` are not moved here.
- Public install smoke from crates.io, cargo-binstall, or GitHub release assets
  has not run for `0.18.0`.
- The active release goal is not archived here.

## Next Operator Notes

Start from these files:

- [`release-0.18.0-publish-packet.md`](../audits/release-0.18.0-publish-packet.md)
- [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)
- [`.codex/goals/active.toml`](../../.codex/goals/active.toml)

If any publish command fails, stop before tags, releases, aliases, or public
smoke and record the partial public state before repairing forward.
