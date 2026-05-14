# External Trust Closeout

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Linked proposal: [`PERFGATE-PROP-0003-external-adoption-canaries`](../proposals/PERFGATE-PROP-0003-external-adoption-canaries.md)
Linked specs: [`PERFGATE-SPEC-0007-guided-adoption-contract`](../specs/PERFGATE-SPEC-0007-guided-adoption-contract.md), [`PERFGATE-SPEC-0004-user-devex-paved-road`](../specs/PERFGATE-SPEC-0004-user-devex-paved-road.md), [`PERFGATE-SPEC-0003-performance-decision-contract`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked plan: [`external-adoption-canaries.md`](../../plans/0.18.0/external-adoption-canaries.md)
Linked policy:
Support/status impact: [`PRODUCT_CLAIMS.md`](../status/PRODUCT_CLAIMS.md) links canary evidence for first-hour and staged adoption claims
Proof commands: docs-check, doc-test, docs-source-check, product-claims-check, action-check, targeted CLI/server tests

## What Changed

This closeout records the lane that moved perfgate from internally coherent to
externally exercised. The lane did not add another performance primitive. It
added external canary receipts, clearer signal guidance, probe design guidance,
platform support boundaries, action failure archaeology, server-ledger key
rotation proof, and a deliberate 0.18 release decision.

## External Canaries

The lane recorded three canary evidence notes:

- [`diffguard` small Rust CLI](../audits/2026-05-13-external-canary-diffguard-small-rust-cli.md):
  first-hour setup exposed zero-benchmark next-step friction, then proved local
  check, baseline promotion, required-baseline rerun, and generated workflow
  wiring after a benchmark was added.
- [`shipper` larger Rust workspace](../audits/2026-05-13-external-canary-shipper-large-rust-workspace.md):
  proved multiple benchmark entries, multiple artifact directories, multiple
  promoted baselines, required-baseline rerun, and noisy-command guidance.
- [`droid-action` non-Rust command benchmark](../audits/2026-05-13-external-canary-droid-action-non-rust-command.md):
  proved a TypeScript repository can use plain command benchmarks with the same
  config, artifact, baseline, and workflow model.

## Fixes From Canaries

Two canary findings produced narrow product fixes:

- `perfgate init` now tells zero-discovery users to add a `[[bench]]` entry
  before running `check --all`.
- The zero-benchmark example is language-neutral and shows Rust and Node
  examples instead of pointing every repo at a Cargo-only command.

## Trust Hardening

The lane also landed:

- signal/noise calibration guidance for thresholds, paired mode, host class,
  repeat count, and when not to gate;
- probe design patterns for naming, placement, refactor stability, and bad
  probes;
- platform metric support boundaries;
- action failure archaeology examples for messy first CI failures;
- server-ledger API key create/list/rotate smoke in the memory operations path;
- 0.18.0 cutover decision deferring publication instead of implying public
  availability.

## Product Claims

`docs/status/PRODUCT_CLAIMS.md` now links canary evidence for:

- `PG-CLAIM-0009`: first-hour local adoption path;
- `PG-CLAIM-0010`: staged adoption levels.

The existing ledger claim remains supported by in-repo server operations proof,
including decision export, dry-run prune, audit export, and API key
create/list/rotate smoke. Server ledger mode remains optional team-scale
history, not a prerequisite for local correctness.

## Proof Commands

Docs/status proof used:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Product/test proof used targeted checks including:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features server_operations_smoke_path_memory --locked
cargo +1.95.0 test -p perfgate-cli --all-features init_without_discovered_benchmarks_points_to_bench_entry_first --locked
cargo +1.95.0 test -p perfgate --all-features onboarding_readme_mentions_bench_entry_when_none_discovered --locked
cargo +1.95.0 run -p xtask -- action-check
```

## What Not To Infer

- The canaries do not prove every repository shape or every hosted runner.
- The canaries did not push external PRs or run hosted CI in the external
  repositories; action-summary behavior is covered by in-repo golden examples
  and `action-check`.
- 0.18.0 was not published, tagged, or aliased by this lane.
- Server-ledger mode is not required for local checks, decisions, or bundles.
- Probe-backed canaries still need a repository with meaningful stable probe
  points before they should become external evidence.

## Remaining Follow-Up

The next useful work is optional and should be demand-driven:

- run a hosted external GitHub Action canary when a candidate repo owner wants
  PR-based evidence;
- add Linux/macOS canaries for shell and metric-boundary coverage;
- run a server-ledger backup/restore drill for teams treating the ledger as
  shared infrastructure;
- add a probe-backed external canary when a real repository has stable probe
  IDs worth preserving.
