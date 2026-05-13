# Product Claims

This file maps user-facing perfgate claims to support tiers and evidence. It is
the status proof map; it should link to specs, tests, policy ledgers, release
proof, or docs rather than duplicate their full contents.

Support tier definitions live in [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md).

## Claim Index

| Claim ID | Claim | Tier | Surface | Review after |
|----------|-------|------|---------|--------------|
| PG-CLAIM-0001 | perfgate supports reviewable performance decisions. | supported | CLI, action, receipts | before-0.18.0-release |
| PG-CLAIM-0002 | perfgate decision bundles are portable local-first evidence. | supported | CLI, receipts | next-decision-contract-change |
| PG-CLAIM-0003 | the server decision ledger is optional team-scale history, not a correctness prerequisite. | supported | server, CLI, receipts | before-0.18.0-release |
| PG-CLAIM-0004 | perfgate has five public crates as the durable public surface. | stable | crates, policy | next-public-surface-change |
| PG-CLAIM-0005 | Rust 1.95 is the governed MSRV for the current release lane. | stable | toolchain, CI, release | next-msrv-change |
| PG-CLAIM-0006 | policy ledgers govern reviewed exceptions and file surfaces. | supported | policy, CI | before-0.18.0-release |
| PG-CLAIM-0007 | the GitHub Action surfaces local reproduction for decision-enabled gates. | supported | action, CLI, artifacts | next-decision-contract-change |
| PG-CLAIM-0008 | release readiness is proven by the publish-order matrix, not by version bumps alone. | supported | release, crates, CI | next-release-candidate |

## PG-CLAIM-0001: Reviewable performance decisions

Tier: supported
Surface: CLI, GitHub Action, local receipts
Linked docs: [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md)
Linked specs: `PERFGATE-SPEC-0003-performance-decision-contract` planned
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_structured_decision_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_structured_decision_e2e_tests.rs)
- [`cli_performance_decision_example_tests.rs`](../../crates/perfgate-cli/tests/cli_performance_decision_example_tests.rs)
- [`cli_release_decision_proof_tests.rs`](../../crates/perfgate-cli/tests/cli_release_decision_proof_tests.rs)

Artifacts:

- `scenario.json`
- `tradeoff.json`
- `decision.md`
- `decision.index.json`
- `decision-bundle.json`

Review after: before-0.18.0-release

## PG-CLAIM-0002: Portable local-first decision bundles

Tier: supported
Surface: CLI, receipts, release/audit handoff
Linked docs: [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md)
Linked specs: `PERFGATE-SPEC-0003-performance-decision-contract` planned
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
```

Linked tests:

- [`cli_performance_decision_example_tests.rs`](../../crates/perfgate-cli/tests/cli_performance_decision_example_tests.rs)
- [`cli_help_snapshot_tests.rs`](../../crates/perfgate-cli/tests/cli_help_snapshot_tests.rs)

Artifacts:

- `perfgate.decision_index.v1`
- `perfgate.decision_bundle.v1`

Review after: next-decision-contract-change

## PG-CLAIM-0003: Optional server decision ledger

Tier: supported
Surface: baseline server, CLI, dashboard, receipts
Linked docs: [`BASELINE_SERVICE_DESIGN.md`](../BASELINE_SERVICE_DESIGN.md), [`GETTING_STARTED_BASELINE_SERVER.md`](../GETTING_STARTED_BASELINE_SERVER.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md)
Linked specs: `PERFGATE-SPEC-0003-performance-decision-contract` planned
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
```

Linked tests:

- [`cli_server_tests.rs`](../../crates/perfgate-cli/tests/cli_server_tests.rs)
- [`cli_mock_server_tests.rs`](../../crates/perfgate-cli/tests/cli_mock_server_tests.rs)

Artifacts:

- `perfgate.decision_record.v1`
- decision upload/history/latest/export/prune/debt responses
- server audit events

Review after: before-0.18.0-release

## PG-CLAIM-0004: Five-crate public surface

Tier: stable
Surface: crates, public API, release policy
Linked docs: [`CRATE_SEAMS.md`](../CRATE_SEAMS.md), [`ARCHITECTURE.md`](../ARCHITECTURE.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md)
Linked specs: `PERFGATE-SPEC-0002-package-surface-boundary` planned
Linked policy:

- [`policy/public_crates.txt`](../../policy/public_crates.txt)
- [`policy/absorbed_crates.txt`](../../policy/absorbed_crates.txt)

Proof commands:

```bash
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

Current public crates:

- `perfgate`
- `perfgate-cli`
- `perfgate-types`
- `perfgate-client`
- `perfgate-server`

Review after: next-public-surface-change

## PG-CLAIM-0005: Rust 1.95 governed MSRV

Tier: stable
Surface: toolchain, CI, release
Linked docs: [`development/RUST_1_95_ROLLOUT.md`](../development/RUST_1_95_ROLLOUT.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`audits/rust-1.95-compatibility.md`](../audits/rust-1.95-compatibility.md)
Linked specs: `PERFGATE-SPEC-0005-release-proof-contract` planned
Linked gates: docs-check, doc-test, public-surface --strict
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- public-surface --strict
```

Linked files:

- [`rust-toolchain.toml`](../../rust-toolchain.toml)
- [`Cargo.toml`](../../Cargo.toml)

Review after: next-msrv-change

## PG-CLAIM-0006: Policy-ledger governed exceptions

Tier: supported
Surface: policy files, CI, release readiness
Linked docs: [`POLICY_ALLOWLISTS.md`](../POLICY_ALLOWLISTS.md), [`CLIPPY_POLICY.md`](../CLIPPY_POLICY.md), [`NO_PANIC_POLICY.md`](../NO_PANIC_POLICY.md), [`FILE_POLICY.md`](../FILE_POLICY.md)
Linked specs: `PERFGATE-SPEC-0006-policy-ledger-contracts` planned
Linked policy:

- [`policy/clippy-lints.toml`](../../policy/clippy-lints.toml)
- [`policy/clippy-debt.toml`](../../policy/clippy-debt.toml)
- [`policy/clippy-exceptions.toml`](../../policy/clippy-exceptions.toml)
- [`policy/no-panic-allowlist.toml`](../../policy/no-panic-allowlist.toml)
- [`policy/no-panic-baseline.toml`](../../policy/no-panic-baseline.toml)
- [`policy/non-rust-allowlist.toml`](../../policy/non-rust-allowlist.toml)
- [`policy/generated-allowlist.toml`](../../policy/generated-allowlist.toml)
- [`policy/executable-allowlist.toml`](../../policy/executable-allowlist.toml)
- [`policy/workflow-allowlist.toml`](../../policy/workflow-allowlist.toml)
- [`policy/dependency-surface-allowlist.toml`](../../policy/dependency-surface-allowlist.toml)

Proof commands:

```bash
cargo +1.95.0 run -p xtask -- policy check-no-panic-family
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
```

Review after: before-0.18.0-release

## PG-CLAIM-0007: Action local reproduction for decisions

Tier: supported
Surface: GitHub Action, CLI, artifacts
Linked docs: [`GETTING_STARTED_GITHUB_ACTIONS.md`](../GETTING_STARTED_GITHUB_ACTIONS.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md)
Linked specs: `PERFGATE-SPEC-0003-performance-decision-contract` planned
Linked gates: action-check, doc-test
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- doc-test
```

Required user-facing evidence:

- local `perfgate decision evaluate --config perfgate.toml` reproduction command
- discovered decision artifacts in the action log
- review-required policy output for `warn`, `fail`, and `pass`

Review after: next-decision-contract-change

## PG-CLAIM-0008: Release-order publish proof

Tier: supported
Surface: release, crates, CI
Linked docs: [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`audits/release-0.17.0-publish-readiness.md`](../audits/release-0.17.0-publish-readiness.md)
Linked specs: `PERFGATE-SPEC-0005-release-proof-contract` planned
Linked gates: publish-check --package-list and per-package publish dry-runs
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- publish-check --package-list
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-types
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-client
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-server
cargo +1.95.0 run -p xtask -- publish-check --dry-run --package perfgate-cli
```

Review after: next-release-candidate
