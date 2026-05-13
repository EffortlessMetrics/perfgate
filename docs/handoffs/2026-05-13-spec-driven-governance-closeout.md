# Handoff: Spec-driven Governance Closeout

Status: complete
Date: 2026-05-13
Owner: perfgate maintainers
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked specs: docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md, docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md, docs/specs/PERFGATE-SPEC-0003-performance-decision-contract.md, docs/specs/PERFGATE-SPEC-0004-user-devex-paved-road.md, docs/specs/PERFGATE-SPEC-0005-release-proof-contract.md
Linked ADRs: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md, docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md
Linked plan: plans/0.18.0/implementation-plan.md
Linked policy: policy/public_crates.txt, policy/absorbed_crates.txt, policy/clippy-*.toml, policy/no-panic-*.toml, policy/*-allowlist.toml
Support/status impact: docs/status/SUPPORT_TIERS.md, docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test

## Summary

The 0.18.0 spec-governance lane now has a linked source-of-truth stack:

- proposal for why the lane exists;
- specs for source-of-truth ownership, package surface, performance decisions,
  user DevEx, and release proof;
- ADRs for public-crate contracts and receipts-first decisions;
- support/status docs that map claims to tiers, evidence, and review cadence;
- an implementation plan that records the PR-sized sequence;
- an archived goal manifest at
  `.codex/goals/archive/perfgate-0-18-spec-driven-governance.toml`;
- machine checks for source-doc metadata and product-claim proof maps.

The lane did not change product behavior, package policy, schemas, workflows,
toolchain files, or release state except for the planned xtask enforcement PRs.

## Evidence

Final local proof from merged `main`:

```bash
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 clippy -p xtask --all-targets --all-features -- -D warnings
cargo +1.95.0 test -p xtask --all-features
```

Hosted proof:

- PRs 365 through 368 were marked ready only after reported checks were green,
  then merged.
- PRs 352, 353, and 354 were closed as superseded by PR 355.
- PR 355 remains the separate canonical RIPR/badge lane and was not combined
  with this source-of-truth lane.

## Remaining work

- `PERFGATE-SPEC-0006-policy-ledger-contracts` remains a planned follow-up if
  maintainers want a dedicated policy-ledger behavior contract.
- Full graph completeness, support-tier coverage enforcement, policy-ledger
  semantic validation, and every README claim mapping are intentionally not
  enforced by the first checker.
- No crates.io publish, tag, GitHub release, or badge/RIPR merge is implied by
  this closeout.

## Next operator notes

Start with:

- `docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md`
- `docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md`
- `plans/0.18.0/implementation-plan.md`
- `.codex/goals/archive/perfgate-0-18-spec-driven-governance.toml`
- `docs/status/PRODUCT_CLAIMS.md`

Use these checks before changing the stack:

```bash
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
```
