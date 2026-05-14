# 0.18.0 External Adoption Canaries and Trust Hardening

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Linked proposal: [`PERFGATE-PROP-0003-external-adoption-canaries`](../../docs/proposals/PERFGATE-PROP-0003-external-adoption-canaries.md)
Linked specs: [`PERFGATE-SPEC-0007-guided-adoption-contract`](../../docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md), [`PERFGATE-SPEC-0004-user-devex-paved-road`](../../docs/specs/PERFGATE-SPEC-0004-user-devex-paved-road.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../../docs/specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../../docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked plan:
Linked policy:
Support/status impact: [`PRODUCT_CLAIMS.md`](../../docs/status/PRODUCT_CLAIMS.md) links external canary evidence for first-hour and staged adoption claims
Proof commands: docs-check, doc-test, docs-source-check, product-claims-check, action-check, targeted CLI/server tests
Current PR: complete
Blocks:
Blocked by:
Rollback: revert the individual proof or copy-fix PR that introduced the regression; canary notes are evidence and can be superseded by newer canaries

## Goal

Move perfgate from internally coherent adoption proof to external trust proof:
real repositories should demonstrate first-hour setup, multiple benchmark
shapes, noisy-command guidance, non-Rust command benchmarks, action-summary
rehearsal, and optional server-ledger operations without making the server
part of correctness.

## Work Items

| Work item | Status | PR | Evidence |
| --- | --- | --- | --- |
| External canary proposal | implemented | #400 | [`PERFGATE-PROP-0003`](../../docs/proposals/PERFGATE-PROP-0003-external-adoption-canaries.md) |
| Small Rust CLI canary | implemented | #401 | [`diffguard canary`](../../docs/audits/2026-05-13-external-canary-diffguard-small-rust-cli.md) |
| Zero-benchmark next-step fix | implemented | #402 | `perfgate init` now points users to add `[[bench]]` before `check --all` |
| Generated badge refresh | implemented | #403 | public badge endpoint refresh |
| Signal/noise calibration guide | implemented | #404 | [`SIGNAL_CALIBRATION.md`](../../docs/SIGNAL_CALIBRATION.md) |
| Probe design patterns | implemented | #405 | [`PROBE_DESIGN_PATTERNS.md`](../../docs/PROBE_DESIGN_PATTERNS.md) |
| Platform metric support matrix | implemented | #406 | [`PLATFORM_SUPPORT.md`](../../docs/status/PLATFORM_SUPPORT.md) |
| 0.18 release cutover decision | implemented | #407 | [`release-0.18.0-cutover-decision.md`](../../docs/audits/release-0.18.0-cutover-decision.md) |
| Action failure archaeology examples | implemented | #408 | action failure summary fixtures and `action-check` |
| Server ledger key rotation smoke | implemented | #409 | `server_operations_smoke_path_memory` create/list/rotate key proof |
| Larger Rust workspace canary | implemented | #410 | [`shipper canary`](../../docs/audits/2026-05-13-external-canary-shipper-large-rust-workspace.md) |
| Non-Rust command benchmark canary | implemented | #411 | [`droid-action canary`](../../docs/audits/2026-05-13-external-canary-droid-action-non-rust-command.md) |
| Language-neutral zero-benchmark example | implemented | #412 | init stdout and generated `.perfgate/README.md` tests |

## Acceptance

- At least three external canary evidence notes exist.
- Canaries cover a small Rust CLI, a larger Rust workspace, and a non-Rust
  command-benchmark repository.
- At least one canary records noisy-command guidance and the user-facing next
  step.
- At least one canary produces multiple benchmark artifact directories and
  promoted baselines.
- Product claims link external canary evidence where it strengthens first-hour
  and staged-adoption claims.
- Action-summary failure surfaces remain covered by golden examples and
  `cargo +1.95.0 run -p xtask -- action-check`.
- Server ledger operations proof covers export, dry-run prune, audit export,
  and API key create/list/rotate smoke while keeping ledger mode optional.
- The release decision states whether 0.18.0 is being published or deferred.

## Proof Commands

Docs/status PRs used:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Product/test PRs used targeted checks including:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features server_operations_smoke_path_memory --locked
cargo +1.95.0 test -p perfgate-cli --all-features init_without_discovered_benchmarks_points_to_bench_entry_first --locked
cargo +1.95.0 test -p perfgate --all-features onboarding_readme_mentions_bench_entry_when_none_discovered --locked
cargo +1.95.0 run -p xtask -- action-check
```

## Non-Goals

- Do not infer 0.18.0 publication from this plan; the release cutover decision
  explicitly deferred publication.
- Do not make external canaries required CI.
- Do not make server ledger mode required for local correctness.
- Do not add public crates or new performance primitives.
- Do not treat one canary as a claim that every repository shape is covered.

## Follow-Up Candidates

- Hosted external GitHub Action canary if a repo owner wants a public PR-based
  adoption proof.
- Additional canaries for Linux/macOS shells and larger hosted runners.
- Server ledger backup/restore drills for teams that operate SQLite or
  PostgreSQL as shared infrastructure.
- More probe-backed canaries once a candidate repo has meaningful stable probe
  points.
