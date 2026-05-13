# PERFGATE-SPEC-0007: Guided adoption contract

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: guided-adoption-contract.v1
Product surface: first-hour UX, GitHub Action gate, structured decisions, probe evidence, optional server ledger
CI surface: doc-test, action-check, first-run tests, probe tests, decision tests, product-claims-check
Schema impact: no new schema version; examples and tests may exercise existing receipt schemas
Action impact: action summaries and failure copy must surface local reproduction and decision artifacts
Server impact: server ledger remains optional team infrastructure; local receipts remain primary
Linked proposal: docs/proposals/PERFGATE-PROP-0002-guided-adoption.md
Linked ADRs: PERFGATE-ADR-0002-receipts-first-performance-decisions, PERFGATE-ADR-0003-local-receipts-first-server-ledger-optional
Linked plan: plans/0.18.0/guided-adoption.md
Linked policy: policy ledgers remain source of truth for governed exceptions and public surface
Support/status impact: guided-adoption product claims planned in docs/status/PRODUCT_CLAIMS.md
Proof commands: cargo +1.95.0 run -p xtask -- docs-check; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- docs-source-check; cargo +1.95.0 run -p xtask -- product-claims-check; git diff --check

## Problem

perfgate already has the primitives required for useful performance decisions:
local checks, baselines, probes, scenarios, tradeoffs, decision receipts,
portable bundles, GitHub Action integration, and an optional server ledger.

The adoption contract must make those primitives approachable without forcing a
new user to understand the full system. A user should be able to stop at the
local gate and still get value, then opt into CI, structured decisions, probes,
and ledger history as the review question becomes more advanced.

This spec defines what must be true for guided adoption so docs, tests, action
output, examples, product claims, and future agents all point at the same
receipt-backed path.

## Behavior

perfgate MUST support a progressive adoption ladder:

| Level | Name | User question |
|-------|------|---------------|
| 1 | Local gate | Did this local change regress a benchmark? |
| 2 | GitHub Action gate | Can CI reproduce and explain the same gate? |
| 3 | Structured decision | Did this local regression buy a larger workload improvement? |
| 4 | Server ledger | What performance debt are we accepting over time? |

Each level MUST define:

- who it is for;
- commands;
- required config;
- generated or durable files;
- artifacts;
- failure example;
- local reproduction command;
- next level; and
- proof commands or linked gates.

The levels MUST be independently useful. Level 1 MUST NOT require the GitHub
Action, structured decisions, probes, or server ledger. Level 2 MUST NOT
require structured decisions. Level 3 MUST NOT require server upload. Level 4
MUST consume local decision receipts instead of inventing a different decision
model.

## Level 1: Local gate

The local gate MUST support this first-hour path:

```bash
cargo binstall perfgate-cli
perfgate doctor
perfgate init --ci github --profile standard
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
```

The path MUST explain:

- what files were created;
- what to commit;
- what not to commit;
- where artifacts live;
- what a first missing-baseline result means;
- how to rerun with `--require-baseline`; and
- how to promote a trusted baseline.

The local gate MUST keep baseline promotion explicit. perfgate MUST NOT silently
invent a durable baseline from a first run.

## Level 2: GitHub Action gate

The GitHub Action gate MUST run the same checked-in policy that users can
reproduce locally.

At minimum, action-facing docs and output SHOULD identify:

- verdict;
- failed metric or failed setup condition;
- artifact paths;
- uploaded artifact list when upload is enabled;
- local reproduction command;
- baseline promotion hint when no baseline exists; and
- decision evaluation command when decision mode is enabled.

For the standard generated workflow, the reproduction command is:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

The action MUST NOT make the server ledger mandatory for ordinary branch
protection.

## Level 3: Structured decision

Structured decisions MUST explain the review question:

```text
what moved, where did it move, what workload matters, and does policy accept it?
```

The normal structured decision path is:

```bash
perfgate check --config perfgate.toml --all --require-baseline
perfgate ingest probes --file artifacts/probes.jsonl --out artifacts/perfgate/probes.json
perfgate decision evaluate --config perfgate.toml
perfgate decision bundle --index artifacts/perfgate/decision.index.json --out artifacts/perfgate/decision-bundle.json
```

Structured decision docs and examples MUST name the core review artifacts:

- `compare.json`;
- `probe-compare.json` when probe evidence is configured;
- `scenario.json`;
- `tradeoff.json`;
- `decision.md`;
- `decision.index.json`; and
- `decision-bundle.json` when exported.

Decision examples SHOULD cover:

- pass;
- fail;
- warn with accepted tradeoff;
- review-required;
- missing evidence; and
- high noise.

Probe guidance MUST frame probes as tradeoff lenses, not as profiling. Probes
SHOULD show where work moved inside a benchmark and SHOULD remain optional for
the basic local gate.

## Level 4: Server ledger

The server ledger MUST remain optional. It is team-scale history and audit
infrastructure, not a prerequisite for local correctness.

Server-ledger adoption docs SHOULD cover:

- local SQLite mode;
- team server mode;
- API key creation, rotation, and revocation;
- decision upload;
- history and latest lookup;
- export;
- prune dry-run and force behavior;
- debt summaries;
- audit events;
- backup expectations;
- dashboard expectations; and
- CI upload failure behavior.

CI upload failure MUST be distinguishable from local decision correctness. A
team MAY choose to make upload required by policy, but perfgate's product
contract remains local receipts first.

## Non-goals

- This spec does not add a new receipt schema version.
- This spec does not require probes for local checks.
- This spec does not make the server mandatory.
- This spec does not change the five public crates.
- This spec does not publish crates, create tags, or create GitHub releases.
- This spec does not duplicate policy ledger rows or release-readiness
  matrices.
- This spec does not require a full documentation graph checker.

## Required evidence

Documentation-only changes to this contract SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
git diff --check
```

First-hour behavior changes SHOULD run the first-run and baseline filters:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features first_run
cargo +1.95.0 test -p perfgate-cli --all-features baseline
```

Action failure-copy or decision-mode action changes SHOULD run:

```bash
cargo +1.95.0 run -p xtask -- action-check
```

Probe-to-decision changes SHOULD run:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features probe
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
```

Server-ledger behavior changes SHOULD run relevant decision/server tests and
schema compatibility:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features decision
cargo +1.95.0 run -p xtask -- schema-compat
```

## Acceptance examples

| Example | Result |
|---------|--------|
| A user can stop at Level 1 with local baselines and still get a useful budget gate. | Pass |
| The GitHub Action failure output includes the local `perfgate check --config perfgate.toml --all --require-baseline` reproduction command. | Pass |
| A structured decision example names `decision.md`, `decision.index.json`, and the receipts that fed it. | Pass |
| A probe quickstart shows JSONL ingest and decision wiring without requiring a server. | Pass |
| Server-ledger docs describe upload, history, export, prune, debt, and audit while preserving local correctness. | Pass |
| A user must configure server ledger before `perfgate check` works. | Fail |
| A structured decision example describes the verdict but omits artifact names and local reproduction. | Fail |
| A CI failure says only "failed" without artifact location or reproduction command. | Fail |
| Probe docs teach one event per function or span as the default path. | Fail |
| A product claim says the first-hour path is supported but has no linked proof commands. | Fail |

## Test mapping

Current and planned proof maps to:

- [`docs/FIRST_HOUR.md`](../FIRST_HOUR.md)
- [`docs/ADOPTION_LEVELS.md`](../ADOPTION_LEVELS.md)
- [`docs/PERFORMANCE_DECISIONS.md`](../PERFORMANCE_DECISIONS.md)
- [`examples/performance-decision`](../../examples/performance-decision)
- [`cli_first_run_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_first_run_e2e_tests.rs)
- [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs)
- [`cli_probe_tests.rs`](../../crates/perfgate-cli/tests/cli_probe_tests.rs)
- [`cli_structured_decision_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_structured_decision_e2e_tests.rs)
- [`cli_performance_decision_example_tests.rs`](../../crates/perfgate-cli/tests/cli_performance_decision_example_tests.rs)
- [`cli_server_tests.rs`](../../crates/perfgate-cli/tests/cli_server_tests.rs)
- `cargo +1.95.0 run -p xtask -- action-check`
- `cargo +1.95.0 run -p xtask -- schema-compat`
- `cargo +1.95.0 run -p xtask -- product-claims-check`

Follow-on PRs SHOULD add or strengthen proof for:

- generated cold project first-hour smoke;
- decision outcome examples;
- probe helper or JSONL to decision workflow;
- action failure summary and reproduction copy; and
- server-ledger operations runbook coverage.

## Implementation mapping

The guided adoption contract is owned by:

- CLI docs and examples under `docs/`;
- deterministic examples under `examples/`;
- CLI integration tests under `crates/perfgate-cli/tests/`;
- the composite action and `xtask action-check`;
- status claims in `docs/status/PRODUCT_CLAIMS.md`; and
- the guided adoption plan and active goal manifest once added.

This spec links to policy ledgers only when a governed surface or exception is
involved. Concrete policy rows remain owned by `policy/`.

## CI proof

Guided adoption changes MUST select proof commands by affected surface:

| Surface | Proof |
|---------|-------|
| Docs, specs, plans, status | `docs-check`, `doc-test`, `docs-source-check`, `product-claims-check`, `git diff --check` |
| First-hour CLI path | `cargo +1.95.0 test -p perfgate-cli --all-features first_run`, `baseline` |
| Action output | `cargo +1.95.0 run -p xtask -- action-check` |
| Probe and decision workflow | `cargo +1.95.0 test -p perfgate-cli --all-features probe`, `decision`, `schema-compat` |
| Server ledger | decision/server tests and `schema-compat` |

## Promotion rule

This spec is accepted when merged as the guided adoption behavior contract. It
is implemented when:

- the guided adoption implementation plan and active goal manifest exist;
- first-hour, adoption-level, probe, decision outcome, and server-ledger docs
  are present and linked;
- first-hour smoke or equivalent proof exists;
- probe-to-decision proof exists;
- action failure copy surfaces reproduction and artifacts;
- product claims cover the guided adoption surfaces with proof commands; and
- the guided adoption closeout handoff archives the active goal and records
  remaining deferred work.
