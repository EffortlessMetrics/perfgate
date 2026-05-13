# v0.18.0 Adoption Readiness Snapshot

Date: 2026-05-13

Purpose: record the current 0.18.0 adoption-hardening proof after wrapper
absorption and external adoption smoke work. This is a pre-release readiness
snapshot. It does not publish crates, move tags, create a GitHub release, or
change action aliases.

## Scope

This snapshot covers the implementation-hardening lane after guided adoption
closed out. The lane goal was to make perfgate release-boring and
adoption-proven:

- a cold user can initialize, check, promote, and rerun with required
  baselines;
- a reviewer can follow structured decision receipts through a portable bundle;
- a GitHub Action failure summary has stable, checked example shapes;
- a team can exercise optional decision-ledger operations safely; and
- the public crate surface remains the five intended contract crates after
  compatibility wrapper absorption.

## Landed Work

| Area | Evidence |
| --- | --- |
| Presentation wrapper absorption | PR #389 removed `perfgate-render`, `perfgate-export`, and `perfgate-sensor` as workspace packages and routed callers to owning facade modules. |
| Runtime/integration wrapper absorption | PR #390 removed `perfgate-adapters` and `perfgate-github` as workspace packages. |
| App/domain wrapper absorption | PR #391 removed `perfgate-app`, `perfgate-domain`, and `perfgate-paired` as workspace packages while preserving user-facing commands and receipt names. |
| Contract-adjacent wrapper absorption | PR #392 removed `perfgate-error` and `perfgate-api` as workspace packages while preserving `perfgate-types`, `perfgate-client`, and `perfgate-server` as public seams. |
| Package-surface closeout | PR #393 added the wrapper absorption handoff and marked the cleanup plan implemented. |
| First-hour smoke | PR #394 strengthened the generated sample repo path through missing-baseline guidance, baseline promotion, required-baseline rerun, workflow wiring, and commit guidance. |
| Generated badge refresh | PR #395 refreshed generated badge endpoint data separately from product changes. |
| Structured decision smoke | PR #396 extended the structured-decision E2E path through `decision bundle` and asserted the portable bundle embeds expected artifacts. |
| Action summary examples | PR #397 added checked golden examples for common GitHub Action failure summaries and wired them into `action-check`. |
| Server ledger operations smoke | PR #398 extended the server operations smoke through decision upload with an artifact index, history/latest/debt/export, dry-run prune preservation, and create-audit visibility. |

## Current Product Proof

| User path | Proof surface |
| --- | --- |
| First-hour local gate | `cli_first_run_e2e_tests.rs`, `FIRST_HOUR.md`, `ADOPTION_LEVELS.md`, `DEBUGGING_FIRST_CI_RUN.md` |
| Required-baseline rerun | `first_run_paved_road_creates_artifacts_and_baselines` checks missing-baseline copy, promotion, and `--require-baseline` rerun behavior |
| Structured decisions | `cli_structured_decision_e2e_tests.rs` covers check, probe compare, scenario, tradeoff, `decision.md`, `decision.index.json`, and `decision-bundle.json` |
| Action failure UX | `docs/examples/action-failure-summaries.md` and `cargo +1.95.0 run -p xtask -- action-check` |
| Optional decision ledger | `server_operations_smoke_path_memory` covers upload, history, latest, debt, export, prune dry-run, and audit visibility |
| Package surface | `cargo +1.95.0 run -p xtask -- public-surface --strict` and `docs/handoffs/2026-05-13-wrapper-absorption-closeout.md` |

## Current Package Surface

The only public contract crates remain:

```text
perfgate
perfgate-cli
perfgate-types
perfgate-client
perfgate-server
```

Former production compatibility wrapper crates are no longer workspace
packages. Historical absorbed-crate disposition remains documented in
`policy/absorbed_crates.txt`, `docs/CRATE_SEAMS.md`, and the wrapper absorption
closeout.

## Snapshot Proof Commands

These commands define the adoption-readiness proof for this snapshot:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 test -p perfgate-cli --all-features first_run --locked
cargo +1.95.0 test -p perfgate-cli --all-features decision_evaluate_runs_structured_decision_workflow --locked
cargo +1.95.0 test -p perfgate-cli --all-features server_operations_smoke_path_memory --locked
cargo +1.95.0 test -p xtask action_summary_examples --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Release Boundary

This snapshot is not a release PR. Before publishing 0.18.0, run the full
release proof matrix from `docs/RELEASE_READINESS.md`, including package-list
and dry-run publish checks for the five public crates.

## Remaining Work

- Run full hosted CI on the eventual 0.18.0 release candidate.
- Run the publish dry-run matrix immediately before any 0.18.0 publish.
- Keep generated badge and baseline refresh PRs separate from product strategy.
