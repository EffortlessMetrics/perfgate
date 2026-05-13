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
| PG-CLAIM-0009 | perfgate supports a first-hour local adoption path. | supported | CLI, docs, artifacts | before-0.18.0-release |
| PG-CLAIM-0010 | perfgate supports staged adoption levels from local gate to team ledger. | supported | docs, CLI, action, server | before-0.18.0-release |
| PG-CLAIM-0011 | perfgate supports probe-backed tradeoff explanation. | supported | CLI, Rust helpers, receipts | next-probe-contract-change |
| PG-CLAIM-0012 | perfgate supports optional team decision-ledger operations. | supported | server, CLI, dashboard, docs | next-server-ledger-change |
| PG-CLAIM-0013 | perfgate documents platform-specific metric availability. | advisory | docs, CLI receipts | next-platform-metric-change |

## PG-CLAIM-0001: Reviewable performance decisions

Tier: supported
Surface: CLI, GitHub Action, local receipts
Linked docs: [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md)
Linked specs: [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
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
Linked specs: [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
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
Linked docs: [`BASELINE_SERVICE_DESIGN.md`](../BASELINE_SERVICE_DESIGN.md), [`GETTING_STARTED_BASELINE_SERVER.md`](../GETTING_STARTED_BASELINE_SERVER.md), [`DECISION_LEDGER_RUNBOOK.md`](../DECISION_LEDGER_RUNBOOK.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md)
Linked specs: [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
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
Linked docs: [`CRATE_SEAMS.md`](../CRATE_SEAMS.md), [`ARCHITECTURE.md`](../ARCHITECTURE.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`2026-05-13-wrapper-absorption-closeout.md`](../handoffs/2026-05-13-wrapper-absorption-closeout.md), [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md)
Linked specs: [`PERFGATE-SPEC-0002-package-surface-boundary`](../specs/PERFGATE-SPEC-0002-package-surface-boundary.md)
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
Linked specs: [`PERFGATE-SPEC-0005-release-proof-contract`](../specs/PERFGATE-SPEC-0005-release-proof-contract.md)
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
Linked specs: [`PERFGATE-SPEC-0006-policy-ledger-contracts`](../specs/PERFGATE-SPEC-0006-policy-ledger-contracts.md)
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
Linked specs: [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
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
Linked docs: [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`audits/release-0.17.0-publish-readiness.md`](../audits/release-0.17.0-publish-readiness.md), [`audits/release-0.17.0-publication-closeout.md`](../audits/release-0.17.0-publication-closeout.md), [`audits/release-0.18.0-cutover-decision.md`](../audits/release-0.18.0-cutover-decision.md)
Linked specs: [`PERFGATE-SPEC-0005-release-proof-contract`](../specs/PERFGATE-SPEC-0005-release-proof-contract.md)
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

## PG-CLAIM-0009: First-hour local adoption path

Tier: supported
Surface: CLI, docs, artifacts
Linked docs: [`FIRST_HOUR.md`](../FIRST_HOUR.md), [`ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md), [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`DEBUGGING_FIRST_CI_RUN.md`](../DEBUGGING_FIRST_CI_RUN.md), [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md)
Linked specs: [`PERFGATE-SPEC-0004-user-devex-paved-road`](../specs/PERFGATE-SPEC-0004-user-devex-paved-road.md), [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features first_run
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_first_run_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_first_run_e2e_tests.rs)
- [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs)
- [`cli_check_tests.rs`](../../crates/perfgate-cli/tests/cli_check_tests.rs)

Artifacts:

- `perfgate.toml`
- `.github/workflows/perfgate.yml`
- `baselines/`
- `.perfgate/README.md`
- `artifacts/perfgate/compare.json`

Review after: before-0.18.0-release

## PG-CLAIM-0010: Staged adoption levels

Tier: supported
Surface: docs, CLI, GitHub Action, server ledger
Linked docs: [`ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md), [`FIRST_HOUR.md`](../FIRST_HOUR.md), [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`DECISION_LEDGER_RUNBOOK.md`](../DECISION_LEDGER_RUNBOOK.md), [`examples/action-failure-summaries.md`](../examples/action-failure-summaries.md), [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md)
Linked specs: [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- action-check
```

Linked gates: docs-check, doc-test, docs-source-check, action-check

Artifacts:

- local gate receipts
- GitHub Action summary and uploaded artifacts
- `decision.md`
- `decision.index.json`
- optional server ledger records

Review after: before-0.18.0-release

## PG-CLAIM-0011: Probe-backed tradeoff explanation

Tier: supported
Surface: CLI, Rust helpers, probe receipts, decision receipts
Linked docs: [`PROBE_QUICKSTART.md`](../PROBE_QUICKSTART.md), [`PROBE_DESIGN_PATTERNS.md`](../PROBE_DESIGN_PATTERNS.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`examples/decision-outcomes.md`](../examples/decision-outcomes.md), [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md)
Linked specs: [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md), [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate --features probe probe_helper_jsonl_drives_tradeoff_decision_evidence
cargo +1.95.0 test -p perfgate-cli --all-features probe
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
```

Linked tests:

- [`probe.rs`](../../crates/perfgate/src/probe.rs)
- [`cli_probe_tests.rs`](../../crates/perfgate-cli/tests/cli_probe_tests.rs)
- [`cli_structured_decision_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_structured_decision_e2e_tests.rs)

Artifacts:

- `probes.json`
- `probe-compare.json`
- `scenario.json`
- `tradeoff.json`
- `decision.md`
- `decision-bundle.json`

Review after: next-probe-contract-change

## PG-CLAIM-0012: Optional team decision-ledger operations

Tier: supported
Surface: server, CLI, dashboard, audit exports
Linked docs: [`DECISION_LEDGER_RUNBOOK.md`](../DECISION_LEDGER_RUNBOOK.md), [`BASELINE_SERVICE_DESIGN.md`](../BASELINE_SERVICE_DESIGN.md), [`GETTING_STARTED_BASELINE_SERVER.md`](../GETTING_STARTED_BASELINE_SERVER.md), [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md)
Linked specs: [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md), [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 test -p perfgate-cli --all-features server_operations_smoke_path_memory --locked
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_server_tests.rs`](../../crates/perfgate-cli/tests/cli_server_tests.rs)
- [`cli_mock_server_tests.rs`](../../crates/perfgate-cli/tests/cli_mock_server_tests.rs)
- [`cli_help_snapshot_tests.rs`](../../crates/perfgate-cli/tests/cli_help_snapshot_tests.rs)

Artifacts:

- `perfgate.decision_record.v1`
- decision history/latest/export/prune/debt output
- audit JSONL exports
- `/health` and `/metrics`

Review after: next-server-ledger-change

## PG-CLAIM-0013: Platform metric availability

Tier: advisory
Surface: docs, CLI receipts, platform runner adapters
Linked docs: [`PLATFORM_SUPPORT.md`](PLATFORM_SUPPORT.md), [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`HOST_MISMATCH.md`](../HOST_MISMATCH.md)
Linked specs: [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md)
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- product-claims-check
```

Linked tests:

- [`runtime.rs`](../../crates/perfgate/src/app/runtime.rs)
- [`cli_cpu_time_tests.rs`](../../crates/perfgate-cli/tests/cli_cpu_time_tests.rs)
- [`cli_host_mismatch_tests.rs`](../../crates/perfgate-cli/tests/cli_host_mismatch_tests.rs)

Artifacts:

- `perfgate.run.v1` optional metric fields
- `perfgate.compare.v1` metric deltas when both sides contain a metric
- host fingerprints

Review after: next-platform-metric-change
