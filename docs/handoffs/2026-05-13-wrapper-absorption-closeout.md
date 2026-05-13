# Wrapper Absorption Closeout

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked specs: docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md; docs/specs/PERFGATE-SPEC-0006-policy-ledger-contracts.md
Linked ADRs: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Linked plan: plans/0.18.0/wrapper-crate-cleanup.md
Linked policy: policy/public_crates.txt; policy/absorbed_crates.txt; policy/no-panic-baseline.toml
Support/status impact: docs/status/PRODUCT_CLAIMS.md PG-CLAIM-0004
Proof commands: cargo +1.95.0 run -p xtask -- public-surface --strict; cargo +1.95.0 run -p xtask -- arch; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; git diff --check

## Summary

The wrapper absorption lane removed the remaining production compatibility
wrapper crates while preserving the five public crates:

- `perfgate`
- `perfgate-cli`
- `perfgate-types`
- `perfgate-client`
- `perfgate-server`

The implementation landed in four batches:

- presentation wrappers: `perfgate-render`, `perfgate-export`,
  `perfgate-sensor`;
- runtime and integration wrappers: `perfgate-adapters`, `perfgate-github`;
- app and domain wrappers: `perfgate-app`, `perfgate-domain`,
  `perfgate-paired`;
- contract-adjacent wrappers: `perfgate-error`, `perfgate-api`.

The remaining non-public workspace packages are private/dev/test/automation
packages only: `perfgate-fake`, `perfgate-selfbench`, root `perfgate-tests`,
and `xtask`.

## Product Surface

The user-facing CLI commands, receipt schemas, default artifact names, and
server/client/type contract crates were not renamed by this cleanup.

Notably:

- `perfgate paired` remains the paired benchmarking command;
- the default paired output remains `perfgate-paired.json`;
- `perfgate-types`, `perfgate-client`, and `perfgate-server` remain external
  contract seams;
- mutation aliases such as `perfgate-domain`, `perfgate-app`,
  `perfgate-paired`, `perfgate-error`, and `perfgate-api` remain available as
  logical targets where useful, but they no longer name workspace packages.

## Evidence

The implementation batches ran their applicable proof commands. The closeout
reruns the package-surface and source-of-truth proof:

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Source Of Truth

- `policy/public_crates.txt` owns the five public crates.
- `policy/absorbed_crates.txt` records the deleted wrapper dispositions.
- `docs/CRATE_SEAMS.md` explains the final package surface.
- `docs/WORKSPACE.md` is regenerated from `xtask docs-sync`.
- `plans/0.18.0/wrapper-crate-cleanup.md` records the implemented batch plan.
- `docs/status/PRODUCT_CLAIMS.md` maps the public-surface claim to proof.

## Remaining Work

No production compatibility wrapper remains as a durable package category.

Future package-surface changes must start from the package-surface spec, the
public-crates ADR, and the policy ledgers. They should not reduce or expand the
five public crates without an explicit proposal/spec/ADR/policy update.
