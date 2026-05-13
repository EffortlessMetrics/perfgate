# PERFGATE-SPEC-0006: Policy ledger contracts

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: policy-ledger-contracts.v1
Product surface: policy ledgers, public surface, no-panic baseline, non-Rust file governance, product claims
CI surface: public-surface --strict, arch, policy check-no-panic-family, docs-source-check, product-claims-check
Schema impact: none
Action impact: none
Server impact: none
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked ADRs: PERFGATE-ADR-0001-public-crates-are-contracts, PERFGATE-ADR-0002-receipts-first-performance-decisions
Linked plan: plans/0.18.0/guided-adoption.md
Linked policy: policy/public_crates.txt, policy/absorbed_crates.txt, policy/clippy-lints.toml, policy/clippy-debt.toml, policy/clippy-exceptions.toml, policy/no-panic-allowlist.toml, policy/no-panic-baseline.toml, policy/non-rust-allowlist.toml, policy/generated-allowlist.toml, policy/executable-allowlist.toml, policy/workflow-allowlist.toml, policy/dependency-surface-allowlist.toml
Support/status impact: docs/status/PRODUCT_CLAIMS.md PG-CLAIM-0006 links this spec
Proof commands: cargo +1.95.0 run -p xtask -- policy check-no-panic-family; cargo +1.95.0 run -p xtask -- public-surface --strict; cargo +1.95.0 run -p xtask -- arch; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

perfgate uses policy files to govern public crates, absorbed crate
disposition, Clippy debt, panic-family debt, non-Rust files, generated files,
workflow files, executable bits, and dependency surface exceptions. Those files
are machine-readable controls, not supporting prose.

Without an explicit contract, future specs and docs can drift by copying policy
rows, inventing unmanaged exception lists, or making claims that are not backed
by the ledgers the automation actually reads.

## Behavior

Policy ledgers MUST own concrete reviewed exceptions, governed surfaces, and
generated baselines. Specs, status docs, plans, README content, and handoffs
MUST link to ledgers instead of copying their rows.

The following ownership rules apply:

| Truth | Source |
|-------|--------|
| Durable public crate surface | `policy/public_crates.txt` |
| Absorbed or compatibility crate disposition | `policy/absorbed_crates.txt` |
| Clippy lint policy, debt, and exceptions | `policy/clippy-*.toml` |
| Panic-family allowance and generated baseline | `policy/no-panic-*.toml` |
| Non-Rust governed files | `policy/non-rust-allowlist.toml` |
| Generated file policy | `policy/generated-allowlist.toml` |
| Executable bit policy | `policy/executable-allowlist.toml` |
| Workflow file policy | `policy/workflow-allowlist.toml` |
| Dependency surface policy | `policy/dependency-surface-allowlist.toml` |

Where a policy file records a reviewed exception, it SHOULD include owner,
reason, and review-after metadata when the ledger format supports those fields.
Generated baselines MUST be marked as generated or refresh-safe by the owning
tooling so reviewers can distinguish measured debt from hand-authored policy.

Policy checks MUST be deterministic from repository files. A policy gate MUST
NOT require chat history or unpublished release context to decide whether an
exception is allowed.

## Non-goals

- Do not define every policy row in this spec.
- Do not change policy file formats in this spec-only PR.
- Do not fail every missing owner/reason/review-after field until the
  corresponding ledger format and migration plan exist.
- Do not make advisory policy debt a release blocker unless the relevant
  policy file or CI gate already says it is blocking.

## Required evidence

The policy-ledger contract is proven by the existing policy and architecture
gates plus source-doc and claim-map checks:

```bash
cargo +1.95.0 run -p xtask -- policy check-no-panic-family
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## Acceptance examples

| Example | Result |
|---------|--------|
| A spec links to `policy/public_crates.txt` for the public crate list instead of copying every row. | Pass |
| A product claim says policy ledgers govern exceptions and links to this spec plus the policy files. | Pass |
| A README section lists a copied no-panic allowlist row as product truth. | Fail |
| A plan proposes a new permanent public crate without updating policy and ADR context. | Fail |
| A generated no-panic baseline refresh is hand-edited without the owning policy command. | Fail |

## Test mapping

- `cargo +1.95.0 run -p xtask -- policy check-no-panic-family` verifies the
  panic-family allowlist and generated baseline.
- `cargo +1.95.0 run -p xtask -- public-surface --strict` verifies the public
  and absorbed crate ledgers.
- `cargo +1.95.0 run -p xtask -- arch` verifies crate/module architecture
  boundaries.
- `cargo +1.95.0 run -p xtask -- product-claims-check` verifies product claims
  include support tiers, proof commands, linked evidence, and fresh spec links.
- `cargo +1.95.0 run -p xtask -- docs-source-check` verifies source-of-truth
  metadata and linked files.

## Implementation mapping

Policy files under `policy/` own the concrete rows. `xtask` owns machine checks.
Specs define the behavior contract. Product claims map user-facing support to
proof. Plans sequence migrations when a ledger needs stronger enforcement.

## CI proof

CI lanes that enforce policy MUST call the relevant `xtask` checks instead of
reimplementing ledger parsing in workflow YAML. Workflow summaries may describe
policy failures, but the repository policy files remain the governed source.

## Promotion rule

This spec remains accepted while the ledgers and product claims link to it. It
becomes implemented when the claim map links this spec and the current policy
gates pass. Future stricter ledger schema requirements should update this spec
or create a follow-on spec before enforcement.
