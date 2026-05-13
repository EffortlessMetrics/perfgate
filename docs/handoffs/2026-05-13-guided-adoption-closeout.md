# Guided Adoption Closeout

Status: implemented
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked specs: docs/specs/PERFGATE-SPEC-0007-guided-adoption-contract.md; docs/specs/PERFGATE-SPEC-0006-policy-ledger-contracts.md
Linked ADRs: docs/adr/PERFGATE-ADR-0002-receipts-first-performance-decisions.md; docs/adr/PERFGATE-ADR-0001-public-crates-are-contracts.md
Linked plan: plans/0.18.0/guided-adoption.md; plans/0.18.0/wrapper-crate-cleanup.md
Linked policy: policy/public_crates.txt; policy/absorbed_crates.txt; policy/no-panic-baseline.toml
Support/status impact: docs/status/PRODUCT_CLAIMS.md PG-CLAIM-0009 through PG-CLAIM-0012; PG-CLAIM-0006 refreshed
Proof commands: cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; git diff --check

## What changed

The guided adoption lane moved perfgate from a powerful maintainer tool toward
a guided performance-decision product. The merged work now gives cold users and
teams a staged path:

```text
install -> doctor -> init -> check -> promote -> CI -> structured decision -> probes -> optional ledger
```

The lane added or refreshed:

- guided adoption proposal, spec, implementation plan, and archived goal rails;
- first-hour adoption guide and executable smoke proof;
- adoption-level guide for local gate, GitHub Action gate, structured
  decisions, and server ledger;
- decision outcome gallery for pass, fail, accepted tradeoff, review-required,
  missing evidence, and high-noise outcomes;
- probe instrumentation quickstart and probe-to-decision proof;
- GitHub Action failure summaries with local reproduction, verdict counts,
  review-required context, missing-baseline hints, and artifact lists;
- decision ledger operations runbook;
- guided adoption product claims and claim/spec freshness enforcement;
- policy-ledger contract spec;
- wrapper-crate cleanup plan that preserves the five public crates.

## Product claims

The lane added support/proof coverage for:

- `PG-CLAIM-0009`: first-hour local adoption path;
- `PG-CLAIM-0010`: staged adoption levels;
- `PG-CLAIM-0011`: probe-backed tradeoff explanation;
- `PG-CLAIM-0012`: optional team decision-ledger operations.

It also refreshed `PG-CLAIM-0006` so policy-ledger governance links to the
concrete policy-ledger contract spec instead of a planned placeholder.

## Proof recorded

The lane PRs ran the relevant proof for each slice, including:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
cargo +1.95.0 run -p xtask -- action-check
cargo +1.95.0 run -p xtask -- schema-compat
cargo +1.95.0 run -p xtask -- public-surface --strict
cargo +1.95.0 run -p xtask -- arch
cargo +1.95.0 run -p xtask -- policy check-no-panic-family
cargo +1.95.0 test -p perfgate-cli --all-features first_run
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 test -p perfgate-cli --all-features probe
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 test -p xtask --all-features
git diff --check
```

The closeout PR reruns the docs/source/status checks and whitespace check.

## User paths now supported

A new user can follow `docs/FIRST_HOUR.md` to install, initialize, run a local
gate, promote a baseline, understand what to commit, wire CI, and reproduce a
failure locally.

A reviewer can use `docs/examples/decision-outcomes.md` to recognize common
structured-decision outputs and decide whether to merge, reject, request
review, or ask for better evidence.

A team can use `docs/DECISION_LEDGER_RUNBOOK.md` to operate optional ledger
mode without making server upload a prerequisite for local correctness.

An agent can read the proposal, spec, plan, product claims, policy ledgers, and
this handoff to understand the lane without chat history.

## Deferred work

The remaining work is intentionally follow-on, not part of the closed guided
adoption lane:

- wrapper absorption is complete and closed out in
  [`2026-05-13-wrapper-absorption-closeout.md`](2026-05-13-wrapper-absorption-closeout.md);
- keep public crates limited to the five-crate surface unless a future ADR and
  policy update justify a change;
- deepen external/public install smoke as release-candidate automation;
- continue platform support cleanup, including Windows metric and timeout
  boundaries, through status docs and targeted implementation PRs;
- add stronger semantic validation for policy-ledger metadata only after each
  ledger has an explicit migration plan.

## Archive

The active goal manifest is archived at
`.codex/goals/archive/perfgate-guided-adoption.toml`. There is no active
guided-adoption work item after this closeout.
