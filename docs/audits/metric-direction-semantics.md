# Metric Direction Semantics Audit

Date: 2026-05-16

Branch: `test/metric-direction-semantics-audit`

Purpose: identify where perfgate interprets metric movement as improvement,
regression, or neutral movement, and separate direction-aware decision logic
from raw percentage display. This audit follows the higher-is-better
`decision suggest` fix and does not publish crates, move release tags, or close
the active 0.18 release cutover goal.

Linked specs:
[`PERFGATE-SPEC-0003`](../specs/PERFGATE-SPEC-0003-performance-decision-contract.md),
[`PERFGATE-SPEC-0008`](../specs/PERFGATE-SPEC-0008-first-use-ux-contract.md)

Linked ADRs:
[`PERFGATE-ADR-0002`](../adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md)

Linked plan: active 0.18 release cutover remains operator-gated.

## Rule

Metric movement semantics must be interpreted through metric direction, not by
raw delta sign alone:

| Metric family | Better direction | Improvement | Regression |
| --- | --- | --- | --- |
| `wall_ms`, `cpu_ms`, `max_rss_kb`, `page_faults`, `ctx_switches` | lower | negative `pct` | positive `pct` |
| `throughput_per_s`, `*_per_s`, probe throughput/rate/count heuristics | higher | positive `pct` | negative `pct` |

Raw `pct` is still valid as a signed display value when the output labels it as
change rather than judgment. Any output that calls a movement an improvement,
regression, warning, failure, accepted tradeoff, or decision candidate must use
direction-aware semantics or already-normalized receipt fields such as
`Delta::regression` and `MetricStatus`.

## Audited Surfaces

| Surface | Current source | Current state | Follow-up |
| --- | --- | --- | --- |
| Compare receipt verdict | `crates/perfgate/src/domain/budget.rs`, `crates/perfgate/src/domain/comparison.rs` | Direction-aware through `calculate_regression` and budget direction. | Centralize movement vocabulary so callers do not need to know regression math. |
| Report derivation | `crates/perfgate/src/app/report.rs` | Uses `Delta::regression` and `MetricStatus`; no raw sign judgment found in report construction. | Add higher-is-better report fixture in the fixture matrix PR. |
| Decision readiness | `crates/perfgate-cli/src/decision_suggest.rs` | Direction-aware after the higher-is-better fix. | Replace local match with shared movement helper once it exists. |
| Tradeoff requirements | `crates/perfgate/src/domain/comparison.rs`, `crates/perfgate/src/app/tradeoff.rs` | Direction-aware `improvement_ratio` helpers translate lower-is-better and higher-is-better metrics into comparable improvement ratios. | Deduplicate the two helper implementations through the shared domain helper. |
| Tradeoff allowances | `crates/perfgate/src/app/tradeoff.rs` | Uses `Delta::regression`, which is already positive normalized regression. | Add probe-backed higher-is-better allowance fixtures. |
| Probe compare | `crates/perfgate/src/app/probe.rs` | Direction-aware through parsed metric defaults and probe metric heuristics for throughput/rate/count names. | Add fixture coverage for custom higher-is-better probe metrics. |
| Repair context | `crates/perfgate-cli/src/repair_context.rs` | Uses non-pass `MetricStatus` and `Delta::regression`; no raw sign judgment found. | Add higher-is-better repair context fixture in the fixture matrix PR. |
| Badge/status surfaces | `crates/perfgate/src/app/badge.rs` | Verdict and metric status are status-driven; raw `pct` is display-only. | Keep display labels as change, not judgment. |
| Markdown rendering and annotations | `crates/perfgate/src/app/render.rs`, `crates/perfgate/src/integrations/github/comment.rs` | Status-driven warnings/failures are correct, but `trend_indicator(delta.pct)` still uses raw sign for arrow language. | Route trend indicators through shared movement semantics. |
| Watch/trend display | `crates/perfgate/src/app/watch.rs`, `crates/perfgate/src/app/trend.rs` | `app/trend.rs` is direction-aware; `watch.rs` stores raw `pct` history and classifies positive average as worsening. | Make watch trends metric-aware or restrict wording to lower-is-better deltas. |
| Export rows | `crates/perfgate/src/app/export/rows.rs` | `regression_pct` currently exports raw signed `delta.pct * 100.0`, despite the column name implying normalized regression. | Decide whether to rename the column or export normalized `Delta::regression`. |
| Decision bundles | `crates/perfgate-cli/src/main.rs` | Bundles preserve receipts and do not reinterpret metric movement. | No immediate semantic change; fixture matrix should include bundled higher-is-better receipts. |

## Focused Test Added

This audit adds domain tests for the existing tradeoff `improvement_ratio`
helper:

- lower-is-better decrease is an improvement ratio above `1.0`;
- higher-is-better increase is an improvement ratio above `1.0`;
- higher-is-better decrease is below `1.0` and therefore cannot satisfy a
  `min_improvement_ratio` requirement.

## Follow-up PRs

1. Add a shared movement model such as `MetricMovement::{Improved, Regressed, Unchanged, Unknown}` in the domain layer.
2. Replace local raw-sign improvement checks with the shared helper.
3. Add fixture coverage for lower-is-better and higher-is-better metrics across compare, report, decision suggest, tradeoff, probe compare, repair context, comments, and bundles.
4. Harden tradeoff and probe requirement tests for higher-is-better dominant improvements and lower-is-better accepted local regressions.
5. Update product claims only after the fixture matrix covers the core user-facing surfaces.

## Proof Commands

```bash
cargo +1.95.0 test -p perfgate --all-features domain::comparison
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```
