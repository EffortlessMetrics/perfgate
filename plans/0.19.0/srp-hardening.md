# perfgate 0.19.0 SRP Hardening Queue Convergence

Status: ready
Owner: perfgate maintainers
Created: 2026-05-15
Milestone: 0.19.0
Current PR: maint: converge SRP refactor queue
Linked proposal:
Linked specs: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md; docs/specs/PERFGATE-SPEC-0002-package-surface-boundary.md
Linked ADRs: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Linked policy: policy/public_crates.txt; policy/absorbed_crates.txt
Support/status impact: no product-claim changes in the convergence PR; later SRP batches must preserve first-use UX claims and release readiness
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check
Blocks: duplicate SRP extraction PRs
Blocked by:
Rollback: revert this plan; no product behavior or public surface changes are made by the convergence PR

## Goal

Converge the overlapping SRP refactor queue before more extraction PRs land.

The queue currently contains multiple PRs extracting the same responsibilities
under competing names such as `io.rs`, `storage.rs`, `artifact_io.rs`,
`json_location.rs`, `io_locations.rs`, `cli_parsers.rs`, and
`cli_parsing.rs`. Those variants should not merge independently.

This plan picks canonical module names, identifies superseded PRs, and defines
the merge/rebase order for the SRP hardening lane. It intentionally does not
move code.

## Non-Goals

- Do not change public crates, public APIs, CLI behavior, receipt schemas,
  Action behavior, product claims, or the active 0.18 release goal.
- Do not merge multiple PRs that extract the same helper family under different
  names.
- Do not use SRP hardening as a reason to add new product behavior.
- Do not collapse the five public crates.

## Canonical CLI Module Map

`crates/perfgate-cli/src/main.rs` should become command dispatch and top-level
orchestration only. Internal helpers should converge on these module names:

| Module | Responsibility |
| --- | --- |
| `cli_parsing.rs` | clap parser helpers, option validators, command normalization |
| `baseline.rs` | baseline selector parsing and baseline-path selection |
| `storage.rs` | local/object-store JSON I/O, artifact location helpers, and atomic writes |
| `repair_context.rs` | `repair_context` receipt generation and git/change context |
| `check_guidance.rs` | failure taxonomy and repair guidance |
| `doctor.rs` | doctor, adoption-state reporting, and calibration-adjacent readiness |
| `artifact_explain.rs` | `perfgate explain artifacts` |
| `decision_suggest.rs` | `perfgate decision suggest` readiness |
| `probe_templates.rs` | `perfgate probes init` templates |
| `ledger_doctor.rs` | optional ledger readiness |

Non-canonical names for this lane:

```text
io.rs
artifact_io.rs
json_location.rs
io_locations.rs
cli_parsers.rs
git_context.rs
```

If a branch contains useful code under a non-canonical name, port the useful
code into the canonical module and close or rework the original PR. Do not
merge the non-canonical module and rename it later in a second PR.

## Canonical Domain Module Map

Use one domain split:

```text
crates/perfgate/src/domain/
  metrics.rs
  comparison.rs
  report.rs
  stats_compute.rs
```

The public facade in `domain/mod.rs` must preserve existing imports and
behavior. Do not merge competing `compare.rs` and `comparison.rs` layouts.

## Canonical Export Module Map

Use one export split:

```text
crates/perfgate/src/app/export/
  mod.rs
  escape.rs
  format.rs
  rows.rs
  formatters.rs
```

Do not merge competing export layouts that introduce separate
format-per-file modules (`csv.rs`, `html.rs`, `jsonl.rs`, `junit.rs`,
`prometheus.rs`) or alternate names such as `escaping.rs`, `row_builders.rs`,
`serializers.rs`, and `mapping.rs`.

## PR Disposition

This table is based on the live #442-#457 queue as of 2026-05-15.

| PR | Current title | Files / shape | Disposition |
| --- | --- | --- | --- |
| #442 | Refactor CLI support code into SRP modules | `artifact_io.rs`, `git_context.rs` | Close as superseded. Port any useful git/change-context behavior into `repair_context.rs`. |
| #443 | Refactor CLI helpers into SRP modules | `cli_parsers.rs`, `storage.rs` | Close as superseded by canonical `cli_parsing.rs` plus the storage/baseline PR. |
| #444 | Refactor export module into SRP submodules | `escaping.rs`, `mapping.rs`, `row_builders.rs`, `serializers.rs` | Close as superseded by the canonical export layout. |
| #445 | refactor(cli): split IO helpers into SRP module | `io.rs` | Close as superseded. `io.rs` is not canonical. |
| #446 | Refactor perfgate-cli: extract artifact_io and repair_context modules | `artifact_io.rs`, `repair_context.rs` | Rework before merge: keep `repair_context.rs`, move I/O helpers to `storage.rs`, drop `artifact_io.rs`. |
| #447 | Refactor export renderers into SRP modules | `csv.rs`, `html.rs`, `jsonl.rs`, `junit.rs`, `prometheus.rs` | Close as superseded by the canonical export layout. |
| #448 | Refactor CLI storage helpers into SRP storage module | `storage.rs` | Close as superseded if #457 is reworked into `baseline.rs` plus `storage.rs`; otherwise use only as a fallback storage source. |
| #449 | Refactor CLI helper code into SRP modules | `cli_parsers.rs`, `io_locations.rs` | Close as superseded. Neither module name is canonical. |
| #450 | Refactor domain analytics into SRP submodules | `comparison.rs`, `metrics.rs`, `report.rs`, `stats_compute.rs` | Keep as canonical domain split candidate. |
| #451 | Refactor perfgate-cli: extract CLI parsing and JSON location helpers | `cli_parsing.rs`, `json_location.rs` | Rework before merge: keep `cli_parsing.rs`, move location/storage helpers to `storage.rs`, drop `json_location.rs`. |
| #452 | refactor(cli): split check guidance into SRP module | `check_guidance.rs` | Keep as canonical failure-guidance candidate after storage/parsing settle. |
| #453 | Refactor domain logic into SRP submodules (compare, metrics, report) | `compare.rs`, `metrics.rs`, `report.rs` | Close as superseded by #450's `comparison.rs` and `stats_compute.rs` layout. |
| #454 | Refactor export module into SRP submodules | `escape.rs`, `format.rs`, `rows.rs`, `formatters.rs` | Keep as canonical export split candidate. |
| #455 | Refactor CLI storage helpers into SRP module | `storage.rs` | Close as duplicate of #448/#457 storage work. |
| #456 | Refactor CLI doctor code into SRP module | `doctor.rs` | Keep as canonical doctor candidate after storage/parsing settle. |
| #457 | Refactor CLI baseline and storage helpers into SRP modules | `baseline.rs`, `io.rs` | Keep only after rework: rename `io.rs` to `storage.rs` and keep baseline selection in `baseline.rs`. Do not merge as-is. |

## Merge Order

1. Land this convergence plan.
2. Close the duplicate PRs identified above as superseded.
3. Merge one clean domain split:

   ```text
   #450 -> domain/{metrics,comparison,report,stats_compute}.rs
   ```

   Close #453.

4. Merge one clean export split:

   ```text
   #454 -> app/export/{escape,format,rows,formatters}.rs
   ```

   Close #444 and #447.

5. Rework and merge one CLI baseline/storage split:

   ```text
   #457 -> baseline.rs + storage.rs
   ```

   Close #445, #448, and #455. Do not retain `io.rs`.

6. Rework and merge CLI parsing:

   ```text
   #451 -> cli_parsing.rs
   ```

   Drop `json_location.rs`; close #443 and #449.

7. Merge the first-use UX helper extractions after storage and parsing are
   stable:

   ```text
   #452 -> check_guidance.rs
   #446 -> repair_context.rs only, with storage delegated to storage.rs
   #456 -> doctor.rs
   follow-up PRs -> artifact_explain.rs, decision_suggest.rs,
                    probe_templates.rs, ledger_doctor.rs
   ```

8. Run the broad SRP contract proof.
9. Close the SRP hardening lane with a handoff that records canonical modules,
   superseded PRs, public-surface preservation, proof commands, and remaining
   intentional debt.

## Broad Proof

The proof PR for this lane should run:

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 check --workspace --all-targets --all-features --locked
cargo +1.95.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.95.0 test --workspace --all-targets --all-features --locked
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

Use an external target directory for heavy local validation if the default
workspace drive is constrained.

## Work Item: queue-convergence

Status: current
Linked proposal:
Linked spec: docs/specs/PERFGATE-SPEC-0008-first-use-ux-contract.md
Linked ADR: docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Blocks: all SRP extraction PRs
Blocked by:

### Goal

Record canonical module names, duplicate PR disposition, and merge order before
any more SRP extraction PRs land.

### Production delta

Docs-only plan file. No code movement.

### Acceptance

- Canonical CLI, domain, and export module maps are recorded.
- #442-#457 have explicit dispositions.
- Duplicate storage, parsing, export, and domain layouts are called out.
- The active 0.18 release goal remains untouched.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert this plan file. No product or code behavior changes are made.
