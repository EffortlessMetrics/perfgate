# SRP Hardening Closeout

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-16
Milestone: 0.18.0
Linked proposal: [`PERFGATE-PROP-0004-0-18-release-cutover`](../proposals/PERFGATE-PROP-0004-0-18-release-cutover.md)
Linked specs: [`PERFGATE-SPEC-0002-package-surface-boundary`](../specs/PERFGATE-SPEC-0002-package-surface-boundary.md), [`PERFGATE-SPEC-0008-first-use-ux-contract`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md)
Linked ADRs: [`PERFGATE-ADR-0001-public-crates-are-contracts`](../adr/PERFGATE-ADR-0001-public-crates-are-contracts.md)
Linked plan: [`release-cutover.md`](../../plans/0.18.0/release-cutover.md)
Linked policy: [`public_crates.txt`](../../policy/public_crates.txt), [`absorbed_crates.txt`](../../policy/absorbed_crates.txt)
Support/status impact: no product-claim tier changes; this lane preserved first-use UX, action, receipt, schema, and release posture while tightening internal ownership.
Proof commands: fmt, workspace check, clippy, workspace tests, public-surface, arch, schema-compat, action-check, docs-source-check, product-claims-check, docs-check, doc-test, git diff --check

## What Changed

This lane converged the overlapping SRP refactor queue into one canonical module
map and then landed behavior-preserving extractions. The product behavior stayed
the same: first-use commands, receipts, schemas, Action summaries, public crates,
and release cutover state were preserved.

The final CLI module map is:

- `main.rs`: command dispatch and top-level orchestration.
- `cli_parsing.rs`: clap parser helpers, option validators, and command normalization.
- `baseline.rs`: baseline selector parsing and baseline-path selection.
- `storage.rs`: local/object-store JSON I/O and atomic writes.
- `repair_context.rs`: repair-context receipt generation and git/change context.
- `check_guidance.rs`: first-use failure taxonomy and repair guidance.
- `doctor.rs`: doctor, adoption state, and calibration.
- `artifact_explain.rs`: artifact explanation command.
- `decision_suggest.rs`: structured-decision readiness guidance.
- `probe_templates.rs`: probe starter templates.
- `ledger_doctor.rs`: optional server-ledger readiness.

The domain and export modules were also split into stable internal owner
modules while keeping public facades intact.

## Queue Disposition

The convergence PR established the canonical queue and closed duplicate
extractions before more refactors landed.

Canonical merged work:

- #458: converged SRP queue, canonical names, superseded PR list, and merge order.
- #450: domain analytics split into `metrics`, `comparison`, `report`, and `stats_compute`.
- #454: export split into `escape`, `format`, `rows`, and `formatters`.
- #457: CLI baseline and storage extraction using `baseline.rs` and `storage.rs`.
- #451: CLI parsing helper extraction.
- #452: check guidance extraction.
- #446: repair-context extraction using canonical storage helpers.
- #456: doctor/adoption-state extraction.
- #459: remaining first-use command extraction for artifact explanation,
  decision readiness, probe templates, and ledger readiness.

Superseded duplicate PRs were closed rather than merged:

- #442, #443, #444, #445, #447, #448, #449, #453, and #455.

The remaining open PRs at closeout were unrelated to this SRP lane:

- #437: higher-is-better decision improvement detection.
- #429: generated badge endpoint refresh.
- #414: nightly baseline/trend refresh.

## Evidence

Hosted PR proof:

- #459 passed hosted CI before merge: cargo-deny, Ubuntu tests, Windows tests,
  server integration, RIPR PR evidence, CodeRabbit, and GitGuardian succeeded.

Merged-main proof passed after #459:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

Structural audit:

- `crates/perfgate-cli/src/` contains the canonical modules listed above.
- duplicate files such as `io.rs`, `artifact_io.rs`, `json_location.rs`,
  `io_locations.rs`, and `cli_parsers.rs` are absent.
- `main.rs` is no longer the owner for artifact explanation, decision
  readiness, probe starter templates, or ledger readiness.
- `public-surface --strict` still reports five publishable public packages.
- `.codex/goals/active.toml` still belongs to the operator-gated 0.18 release
  cutover; this SRP lane did not archive or replace it.

## What Not To Infer

- This lane did not publish `0.18.0`.
- This lane did not tag a release or move `v0`, `v0.18`, or `v0.18.0`.
- This lane did not change CLI behavior, receipt schemas, Action behavior, or
  public crate surfaces.
- This lane did not merge unrelated badge, baseline/trend, or metric-decision
  PRs.

## Remaining Follow-Up

The next release-critical work remains the operator-gated 0.18 publication path:
publish crates with approval, verify crates.io, cut the GitHub release, move
aliases intentionally, run public install smoke, and close the release lane.
