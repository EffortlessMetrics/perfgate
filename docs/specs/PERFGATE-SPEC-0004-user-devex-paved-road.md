# PERFGATE-SPEC-0004: User DevEx paved road

Status: accepted
Owner: perfgate maintainers
Created: 2026-05-13
Milestone: 0.18.0
Behavior version: user-devex-paved-road.v1
Product surface: install, doctor, init, check, baseline promotion, action setup
CI surface: first-run tests, baseline tests, doc-test, action-check
Schema impact: config examples and generated setup only
Action impact: generated GitHub Actions workflow and local reproduction guidance
Server impact: none for the local paved road
Linked proposal: docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md
Linked ADRs: none
Linked plan: plans/0.18.0/implementation-plan.md
Linked policy: release and action proof gates
Support/status impact: user-devex product claim planned
Proof commands: cargo +1.95.0 test -p perfgate-cli --all-features first_run; cargo +1.95.0 test -p perfgate-cli --all-features baseline; cargo +1.95.0 run -p xtask -- doc-test; cargo +1.95.0 run -p xtask -- action-check

## Problem

perfgate should be boring for new adopters. A user should not need to learn the
entire architecture before getting from installation to a checked benchmark and
a promotable baseline.

The repo already has first-run and baseline-bootstrap coverage. This spec turns
that path into a product contract so future UX, docs, and action changes do not
drift away from the supported setup.

## Behavior

The supported first-run paved road is:

```bash
cargo binstall perfgate-cli
perfgate doctor
perfgate init --ci github --profile standard
perfgate check --config perfgate.toml --all
perfgate baseline promote --config perfgate.toml --all
```

Generated setup SHOULD make the local path and GitHub Action path line up:

- `perfgate.toml` records the benchmark configuration;
- `.github/workflows/perfgate.yml` runs the configured gate;
- `baselines/` exists as the local baseline home;
- `.perfgate/README.md` explains generated project artifacts; and
- `doctor`, `check`, and `baseline promote` give the next useful command when
  setup is incomplete.

## Required UX

Every first-run failure SHOULD explain:

- what happened;
- why it matters;
- what command to run next;
- where artifacts were written; and
- how to reproduce locally.

The paved road MUST keep missing-baseline behavior explicit. The first check
may create trusted first-run artifacts, and promotion must be an explicit user
step.

## Non-goals

- This spec does not require a baseline server.
- This spec does not require users to enable decision mode on first run.
- This spec does not change generated workflow defaults.
- This spec does not change release or publish behavior.
- This spec does not make `cargo binstall` the only installation path.

## Required evidence

Changes to first-run setup, baseline bootstrap, generated action wiring, or
doctor guidance MUST run:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features first_run
cargo +1.95.0 test -p perfgate-cli --all-features baseline
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- action-check
```

Documentation-only changes to this spec SHOULD also run:

```bash
cargo +1.95.0 run -p xtask -- docs-check
git diff --check
```

## Acceptance examples

| Example | Result |
|---------|--------|
| `perfgate init --ci github --profile standard` writes local config, baseline directory, generated workflow, and `.perfgate/README.md`. | Pass |
| `perfgate doctor` points at the next setup command when config is missing. | Pass |
| `perfgate check --config perfgate.toml --all` writes artifacts that can be promoted. | Pass |
| `perfgate baseline promote --config perfgate.toml --all` promotes the first trusted local run. | Pass |
| First-run docs teach a server as mandatory. | Fail |
| A generated failure tells users only that the command failed, without next command or artifact location. | Fail |
| The generated action path diverges from the documented local command without an action-check update. | Fail |

## Test mapping

Current proof is mapped to:

- [`cli_first_run_e2e_tests.rs`](../../crates/perfgate-cli/tests/cli_first_run_e2e_tests.rs)
- [`cli_baseline_bootstrap_tests.rs`](../../crates/perfgate-cli/tests/cli_baseline_bootstrap_tests.rs)
- [`cli_init_tests.rs`](../../crates/perfgate-cli/tests/cli_init_tests.rs)
- [`cli_doctor_tests.rs`](../../crates/perfgate-cli/tests/cli_doctor_tests.rs)
- `cargo +1.95.0 run -p xtask -- doc-test`
- `cargo +1.95.0 run -p xtask -- action-check`

## Implementation mapping

The paved road is implemented or documented across:

- [`README.md`](../../README.md)
- [`docs/DEBUGGING_FIRST_CI_RUN.md`](../DEBUGGING_FIRST_CI_RUN.md)
- [`docs/GETTING_STARTED_GITHUB_ACTIONS.md`](../GETTING_STARTED_GITHUB_ACTIONS.md)
- `crates/perfgate-cli`
- the generated `.github/workflows/perfgate.yml` fixture path checked by tests
- the composite GitHub Action checked by `xtask action-check`

## CI proof

First-run UX or generated setup changes MUST include the narrow test filters:

```bash
cargo +1.95.0 test -p perfgate-cli --all-features first_run
cargo +1.95.0 test -p perfgate-cli --all-features baseline
```

Generated workflow or docs-example changes MUST also run:

```bash
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- action-check
```

## Promotion rule

This spec is accepted when merged as the documented first-run contract. It is
implemented when:

- the status proof map includes a user-devex paved-road claim;
- the 0.18.0 implementation plan identifies any remaining first-run follow-up;
- first-run and baseline tests cover the documented path; and
- action-check keeps the generated workflow aligned with local reproduction.
