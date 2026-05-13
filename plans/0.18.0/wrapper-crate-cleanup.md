# perfgate 0.18.0 Wrapper Crate Cleanup Plan

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Current PR: refactor: absorb app and domain wrapper crates
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked specs: docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md; docs/specs/PERFGATE-SPEC-0006-policy-ledger-contracts.md
Linked ADRs: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Linked plan: plans/0.18.0/guided-adoption.md
Linked policy: policy/public_crates.txt; policy/absorbed_crates.txt
Support/status impact: PG-CLAIM-0004 remains the public-surface claim; no new product claim in this plan PR
Proof commands: cargo +1.95.0 run -p xtask -- public-surface --strict; cargo +1.95.0 run -p xtask -- arch; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check
Blocks: wrapper absorption implementation batches
Blocked by: none
Rollback: revert this plan and its links; no workspace manifests or crates change in this PR

## Goal

Plan the remaining compatibility wrapper absorption without changing crate
membership in the planning PR.

The governing rule remains:

```text
public crates are contracts
modules are architecture boundaries
no durable unpublished production crates
```

## Remaining Wrappers

The public surface remains exactly:

- `perfgate`
- `perfgate-cli`
- `perfgate-types`
- `perfgate-client`
- `perfgate-server`

The remaining non-public wrappers are transitional and listed in
[`policy/absorbed_crates.txt`](../../policy/absorbed_crates.txt):

| Wrapper | Owner path | Disposition | Batch |
|---------|------------|-------------|-------|
| `perfgate-render` | `perfgate::presentation::render` | deleted in presentation batch | presentation |
| `perfgate-export` | `perfgate::presentation::export` | deleted in presentation batch | presentation |
| `perfgate-sensor` | `perfgate::presentation::sensor` | deleted in presentation batch | presentation |
| `perfgate-adapters` | `perfgate::runtime` | deleted in runtime/integration batch | runtime/integration |
| `perfgate-github` | `perfgate::integrations::github` | deleted in runtime/integration batch | runtime/integration |
| `perfgate-app` | `perfgate::app` | deleted in app/domain batch | app/domain |
| `perfgate-domain` | `perfgate::domain` | deleted in app/domain batch | app/domain |
| `perfgate-paired` | `perfgate::domain::paired` | deleted in app/domain batch | app/domain |
| `perfgate-error` | `perfgate_types::error` | delete after all workspace deps use `perfgate-types` | contract-adjacent |
| `perfgate-api` | `perfgate_types::baseline_service`; `perfgate_server::CredentialSource` | delete after contract-adjacent imports are direct | contract-adjacent |

Private dev/test packages are not wrapper absorption targets in this lane:

- `perfgate-fake`
- `perfgate-selfbench`
- root `perfgate-tests`
- `xtask`

## Batch Sequence

### Batch 1: Presentation Wrappers

Target:

- `perfgate-render`
- `perfgate-export`
- `perfgate-sensor`

Required work:

- update any workspace imports to `perfgate::presentation::*`;
- delete wrapper crates and remove them from workspace members/dependencies;
- update `docs/WORKSPACE.md`, `docs/CRATE_SEAMS.md`, and
  `policy/absorbed_crates.txt`;
- refresh generated no-panic baseline if deleted examples change scan results.

### Batch 2: Runtime And Integration Wrappers

Target:

- `perfgate-adapters`
- `perfgate-github`

Required work:

- update callers to `perfgate::runtime` and `perfgate::integrations::github`;
- remove wrapper crates and dependency metadata;
- keep runtime/platform boundaries covered by `xtask arch`;
- refresh docs and policy ledgers in the same PR.

### Batch 3: App And Domain Wrappers

Target:

- `perfgate-app`
- `perfgate-domain`
- `perfgate-paired`

Required work:

- update CLI, examples, mutation docs, and migration docs to facade paths;
- keep paired benchmark CLI artifact names unchanged where they are user-facing
  output names rather than crate references;
- remove wrapper crates and dependency metadata;
- refresh generated policy baselines if examples disappear.

### Batch 4: Contract-Adjacent Wrappers

Target:

- `perfgate-error`
- `perfgate-api`

Required work:

- update internal imports to `perfgate_types::error` and
  `perfgate_types::baseline_service`;
- keep `perfgate-client`, `perfgate-server`, and `perfgate-types` as the
  external contract seams;
- remove wrapper crates only after `public-surface --strict` and `arch` prove no
  production dependency still relies on them.

## Non-goals

- Do not remove crates in this planning PR.
- Do not reduce the five public crates.
- Do not change release tags, crates.io state, or action aliases.
- Do not invent a durable unpublished production crate category.

## Acceptance

- The remaining wrapper set is named.
- Each wrapper has an owner path, disposition, and implementation batch.
- Dev/test packages are explicitly excluded from wrapper absorption.
- Proof commands and rollback are documented for future implementation PRs.

## Proof Commands

Each implementation batch must run:

```bash
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

If a batch deletes Rust source that changes panic-family scan results, it must
also run:

```bash
cargo +1.95.0 run -p xtask -- policy check-no-panic-family
```

Documentation-only updates to this plan run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## Rollback

Rollback for this planning PR is a straight revert. Rollback for future
implementation batches is batch-specific: restore the wrapper crate, workspace
membership, dependency metadata, policy row disposition, docs references, and
generated baselines changed in that batch.
