# Decision Semantics Verification

Date: 2026-05-16
Status: passed
Scope: metric-direction semantics after the decision-semantics lane

## Purpose

Record the post-lane proof that perfgate interprets performance movement by
metric direction across the core decision surfaces before returning to the
operator-gated 0.18.0 release cutover.

This audit follows:

- `decision: detect higher-is-better improvements`
- `test: audit metric direction semantics`
- `domain: centralize metric movement semantics`
- `test: cover metric direction fixtures`
- `decision: harden tradeoff direction semantics`
- `docs: explain metric direction semantics`
- `docs(status): map metric direction semantics to proof`

## Verified Behavior

perfgate now has a shared domain movement vocabulary for core metric judgment:

- lower-is-better metrics improve when the current value goes down;
- higher-is-better metrics improve when the current value goes up;
- `pct` remains signed numeric movement;
- `regression` remains normalized positive worsening after metric direction;
- decision readiness and tradeoff requirements use direction-aware movement;
- probe comparison applies metric defaults and probe metric heuristics before
  producing normalized regression;
- product claims link direction-aware movement to docs and tests.

## Proof Commands

All commands below passed on 2026-05-16 from the current release-prep tree.
Cargo-heavy commands used `CARGO_TARGET_DIR=C:\perfgate-target-semantic-proof`
to keep repository-local artifacts disposable.

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

## Focused Evidence

The broad test run included these direction-specific fixtures:

- domain movement helper tests for lower-is-better and higher-is-better
  improvement/regression;
- compare fixtures where `throughput_per_s` improvement is positive `pct` and
  zero normalized regression;
- report fixtures preserving higher-is-better direction for throughput
  failures;
- probe fixtures where `*_per_s` movement is treated as higher-is-better;
- decision readiness fixtures for throughput improvement and regression;
- tradeoff fixtures for latency improving while throughput regresses;
- tradeoff fixtures for throughput improving while latency regresses;
- probe-backed tradeoff fixtures where dominant throughput improves while a
  local latency probe stays inside its regression cap.

## Non-Inferences

This audit does not publish 0.18.0, create tags, move action aliases, or close
the active release cutover goal.

This audit does not claim every display surface has been converted away from
raw signed `pct`. The metric-direction audit still tracks follow-ups where raw
display is intentional or where naming/language should be tightened, including
trend arrows and export column naming.

This audit does not make the server ledger part of correctness. Local receipts
remain the primary correctness contract; the server ledger remains optional
team history.

## Next Step

Resume the operator-gated 0.18.0 release cutover from the existing active goal:
publish crates only with explicit operator approval, verify crates.io, cut the
GitHub release, move action aliases intentionally, run public install smoke, and
close publication with a committed release audit.
