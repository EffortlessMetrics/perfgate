# perfgate 0.21.0 Evidence Intake and Adoption Packs Plan

Status: active
Owner: perfgate maintainers
Created: 2026-05-19
Milestone: 0.21.0
Current PR: TBD (next: criterion-adapter)
Linked proposal: [`PERFGATE-PROP-0008-evidence-intake-adoption-packs`](../../docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md)
Linked specs: [`PERFGATE-SPEC-0013-evidence-source-contract`](../../docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md)
Linked ADRs: [`PERFGATE-ADR-0002-receipts-first-performance-decisions`](../../docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)
Linked policy: policy ledgers remain referenced by specs and status docs; no policy row changes in this plan PR
Support/status impact: product claims should add or promote adapter and adoption-pack claims only after behavior, fixtures, docs, Action proof, and external canaries land
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check
Blocks: generic command JSON adapter, hyperfine adapter, Criterion adapter, pytest-benchmark adapter, k6 adapter, custom JSON/CSV mapping, adoption packs, Action review path, external canaries, product-claim mapping, closeout
Blocked by:
Rollback: revert this plan and `.codex/goals/active.toml`; proposal and evidence-source contract remain accepted source-of-truth artifacts

## Goal

Make perfgate useful for teams that already have benchmark ecosystems.
0.18 made the tool public, 0.19 classified evidence maturity, and 0.20 made
policy rollout reviewable. 0.21 should let existing measurements flow into the
same receipt, maturity, policy, review, and Action surfaces without turning
perfgate into another benchmark engine.

The lane target is:

```text
existing benchmark output
  -> adapter mapping
  -> perfgate evidence
  -> maturity and policy posture
  -> review packet and Action summary
```

## Activation Boundary

The 0.18 release cutover, 0.19 evidence maturity lane, and 0.20 policy
ergonomics lane are complete and archived. This lane builds on those surfaces.

This plan does not add a dashboard, benchmark scheduler, mandatory server
ledger path, public crate expansion, automatic baseline promotion, automatic
threshold loosening, or blocking policy by default. Receipt schema changes are
forbidden unless a follow-up accepted spec proves the need and the PR runs
schema-compat proof.

## Operating Rules

- Keep one adapter or narrow product delta per PR.
- Prefer existing receipt shapes and isolated adapter metadata before schema
  changes.
- Keep source kind, metric unit, metric direction, sample model, host context,
  baseline compatibility, and non-inferences visible.
- Fail closed for ambiguous unit or direction mapping.
- Preserve direction-aware movement semantics across imported evidence.
- Keep imported evidence advisory until maturity and policy surfaces support
  promotion.
- Do not silently promote baselines, make gates blocking, loosen thresholds, or
  require server ledger mode.
- Keep adoption packs reviewable templates, not magic detection.
- Keep product claims conservative until fixtures, docs, Action proof, and
  canaries exist.
- Keep generated badge or baseline churn separate from adapter semantics.

## PR Sequence

| PR | Work item | Status | Files / surface |
|----|-----------|--------|-----------------|
| 574 | Evidence intake proposal | merged | `docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md` |
| 576 | Evidence source contract | merged | `docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md` |
| 578 | Implementation plan and active goal | merged | `plans/0.21.0/evidence-intake-adoption-packs.md`, `.codex/goals/active.toml` |
| 580 | Generic command JSON adapter | merged | CLI adapter/import surface, fixtures, docs |
| 585 | hyperfine JSON adapter | merged | hyperfine fixtures, unit/direction/sample mapping |
| TBD | Criterion adapter | pending | Criterion fixtures and non-inference docs |
| TBD | pytest-benchmark JSON adapter | pending | Python benchmark fixtures and environment limits |
| TBD | k6 summary JSON adapter | pending | HTTP/load-test fixtures and capacity non-inferences |
| TBD | Custom JSON/CSV mapping | pending | explicit field mapping and fail-closed errors |
| TBD | Imported evidence maturity integration | pending | baseline doctor, signal doctor, calibration/policy surfaces |
| TBD | Review packet and Action posture | pending | report/comment/action-check coverage |
| TBD | Adoption pack catalog | pending | Rust CLI, Rust workspace, Python, Node, HTTP, generic command packs |
| TBD | Adoption pack docs | pending | user-facing intake and anti-pattern guidance |
| TBD | Product claims and canary freshness | pending | `docs/status/PRODUCT_CLAIMS.md`, `docs/status/CANARY_MATRIX.md` |
| TBD | External Rust canary | pending | existing benchmark repo import-to-review path |
| TBD | External non-Rust canary | pending | command/HTTP/import-to-review path |
| TBD | Final closeout | pending | handoff and archived active goal |

## Work item: implementation-plan

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: generic-command-json-adapter, hyperfine-json-adapter, criterion-adapter
Blocked by:

### Goal

Create the implementation sequence and active goal manifest for the 0.21
evidence intake and adoption packs lane.

### Production delta

Add:

```text
plans/0.21.0/evidence-intake-adoption-packs.md
.codex/goals/active.toml
```

Update proposal and spec headers to point at this concrete plan.

### Non-goals

- No adapter implementation.
- No public crate, receipt schema, release, tag, alias, or Action behavior
  change.
- No product-claim promotion before behavior exists.

### Acceptance

- Plan links proposal, spec, ADR, source-of-truth boundaries, PR sequence,
  proof commands, and rollback.
- `.codex/goals/active.toml` points at this lane.
- Proposal/spec plan pointers no longer say planned.
- Product claims remain unchanged.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert this plan, active goal, and proposal/spec pointer updates. The proposal
and evidence-source contract remain accepted artifacts.

## Work item: generic-command-json-adapter

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: hyperfine-json-adapter, custom-json-csv-mapping, imported-evidence-maturity
Blocked by: implementation-plan

### Goal

Add the lowest-risk intake path for user-controlled JSON evidence.

### Production delta

Add a CLI import surface for generic command JSON. The adapter should require
or preserve benchmark name, metric name, unit, direction, samples or summary,
source path, and host context where available.

### Acceptance

- Positive fixtures cover raw samples and summary-only input.
- Bad-input fixtures cover missing metric, ambiguous unit, ambiguous
  direction, missing host context, and invalid JSON.
- Imported metrics preserve direction-aware improvement/regression semantics.
- Output states what was inferred and what was not inferred.
- No baseline is promoted automatically.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 test -p perfgate-cli --all-features check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

### Rollback

Revert the adapter, fixtures, and docs. Existing perfgate command checks remain
unchanged.

## Work item: hyperfine-json-adapter

Status: merged
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: imported-evidence-maturity, adoption-pack-catalog
Blocked by: generic-command-json-adapter

### Goal

Import hyperfine JSON command-benchmark evidence with explicit command-timing
limits.

### Acceptance

- Mean, median, standard deviation, min, max, user time, system time, and raw
  runs are preserved where available.
- Command timing non-inferences are visible.
- Compile-heavy or setup-heavy examples remain advisory unless maturity proof
  supports promotion.
- Unit and direction mapping are fixture-backed.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 test -p perfgate-cli --all-features policy
git diff --check
```

## Work item: criterion-adapter

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: rust-adoption-pack, external-rust-canary
Blocked by: generic-command-json-adapter

### Goal

Import stable Criterion benchmark fields without overstating Criterion
statistics as perfgate maturity policy.

### Acceptance

- Benchmark identity, measured unit, sample information, and selected estimates
  are preserved where semantics are clear.
- Non-inferences explain what Criterion proves and what perfgate still needs
  to classify maturity or policy posture.
- Unsupported or ambiguous output fails with actionable guidance.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 test -p perfgate-cli --all-features doctor
git diff --check
```

## Work item: pytest-benchmark-json-adapter

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: python-adoption-pack, external-non-rust-canary
Blocked by: generic-command-json-adapter

### Goal

Import pytest-benchmark JSON while separating correctness test success from
performance evidence.

### Acceptance

- Runtime/interpreter and environment context are preserved where available.
- Fixture and host limitations are visible.
- Passing test status is not treated as performance maturity.
- Unit, direction, sample, and summary mapping are fixture-backed.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 test -p perfgate-cli --all-features doctor
git diff --check
```

## Work item: k6-summary-json-adapter

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: http-adoption-pack, external-non-rust-canary
Blocked by: generic-command-json-adapter

### Goal

Import k6 summary JSON as HTTP/load-test evidence with explicit environment
limits.

### Acceptance

- Request rate, latency summaries, error rate, and scenario labels are
  preserved where available.
- Local/shared-runner output is not described as production capacity proof.
- Output distinguishes smoke, advisory, and candidate policy evidence.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 test -p perfgate-cli --all-features policy
git diff --check
```

## Work item: custom-json-csv-mapping

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: generic-adoption-pack
Blocked by: generic-command-json-adapter

### Goal

Allow explicit custom JSON/CSV field mapping without silent metric ambiguity.

### Acceptance

- Mapping requires or derives metric name, value, unit, direction, sample
  identity, and host context where available.
- Ambiguous unit or direction mapping fails closed.
- CSV parsing errors are actionable and do not dump large payloads.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: imported-evidence-maturity

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: review-packet-action-posture, product-claims-canary-freshness
Blocked by: generic-command-json-adapter, hyperfine-json-adapter

### Goal

Make imported evidence visible to maturity and policy surfaces without changing
the advisory boundary.

### Acceptance

- Imported evidence can be explained by baseline doctor and signal doctor where
  supported.
- Policy doctor and emit-patch can consume imported evidence without making it
  blocking by default.
- Missing host, summary-only data, or limited noise support stays visible.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 test -p perfgate-cli --all-features doctor
cargo +1.95.0 test -p perfgate-cli --all-features policy
git diff --check
```

## Work item: review-packet-action-posture

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: external canaries
Blocked by: imported-evidence-maturity

### Goal

Surface imported evidence in review packets and GitHub Action summaries.

### Acceptance

- Review packets name source kind, source path, metric mapping, maturity
  limits, artifacts, local reproduction, and do-not guidance.
- Action summaries preserve existing verdict and reproduction behavior.
- Advisory imported evidence does not become blocking without configured
  policy.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features report
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
git diff --check
```

## Work item: adoption-pack-catalog

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: adoption-pack-docs, external canaries
Blocked by: generic-command-json-adapter, hyperfine-json-adapter

### Goal

Add reviewable adoption packs for common repo shapes.

### Pack set

```text
rust-cli
rust-workspace
python-service
node-tool-action
http-local-smoke
generic-command
```

### Acceptance

Each pack names benchmark source, expected artifact path, adapter command,
starting posture, known bad fits, Action snippet, local reproduction command,
baseline path, promotion path, and what not to infer.

### Proof commands

```bash
cargo +1.95.0 fmt --all -- --check
cargo +1.95.0 test -p perfgate-cli --all-features init
cargo +1.95.0 test -p perfgate-cli --all-features import
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Work item: adoption-pack-docs

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: product-claims-canary-freshness
Blocked by: adoption-pack-catalog

### Goal

Explain how teams keep existing benchmark tools while adding perfgate receipts,
maturity, policy, and review surfaces.

### Acceptance

- Docs include local reproduction and Action examples.
- Docs explain adapter non-inferences and bad fits.
- Docs keep benchmark selection reviewable.
- Docs do not claim public or universal support before proof exists.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

## Work item: product-claims-canary-freshness

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: external-rust-canary, external-non-rust-canary
Blocked by: adoption-pack-docs, review-packet-action-posture

### Goal

Map adapter and adoption-pack claims to proof without overclaiming support.

### Acceptance

- Product claims distinguish implemented adapters from planned ones.
- Canary matrix records source kind, repo shape, freshness, proof, and
  non-inferences.
- Source-built proof is not cited as public release proof.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
git diff --check
```

## Work item: external-rust-canary

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: final-closeout
Blocked by: adoption-pack-catalog, product-claims-canary-freshness

### Goal

Prove the intake-to-review path in a real Rust repo with an existing benchmark
source.

### Acceptance

The canary records repo shape, existing source, adapter command, imported
evidence artifact, baseline path, check/compare command, baseline doctor,
signal doctor, policy doctor, review packet, Action posture if hosted CI is in
scope, confusion/fixes, proof, non-inferences, and freshness.

## Work item: external-non-rust-canary

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks: final-closeout
Blocked by: adoption-pack-catalog, product-claims-canary-freshness

### Goal

Prove the intake-to-review path in a real non-Rust command, JSON/CSV, Python,
Node, k6, or HTTP repo.

### Acceptance

The canary records the same proof shape as the Rust canary and explicitly names
which non-Rust assumptions remain unproven.

## Work item: final-closeout

Status: pending
Linked proposal: docs/proposals/PERFGATE-PROP-0008-evidence-intake-adoption-packs.md
Linked spec: docs/specs/PERFGATE-SPEC-0013-evidence-source-contract.md
Blocks:
Blocked by: external-rust-canary, external-non-rust-canary

### Goal

Close the evidence intake lane with durable proof and non-inferences.

### Acceptance

- Handoff records which adapters exist and what each proves.
- It records which adoption packs exist and which repo shapes remain unproven.
- It records how imported evidence reaches maturity, policy, review, and
  Action surfaces.
- It records canary freshness and product-claim support.
- It archives `.codex/goals/active.toml`.
- It names the next recommended lane.

### Proof commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

### Rollback

Revert the closeout handoff and goal archive. Implemented adapter behavior
remains intact unless the closeout PR also changed status mappings.
