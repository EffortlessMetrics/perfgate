# Product Claims

This file maps user-facing perfgate claims to support tiers and evidence. It is
the status proof map; it should link to specs, tests, policy ledgers, release
proof, or docs rather than duplicate their full contents.

Support tier definitions live in [`SUPPORT_TIERS.md`](SUPPORT_TIERS.md). Proof
freshness definitions live in [`PROOF_FRESHNESS.md`](PROOF_FRESHNESS.md).

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
| PG-CLAIM-0014 | perfgate reports first-use adoption state and benchmark suggestions. | supported | CLI, config, docs | next-onboarding-change |
| PG-CLAIM-0015 | perfgate explains first-use artifacts and repair classes. | supported | CLI, action, artifacts | next-repair-copy-change |
| PG-CLAIM-0016 | perfgate provides advisory calibration and decision-readiness suggestions. | advisory | CLI, receipts | next-signal-or-decision-change |
| PG-CLAIM-0017 | perfgate provides starter probe templates without requiring probes. | supported | CLI, docs, examples | next-probe-template-change |
| PG-CLAIM-0018 | perfgate reports optional ledger readiness without making the server required. | supported | CLI, server config | next-server-ledger-change |
| PG-CLAIM-0019 | perfgate has hosted external Action canary evidence for first-use failure UX. | advisory | GitHub Action, external canary | next-hosted-action-change |
| PG-CLAIM-0020 | perfgate interprets core metric improvement and regression using metric direction. | supported | domain, CLI, receipts, docs | next-decision-contract-change |
| PG-CLAIM-0021 | perfgate provides reviewable benchmark recipes with maturity metadata. | supported | CLI, docs, config | next-evidence-maturity-change |
| PG-CLAIM-0022 | perfgate reports baseline and signal maturity as advisory trust guidance. | advisory | CLI, receipts, docs | next-evidence-maturity-change |
| PG-CLAIM-0023 | perfgate can emit non-mutating calibration patch guidance. | advisory | CLI, config, receipts | next-signal-or-decision-change |
| PG-CLAIM-0024 | perfgate explains structured-decision suggestions with recognizable tradeoff examples. | supported | CLI, examples, receipts | next-decision-contract-change |
| PG-CLAIM-0025 | perfgate tracks adoption canary freshness without overclaiming one canary shape. | advisory | status docs, canaries | next-canary-refresh |
| PG-CLAIM-0026 | perfgate documents optional ledger backup, restore, retention, and migration expectations. | advisory | server docs, tests, status | next-server-ledger-change |
| PG-CLAIM-0027 | perfgate has fixture-backed agent repair-context guidance for common repair scenarios. | advisory | repair_context.json, CLI guidance, tests | next-agent-repair-contract-change |
| PG-CLAIM-0028 | perfgate supports advisory policy rollout profiles, promotion readiness, and non-mutating policy patches. | supported | CLI, docs, config | next-policy-ergonomics-change |
| PG-CLAIM-0029 | perfgate surfaces policy posture in review packets and Action summaries without changing configured behavior. | supported | CLI, action, artifacts | next-policy-ergonomics-change |
| PG-CLAIM-0030 | perfgate has fixture-backed agent policy guardrails for review-required policy changes. | advisory | CLI guidance, specs, tests | next-agent-policy-change |
| PG-CLAIM-0031 | perfgate imports existing benchmark outputs into receipts with explicit units, directions, sample model, and non-inferences. | supported | CLI, receipts, docs | next-evidence-intake-change |
| PG-CLAIM-0032 | perfgate surfaces imported-evidence limits in maturity, policy, review packet, and Action posture output. | supported | CLI, action, artifacts | next-evidence-intake-change |
| PG-CLAIM-0033 | perfgate provides reviewable adoption packs for common repo shapes without automatic policy changes. | supported | CLI, docs | next-evidence-intake-change |
| PG-CLAIM-0034 | perfgate can recommend and dry-run a first-use setup without mutating repository policy. | supported | CLI, docs, artifacts | next-first-useful-review-change |
| PG-CLAIM-0035 | perfgate can explain a first-use performance review with a benchmark passport and explicit non-inferences. | supported | CLI, docs, artifacts | next-first-useful-review-change |
| PG-CLAIM-0036 | perfgate emits agent-safe repair context and review guardrails without making agents policy authorities. | advisory | CLI, artifacts, tests | next-agent-repair-contract-change |
| PG-CLAIM-0037 | perfgate emits non-mutating baseline and policy promotion plans for reviewed graduation. | advisory | CLI, config guidance, tests | next-policy-promotion-change |

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
Linked docs: [`GETTING_STARTED_GITHUB_ACTIONS.md`](../GETTING_STARTED_GITHUB_ACTIONS.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`release-0.18.0-install-action-example-audit.md`](../audits/release-0.18.0-install-action-example-audit.md)
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
Linked docs: [`RELEASE_READINESS.md`](../RELEASE_READINESS.md), [`audits/release-0.17.0-publish-readiness.md`](../audits/release-0.17.0-publish-readiness.md), [`audits/release-0.17.0-publication-closeout.md`](../audits/release-0.17.0-publication-closeout.md), [`audits/release-0.18.0-cutover-decision.md`](../audits/release-0.18.0-cutover-decision.md), [`audits/release-0.18.0-publish-readiness.md`](../audits/release-0.18.0-publish-readiness.md), [`audits/release-0.18.0-final-prepublish-proof.md`](../audits/release-0.18.0-final-prepublish-proof.md), [`audits/release-0.18.0-restored-coverage-proof.md`](../audits/release-0.18.0-restored-coverage-proof.md), [`audits/release-0.18.0-final-proof-after-restored-coverage.md`](../audits/release-0.18.0-final-proof-after-restored-coverage.md), [`audits/release-0.18.0-final-proof-after-init-extraction.md`](../audits/release-0.18.0-final-proof-after-init-extraction.md), [`audits/release-0.18.0-publish-packet.md`](../audits/release-0.18.0-publish-packet.md), [`audits/release-0.18.0-install-action-example-audit.md`](../audits/release-0.18.0-install-action-example-audit.md), [`audits/release-0.18.0-artifact-smoke.md`](../audits/release-0.18.0-artifact-smoke.md), [`audits/release-0.18.0-public-install-smoke.md`](../audits/release-0.18.0-public-install-smoke.md), [`audits/release-0.18.0-publication-closeout.md`](../audits/release-0.18.0-publication-closeout.md)
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

Known limits:

- Alias-triggered release workflows for `v0.18` and `v0` were intentionally
  cancelled after the exact `v0.18.0` release workflow produced assets.
- Public install smoke was run on Windows from the public release asset path;
  hosted external repository canaries were not rerun from `v0.18.0` in the
  publication closeout.

Review after: next-release

## PG-CLAIM-0009: First-hour local adoption path

Tier: supported
Surface: CLI, docs, artifacts
Linked docs: [`FIRST_HOUR.md`](../FIRST_HOUR.md), [`ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md), [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`DEBUGGING_FIRST_CI_RUN.md`](../DEBUGGING_FIRST_CI_RUN.md), [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md), [`release-0.18.0-final-proof-after-init-extraction.md`](../audits/release-0.18.0-final-proof-after-init-extraction.md), [`release-0.18.0-install-action-example-audit.md`](../audits/release-0.18.0-install-action-example-audit.md), [`release-0.18.0-public-install-smoke.md`](../audits/release-0.18.0-public-install-smoke.md), [`release-0.18.0-publication-closeout.md`](../audits/release-0.18.0-publication-closeout.md), [`release-0.18.0-artifact-smoke.md`](../audits/release-0.18.0-artifact-smoke.md), [`2026-05-13-external-canary-diffguard-small-rust-cli.md`](../audits/2026-05-13-external-canary-diffguard-small-rust-cli.md), [`2026-05-13-external-canary-shipper-large-rust-workspace.md`](../audits/2026-05-13-external-canary-shipper-large-rust-workspace.md), [`2026-05-13-external-canary-droid-action-non-rust-command.md`](../audits/2026-05-13-external-canary-droid-action-non-rust-command.md), [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md)
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

Known limits:

- Public `0.18.0` install smoke proves the Windows public release asset path;
  external hosted canaries were not rerun from `v0.18.0` in the release
  closeout.

Review after: before-0.18.0-release

## PG-CLAIM-0010: Staged adoption levels

Tier: supported
Surface: docs, CLI, GitHub Action, server ledger
Linked docs: [`ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md), [`FIRST_HOUR.md`](../FIRST_HOUR.md), [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`DECISION_LEDGER_RUNBOOK.md`](../DECISION_LEDGER_RUNBOOK.md), [`examples/action-failure-summaries.md`](../examples/action-failure-summaries.md), [`release-0.18.0-adoption-readiness.md`](../audits/release-0.18.0-adoption-readiness.md), [`release-0.18.0-install-action-example-audit.md`](../audits/release-0.18.0-install-action-example-audit.md), [`release-0.18.0-artifact-smoke.md`](../audits/release-0.18.0-artifact-smoke.md), [`2026-05-13-external-canary-shipper-large-rust-workspace.md`](../audits/2026-05-13-external-canary-shipper-large-rust-workspace.md), [`2026-05-13-external-canary-droid-action-non-rust-command.md`](../audits/2026-05-13-external-canary-droid-action-non-rust-command.md), [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md)
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
- API key create/list/rotate output for ledger operations
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

## PG-CLAIM-0014: Adoption state and benchmark suggestions

Tier: supported
Surface: CLI, config, generated docs
Linked docs: [`FIRST_HOUR.md`](../FIRST_HOUR.md), [`ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md), [`release-0.18.0-final-proof-after-init-extraction.md`](../audits/release-0.18.0-final-proof-after-init-extraction.md), [`release-0.18.0-install-action-example-audit.md`](../audits/release-0.18.0-install-action-example-audit.md), [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md)
Linked specs: [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_doctor_tests.rs`](../../crates/perfgate-cli/tests/cli_doctor_tests.rs)
- [`cli_init_tests.rs`](../../crates/perfgate-cli/tests/cli_init_tests.rs)
- [`cli_first_run_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_first_run_e2e_tests.rs)

Artifacts:

- adoption-state doctor output
- `perfgate.toml` with reviewable suggested benches
- `.perfgate/README.md`

Review after: next-onboarding-change

## PG-CLAIM-0015: Artifact explanation and repair classes

Tier: supported
Surface: CLI, GitHub Action, artifacts
Linked docs: [`DEBUGGING_FIRST_CI_RUN.md`](../DEBUGGING_FIRST_CI_RUN.md), [`examples/action-failure-summaries.md`](../examples/action-failure-summaries.md), [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md)
Linked specs: [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features explain
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- action-check
```

Linked tests:

- [`cli_explain_tests.rs`](../../crates/perfgate-cli/tests/cli_explain_tests.rs)
- [`cli_check_tests.rs`](../../crates/perfgate-cli/tests/cli_check_tests.rs)
- [`xtask/src/main.rs`](../../xtask/src/main.rs)

Artifacts:

- `run.json`
- `compare.json`
- `report.json`
- `comment.md`
- `repair_context.json`
- GitHub Action failure summary

Review after: next-repair-copy-change

## PG-CLAIM-0016: Calibration and decision-readiness suggestions

Tier: advisory
Surface: CLI, receipts, decision guidance
Linked docs: [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md)
Linked specs: [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features calibrate
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
```

Linked tests:

- [`cli_calibrate_tests.rs`](../../crates/perfgate-cli/tests/cli_calibrate_tests.rs)
- [`cli_decision_suggest_tests.rs`](../../crates/perfgate-cli/tests/cli_decision_suggest_tests.rs)
- [`cli_structured_decision_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_structured_decision_e2e_tests.rs)

Artifacts:

- run receipts used for calibration suggestions
- advisory threshold/noise-policy output
- decision-readiness output
- decision receipt and bundle paths when evidence is ready

Review after: next-signal-or-decision-change

## PG-CLAIM-0017: Probe starter templates

Tier: supported
Surface: CLI, docs, examples
Linked docs: [`PROBE_QUICKSTART.md`](../PROBE_QUICKSTART.md), [`PROBE_DESIGN_PATTERNS.md`](../PROBE_DESIGN_PATTERNS.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md)
Linked specs: [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features probe
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_probe_tests.rs`](../../crates/perfgate-cli/tests/cli_probe_tests.rs)
- [`cli_help_snapshot_tests.rs`](../../crates/perfgate-cli/tests/cli_help_snapshot_tests.rs)

Artifacts:

- probe JSONL starter events
- scenario/tradeoff starter snippets
- `probes.json`
- `probe-compare.json`

Review after: next-probe-template-change

## PG-CLAIM-0018: Optional ledger readiness doctor

Tier: supported
Surface: CLI, server config, optional ledger
Linked docs: [`DECISION_LEDGER_RUNBOOK.md`](../DECISION_LEDGER_RUNBOOK.md), [`GETTING_STARTED_BASELINE_SERVER.md`](../GETTING_STARTED_BASELINE_SERVER.md), [`ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md)
Linked specs: [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features ledger
cargo +1.95.0 test -p perfgate-cli --all-features server
cargo +1.95.0 run -p xtask -- schema-compat
```

Linked tests:

- [`cli_server_tests.rs`](../../crates/perfgate-cli/tests/cli_server_tests.rs)
- [`cli_mock_server_tests.rs`](../../crates/perfgate-cli/tests/cli_mock_server_tests.rs)
- [`cli_help_snapshot_tests.rs`](../../crates/perfgate-cli/tests/cli_help_snapshot_tests.rs)

Artifacts:

- `perfgate ledger doctor` readiness output
- local receipt readiness status
- optional server URL/API key/project readiness status
- history/export/prune readiness checks

Review after: next-server-ledger-change

## PG-CLAIM-0019: Hosted external Action canary evidence

Tier: advisory
Surface: GitHub Action, external canary, artifacts
Linked docs: [`2026-05-15-hosted-external-action-canary-droid-action.md`](../audits/2026-05-15-hosted-external-action-canary-droid-action.md), [`DEBUGGING_FIRST_CI_RUN.md`](../DEBUGGING_FIRST_CI_RUN.md), [`examples/action-failure-summaries.md`](../examples/action-failure-summaries.md)
Linked specs: [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md)
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 test -p xtask action_check
```

Linked gates: action-check, hosted external canary rerun

Artifacts:

- external PR `EffortlessSteven/droid-action#7`
- hosted run `25941883937`, attempt 2
- uploaded artifact `perfgate-artifacts-25941883937-2`
- local reproduction line from the action summary

Review after: next-hosted-action-change

## PG-CLAIM-0020: Direction-aware metric movement

Tier: supported
Surface: domain, CLI, compare/probe/tradeoff receipts, decision guidance
Linked docs: [`DESIGN.md`](../DESIGN.md), [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`metric-direction-semantics.md`](../audits/metric-direction-semantics.md)
Linked specs: [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md), [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate --all-features domain::movement
cargo +1.95.0 test -p perfgate --all-features app::tradeoff
cargo +1.95.0 test -p perfgate --all-features trend_indicator
cargo +1.95.0 test -p perfgate --all-features trend_direction
cargo +1.95.0 test -p perfgate --all-features compare_regression_pct
cargo +1.95.0 test -p perfgate --all-features build_report_normalizes_higher_is_better_regression
cargo +1.95.0 test -p perfgate-cli --test cli_decision_suggest_tests
cargo +1.95.0 run -p xtask -- product-claims-check
```

Linked tests:

- [`movement.rs`](../../crates/perfgate/src/domain/movement.rs)
- [`comparison.rs`](../../crates/perfgate/src/domain/comparison.rs)
- [`probe.rs`](../../crates/perfgate/src/app/probe.rs)
- [`tradeoff.rs`](../../crates/perfgate/src/app/tradeoff.rs)
- [`check.rs`](../../crates/perfgate/src/app/check.rs)
- [`export.rs`](../../crates/perfgate/src/app/export.rs)
- [`watch.rs`](../../crates/perfgate/src/app/watch.rs)
- [`comment.rs`](../../crates/perfgate/src/integrations/github/comment.rs)
- [`cli_decision_suggest_tests.rs`](../../crates/perfgate-cli/tests/cli_decision_suggest_tests.rs)
- [`cli_tradeoff_tests.rs`](../../crates/perfgate-cli/tests/cli_tradeoff_tests.rs)

Artifacts:

- `perfgate.compare.v1` deltas with signed `pct`, normalized `regression`, and status
- `perfgate.probe_compare.v1` probe deltas using metric direction or probe metric heuristics
- `perfgate.tradeoff.v1` requirements and allowances evaluated through direction-aware improvement/regression semantics
- decision readiness output for lower-is-better and higher-is-better movement
- watch trends, GitHub comment movement labels, and export `regression_pct` using direction-aware judgment fields

Known limits:

- Raw signed `pct` remains a display field; callers must not treat its sign as judgment without metric direction.

Review after: next-decision-contract-change

## PG-CLAIM-0021: Benchmark recipes with maturity metadata

Tier: supported
Proof freshness: current
Surface: CLI, docs, generated config comments
Linked docs: [`BENCHMARK_RECIPES.md`](../BENCHMARK_RECIPES.md), [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_init_tests.rs`](../../crates/perfgate-cli/tests/cli_init_tests.rs)
- [`init.rs`](../../crates/perfgate-cli/src/init.rs)

Artifacts:

- `perfgate init --suggest-benches` generated recipe comments
- recipe metadata for best fit, bad fit, expected noise, recommended mode,
  advisory/blocking posture, and paired-mode hints

Known limits:

- Recipes are suggestions, not automatic benchmark selection.
- Generated recipes do not silently mark a benchmark as gate-ready.

Review after: next-evidence-maturity-change

## PG-CLAIM-0022: Baseline and signal maturity guidance

Tier: advisory
Proof freshness: current
Surface: CLI, receipts, docs
Linked docs: [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 run -p xtask -- docs-source-check
```

Linked tests:

- [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs)
- [`cli_doctor_tests.rs`](../../crates/perfgate-cli/tests/cli_doctor_tests.rs)

Artifacts:

- `perfgate baseline doctor` maturity classifications
- `perfgate doctor signal` sample/noise/host/baseline recommendations

Known limits:

- Maturity output is advisory; it does not mutate policy, promote baselines, or
  make noisy checks blocking by itself.

Review after: next-evidence-maturity-change

## PG-CLAIM-0023: Non-mutating calibration patch guidance

Tier: advisory
Proof freshness: current
Surface: CLI, config, receipts
Linked docs: [`SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md), [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features calibrate
```

Linked tests:

- [`cli_calibrate_tests.rs`](../../crates/perfgate-cli/tests/cli_calibrate_tests.rs)
- [`doctor.rs`](../../crates/perfgate-cli/src/doctor.rs)

Artifacts:

- `perfgate calibrate --emit-patch` TOML fragment
- non-mutating advisory reason output

Known limits:

- The patch is review input. It is not written automatically and does not
  authorize threshold loosening.

Review after: next-signal-or-decision-change

## PG-CLAIM-0024: Decision examples and suggestion reasons

Tier: supported
Proof freshness: current
Surface: CLI, examples, receipts
Linked docs: [`PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md), [`examples/decision-outcomes.md`](../examples/decision-outcomes.md), [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_decision_suggest_tests.rs`](../../crates/perfgate-cli/tests/cli_decision_suggest_tests.rs)
- [`cli_performance_decision_example_tests.rs`](../../crates/perfgate-cli/tests/cli_performance_decision_example_tests.rs)

Artifacts:

- `perfgate decision examples`
- `perfgate decision suggest` reason lines for improvement, regression, noise,
  and incomplete scenario/tradeoff evidence

Known limits:

- Decision examples teach patterns; they do not force structured decisions for
  simple gates.

Review after: next-decision-contract-change

## PG-CLAIM-0025: Canary freshness matrix

Tier: advisory
Proof freshness: current
Surface: status docs, external canary evidence, release smoke
Linked docs: [`CANARY_MATRIX.md`](CANARY_MATRIX.md), [`PROOF_FRESHNESS.md`](PROOF_FRESHNESS.md)
Linked gates: docs-check, docs-source-check, product-claims-check
Proof commands:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
```

Artifacts:

- canary freshness states
- proof/non-proof columns for external and release canary shapes

Known limits:

- Freshness is not a support tier.
- One canary shape does not prove every repo, runner, platform, or workflow.

Review after: next-canary-refresh

## PG-CLAIM-0026: Optional ledger operations policy

Tier: advisory
Proof freshness: current
Surface: server docs, in-repo server tests, canary matrix
Linked docs: [`DECISION_LEDGER_RUNBOOK.md`](../DECISION_LEDGER_RUNBOOK.md), [`CANARY_MATRIX.md`](CANARY_MATRIX.md), [`PERFGATE-SPEC-0009-evidence-maturity-contract`](../specs/PERFGATE-SPEC-0009-evidence-maturity-contract.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-server --all-features backup_restore_smoke_preserves_latest_history_audit_and_dry_run
cargo +1.95.0 run -p xtask -- docs-source-check
```

Linked tests:

- [`memory.rs`](../../crates/perfgate-server/src/storage/memory.rs)

Artifacts:

- decision export/audit export guidance
- restore drill guidance
- retention and migration policy guidance
- prune dry-run preservation proof for the in-memory store path

Known limits:

- This claim does not prove production database restore, large histories, or
  migration compatibility in every deployment.
- Server ledger mode remains optional team history, not local correctness.

Review after: next-server-ledger-change

## PG-CLAIM-0027: Agent repair-context guidance

Tier: advisory
Proof freshness: current
Surface: `repair_context.json`, CLI guidance, tests
Linked docs: [`PERFGATE-SPEC-0010-agent-repair-context-contract`](../specs/PERFGATE-SPEC-0010-agent-repair-context-contract.md), [`DEBUGGING_FIRST_CI_RUN.md`](../DEBUGGING_FIRST_CI_RUN.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features --test cli_repair_context_agent_tests
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- schema-compat
```

Linked tests:

- [`cli_repair_context_agent_tests.rs`](../../crates/perfgate-cli/tests/cli_repair_context_agent_tests.rs)
- [`check_guidance.rs`](../../crates/perfgate-cli/src/check_guidance.rs)

Artifacts:

- `repair_context.json`
- failure-class guidance for missing baseline, regression, setup command
  failure, high noise, host mismatch, review required, and server upload
  failure

Known limits:

- Repair context is advisory. It does not make agents policy authorities, and it
  does not authorize baseline promotion, threshold loosening, or server-ledger
  requirements.

Review after: next-agent-repair-contract-change

## PG-CLAIM-0028: Advisory policy rollout profiles

Tier: supported
Proof freshness: current
Surface: CLI, docs, config
Linked docs: [`POLICY_ROLLOUT.md`](../POLICY_ROLLOUT.md), [`PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract`](../specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md), [`PROOF_FRESHNESS.md`](PROOF_FRESHNESS.md), [`2026-05-19-policy-rollout-canary-droid-action.md`](../audits/2026-05-19-policy-rollout-canary-droid-action.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
```

Linked tests:

- [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs)
- [`policy.rs`](../../crates/perfgate-cli/src/policy.rs)

Artifacts:

- `perfgate policy profiles`
- `perfgate policy doctor --config perfgate.toml`
- `perfgate policy emit-patch --config perfgate.toml --bench <bench> --to <posture>`

Known limits:

- Policy profiles are starting points, not automatic benchmark selection.
- Promotion readiness and emitted patches are advisory and non-mutating.
- `required_gate` still requires deliberate review and policy application.

Review after: next-policy-ergonomics-change

## PG-CLAIM-0029: Review packet and Action policy posture

Tier: supported
Proof freshness: current
Surface: CLI, GitHub Action, artifacts
Linked docs: [`POLICY_ROLLOUT.md`](../POLICY_ROLLOUT.md), [`examples/action-failure-summaries.md`](../examples/action-failure-summaries.md), [`PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract`](../specs/PERFGATE-SPEC-0011-advisory-to-blocking-promotion-contract.md), [`PROOF_FRESHNESS.md`](PROOF_FRESHNESS.md), [`2026-05-19-policy-rollout-canary-droid-action.md`](../audits/2026-05-19-policy-rollout-canary-droid-action.md), [`2026-05-19-hosted-policy-action-canary-droid-action.md`](../audits/2026-05-19-hosted-policy-action-canary-droid-action.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
```

Linked tests:

- [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs)
- [`xtask/src/main.rs`](../../xtask/src/main.rs)

Artifacts:

- `perfgate policy review-packet --config perfgate.toml --bench <bench>`
- Action summary policy posture block
- local reproduction and policy doctor commands in review surfaces

Known limits:

- Review packets summarize receipts; they do not replace receipts as source of
  truth.
- Action posture summaries preserve existing configured exit-code behavior.
- Advisory maturity output does not become blocking from the summary alone.
- The hosted policy Action canary exercised an advisory missing-baseline path,
  not mature `gate_candidate` promotion or `required_gate` approval.

Review after: next-policy-ergonomics-change

## PG-CLAIM-0030: Agent policy guardrails

Tier: advisory
Proof freshness: current
Surface: CLI guidance, specs, tests
Linked docs: [`PERFGATE-SPEC-0012-agent-policy-change-guardrails`](../specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md), [`POLICY_ROLLOUT.md`](../POLICY_ROLLOUT.md), [`PROOF_FRESHNESS.md`](PROOF_FRESHNESS.md), [`2026-05-19-policy-rollout-canary-droid-action.md`](../audits/2026-05-19-policy-rollout-canary-droid-action.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- product-claims-check
```

Linked tests:

- [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs)
- [`check_guidance.rs`](../../crates/perfgate-cli/src/check_guidance.rs)

Artifacts:

- policy review-packet `Agent Guardrails` section
- missing-baseline, noisy-signal, mature-promotion, regression,
  tradeoff-candidate, and stale-proof guardrail fixtures

Known limits:

- Agent guardrails do not make agents policy authorities.
- Agents may inspect, rerun, summarize, and propose reviewable patches, but
  baseline promotion, threshold loosening, required-gate changes, tradeoff
  acceptance, and ledger requirements remain review-required.
- Fresh guardrail fixtures do not prove every agent workflow or external repo.

Review after: next-agent-policy-change

## PG-CLAIM-0031: Evidence intake adapters

Tier: supported
Proof freshness: current
Surface: CLI, receipts, docs
Linked docs: [`EVIDENCE_INTAKE.md`](../EVIDENCE_INTAKE.md), [`PERFGATE-SPEC-0013-evidence-source-contract`](../specs/PERFGATE-SPEC-0013-evidence-source-contract.md), [`PROOF_FRESHNESS.md`](PROOF_FRESHNESS.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_ingest_tests.rs`](../../crates/perfgate-cli/tests/cli_ingest_tests.rs)
- [`imported_evidence.rs`](../../crates/perfgate-cli/src/imported_evidence.rs)
- [`main.rs`](../../crates/perfgate-cli/src/main.rs)

Artifacts:

- `perfgate ingest --format generic-command-json`
- `perfgate ingest --format hyperfine`
- `perfgate ingest --format criterion`
- `perfgate ingest --format pytest-benchmark`
- `perfgate ingest --format k6`
- `perfgate ingest --format custom-json`
- `perfgate ingest --format custom-csv`
- `perfgate.run.v1` receipts with imported source metadata where supported

Known limits:

- External tools remain the measurement authority; perfgate imports evidence
  into receipts and review surfaces.
- Successful import does not prove benchmark maturity, host compatibility, or
  baseline quality.
- Summary-only imports have weaker noise support than raw samples.
- Current proof includes in-repo fixtures, one source-built external Rust CLI
  generic-command canary, and one source-built external non-Rust TypeScript
  command canary.

Review after: next-evidence-intake-change

## PG-CLAIM-0032: Imported evidence in review surfaces

Tier: supported
Proof freshness: current
Surface: CLI, GitHub Action, artifacts
Linked docs: [`EVIDENCE_INTAKE.md`](../EVIDENCE_INTAKE.md), [`ADOPTION_PACKS.md`](../ADOPTION_PACKS.md), [`POLICY_ROLLOUT.md`](../POLICY_ROLLOUT.md), [`PERFGATE-SPEC-0013-evidence-source-contract`](../specs/PERFGATE-SPEC-0013-evidence-source-contract.md), [`PROOF_FRESHNESS.md`](PROOF_FRESHNESS.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 run -p xtask -- action-check
```

Linked tests:

- [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs)
- [`cli_doctor_tests.rs`](../../crates/perfgate-cli/tests/cli_doctor_tests.rs)
- [`cli_calibrate_tests.rs`](../../crates/perfgate-cli/tests/cli_calibrate_tests.rs)
- [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs)
- [`xtask/src/main.rs`](../../xtask/src/main.rs)

Artifacts:

- `perfgate baseline doctor --config perfgate.toml --bench <bench>`
- `perfgate doctor signal --config perfgate.toml --bench <bench>`
- `perfgate calibrate --config perfgate.toml --bench <bench> --emit-patch`
- `perfgate policy doctor --config perfgate.toml --bench <bench>`
- `perfgate policy review-packet --config perfgate.toml --bench <bench>`
- Action policy posture summary for imported evidence where receipts expose
  source metadata

Known limits:

- Imported evidence remains advisory until normal maturity and policy review
  surfaces support promotion.
- Action summaries preserve configured exit-code behavior; they do not make
  advisory evidence blocking.
- Missing source path, metric mapping, host context, or raw samples is surfaced
  as a review limit, not treated as native proof.
- External hosted Action canaries for the 0.21 intake path remain pending.

Review after: next-evidence-intake-change

## PG-CLAIM-0033: Reviewable adoption packs

Tier: supported
Proof freshness: current
Surface: CLI, docs
Linked docs: [`ADOPTION_PACKS.md`](../ADOPTION_PACKS.md), [`EVIDENCE_INTAKE.md`](../EVIDENCE_INTAKE.md), [`BENCHMARK_RECIPES.md`](../BENCHMARK_RECIPES.md), [`GETTING_STARTED_GITHUB_ACTIONS.md`](../GETTING_STARTED_GITHUB_ACTIONS.md), [`PERFGATE-PROP-0008-evidence-intake-adoption-packs`](../proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features adoption
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_adoption_tests.rs`](../../crates/perfgate-cli/tests/cli_adoption_tests.rs)
- [`adoption_packs.rs`](../../crates/perfgate-cli/src/adoption_packs.rs)
- [`cli_help_snapshot_tests.rs`](../../crates/perfgate-cli/tests/cli_help_snapshot_tests.rs)

Artifacts:

- `perfgate adoption packs`
- `perfgate adoption packs --pack rust-cli`
- `perfgate adoption packs --pack rust-workspace`
- `perfgate adoption packs --pack python-service`
- `perfgate adoption packs --pack node-tool-action`
- `perfgate adoption packs --pack http-local-smoke`
- `perfgate adoption packs --pack generic-command`

Known limits:

- Adoption packs are starting points, not automatic benchmark selection.
- They do not promote baselines, loosen thresholds, make checks blocking, or
  require server ledger mode.
- Source-built Rust CLI and non-Rust TypeScript command canaries have proven
  the generic-command intake path, but HTTP/k6 adoption and public-release
  intake proof remain unproven.
- Source-built adoption-pack docs are not public release proof.

Review after: next-evidence-intake-change

## PG-CLAIM-0034: First-use setup recommendation and dry-run

Tier: supported
Proof freshness: current
Surface: CLI, docs, artifacts
Linked docs: [`FIRST_USEFUL_PERFORMANCE_REVIEW.md`](../FIRST_USEFUL_PERFORMANCE_REVIEW.md), [`ADOPTION_PACKS.md`](../ADOPTION_PACKS.md), [`PERFGATE-PROP-0002-first-useful-performance-review`](../../.rails/proposals/PERFGATE-PROP-0002-first-useful-performance-review.md), [`PERFGATE-SPEC-0002-first-useful-performance-review`](../../.rails/specs/PERFGATE-SPEC-0002-first-useful-performance-review.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features adoption
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
```

Linked tests:

- [`cli_adoption_tests.rs`](../../crates/perfgate-cli/tests/cli_adoption_tests.rs)
- [`adoption.rs`](../../crates/perfgate-cli/src/adoption.rs)
- [`adoption_packs.rs`](../../crates/perfgate-cli/src/adoption_packs.rs)

Artifacts:

- `perfgate adoption recommend`
- `perfgate adoption recommend --json`
- `perfgate adoption apply --pack <pack> --ci github --dry-run`
- `target/perfgate-adoption/perfgate.toml.patch`
- `target/perfgate-adoption/github-workflow.yml`
- `target/perfgate-adoption/local-commands.md`
- `target/perfgate-adoption/non-inferences.md`

Known limits:

- Recommendations are reviewable heuristics, not automatic benchmark
  selection.
- Dry-run apply does not write repo files unless a future explicit write path
  is reviewed separately.
- Dry-run setup does not promote baselines, loosen thresholds, make gates
  blocking, or require server ledger mode.
- Current proof is source-built and fixture-backed; it is not public release
  proof for the next shipped version.

Review after: next-first-useful-review-change

## PG-CLAIM-0035: First-use review explain and benchmark passport

Tier: supported
Proof freshness: current
Surface: CLI, docs, artifacts
Linked docs: [`FIRST_USEFUL_PERFORMANCE_REVIEW.md`](../FIRST_USEFUL_PERFORMANCE_REVIEW.md), [`PERFORMANCE_REVIEW_FAILURE_GALLERY.md`](../PERFORMANCE_REVIEW_FAILURE_GALLERY.md), [`PERFGATE-SPEC-0002-first-useful-performance-review`](../../.rails/specs/PERFGATE-SPEC-0002-first-useful-performance-review.md), [`CANARY_MATRIX.md`](CANARY_MATRIX.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features review
cargo +1.95.0 test -p perfgate-cli --all-features policy
cargo +1.95.0 run -p xtask -- action-check
```

Linked tests:

- [`cli_review_tests.rs`](../../crates/perfgate-cli/tests/cli_review_tests.rs)
- [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs)
- [`benchmark_passport.rs`](../../crates/perfgate-cli/src/benchmark_passport.rs)
- [`review.rs`](../../crates/perfgate-cli/src/review.rs)

Artifacts:

- `perfgate review explain --config perfgate.toml --bench <bench>`
- `perfgate review explain --config perfgate.toml --bench <bench> --json`
- `perfgate policy review-packet --config perfgate.toml --bench <bench>`
- benchmark passport in review and Action summary surfaces

Known limits:

- Review explain and packets summarize receipts; receipts remain the source of
  truth.
- The benchmark passport does not make advisory evidence blocking.
- Action summary rendering preserves configured exit-code behavior.
- Current proof is in-repo and source-built; hosted first-useful-review Action
  canaries and public-release proof remain explicit canary gaps.

Review after: next-first-useful-review-change

## PG-CLAIM-0036: Agent-safe repair context and guardrails

Tier: advisory
Proof freshness: current
Surface: CLI, artifacts, tests
Linked docs: [`FIRST_USEFUL_PERFORMANCE_REVIEW.md`](../FIRST_USEFUL_PERFORMANCE_REVIEW.md), [`PERFORMANCE_REVIEW_FAILURE_GALLERY.md`](../PERFORMANCE_REVIEW_FAILURE_GALLERY.md), [`PERFGATE-SPEC-0002-first-useful-performance-review`](../../.rails/specs/PERFGATE-SPEC-0002-first-useful-performance-review.md), [`PERFGATE-SPEC-0012-agent-policy-change-guardrails`](../specs/PERFGATE-SPEC-0012-agent-policy-change-guardrails.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features repair
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 test -p perfgate-cli --all-features policy
```

Linked tests:

- [`cli_repair_context_tests.rs`](../../crates/perfgate-cli/tests/cli_repair_context_tests.rs)
- [`cli_check_tests.rs`](../../crates/perfgate-cli/tests/cli_check_tests.rs)
- [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs)
- [`repair_context.rs`](../../crates/perfgate-cli/src/repair_context.rs)

Artifacts:

- `repair_context.json`
- `perfgate review explain` agent guardrails
- policy review-packet agent guardrails

Known limits:

- Agent guidance is advisory. It does not authorize baseline promotion,
  threshold loosening, required-gate changes, tradeoff acceptance, or server
  ledger requirements.
- Current proof covers fixture-backed repair and policy scenarios, not every
  agent workflow or external hosted repair loop.
- A copyable standalone agent prompt is not claimed as a shipped command
  surface.

Review after: next-agent-repair-contract-change

## PG-CLAIM-0037: Non-mutating promotion plans

Tier: advisory
Proof freshness: current
Surface: CLI, config guidance, tests
Linked docs: [`FIRST_USEFUL_PERFORMANCE_REVIEW.md`](../FIRST_USEFUL_PERFORMANCE_REVIEW.md), [`POLICY_ROLLOUT.md`](../POLICY_ROLLOUT.md), [`PERFGATE-SPEC-0002-first-useful-performance-review`](../../.rails/specs/PERFGATE-SPEC-0002-first-useful-performance-review.md)
Proof commands:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 test -p perfgate-cli --all-features policy
```

Linked tests:

- [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs)
- [`cli_policy_tests.rs`](../../crates/perfgate-cli/tests/cli_policy_tests.rs)
- [`policy.rs`](../../crates/perfgate-cli/src/policy.rs)

Artifacts:

- `perfgate baseline promote-plan --config perfgate.toml --bench <bench>`
- `perfgate policy promote-plan --config perfgate.toml --bench <bench> --to gate_candidate`
- `perfgate policy promote-plan --config perfgate.toml --bench <bench> --to required_gate`

Known limits:

- Promotion plans do not write config, baselines, thresholds, policy, or server
  settings.
- `gate_candidate` is reviewable evidence, not blocking policy.
- `required_gate` remains a human policy decision and needs explicit approval.
- Plans do not prove public-release behavior until the commands ship and are
  tested from public artifacts.

Review after: next-policy-promotion-change
