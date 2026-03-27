# Testing Guide for perfgate

This document describes the testing strategy, how to run each test type, and provides examples of well-written tests for the perfgate project.

## Table of Contents

- [Testing Strategy Overview](#testing-strategy-overview)
- [Running Tests](#running-tests)
  - [Unit Tests](#unit-tests)
  - [BDD Tests](#bdd-tests)
  - [Property-Based Tests](#property-based-tests)
  - [Fuzz Tests](#fuzz-tests)
  - [Mutation Tests](#mutation-tests)
  - [Integration Tests](#integration-tests)
- [Test Examples](#test-examples)
  - [Unit Test Example](#unit-test-example)
  - [BDD Scenario Example](#bdd-scenario-example)
  - [Property-Based Test Example](#property-based-test-example)
  - [Fuzz Target Example](#fuzz-target-example)
- [Property-Based Testing Patterns](#property-based-testing-patterns)
- [Mutation Testing Coverage Targets](#mutation-testing-coverage-targets)
- [Writing New Tests](#writing-new-tests)

## Testing Strategy Overview

perfgate employs a multi-layered testing strategy following the test pyramid:

```
                    ┌─────────────────┐
                    │   BDD Tests     │  User-facing behavior
                    │   (Cucumber)    │  Living documentation
                    ├─────────────────┤
                    │  Integration    │  Cross-crate workflows
                    │     Tests       │  End-to-end pipelines
                    ├─────────────────┤
                    │ Property Tests  │  Algorithmic correctness
                    │   (proptest)    │  Universal properties
                    ├─────────────────┤
                    │   Unit Tests    │  Individual functions
                    │                 │  Edge cases & errors
                    ├─────────────────┤
                    │   Fuzz Tests    │  Robustness testing
                    │  (cargo-fuzz)   │  Malformed inputs
                    └─────────────────┘
```

### Test Types by Crate (26 Workspace Crates)

| Crate | Unit Tests | Property Tests | BDD Coverage | Fuzz Targets |
|-------|------------|----------------|--------------|--------------|
| perfgate-types | Serialization | Round-trip | N/A | `parse_run_receipt`, `parse_compare_receipt`, `parse_config` |
| perfgate-domain | Logic, Errors | Stats & Comparison | N/A | `compare_stats` |
| perfgate-adapters | Mock-based | Output truncation | N/A | N/A |
| perfgate-app | Use-cases | Orchestration | N/A | `render_markdown` |
| perfgate-cli | N/A | N/A | Full command coverage | N/A |
| perfgate-server | API handlers | N/A | Baseline management | N/A |
| perfgate-client | Client logic | N/A | Remote workflows | N/A |

## Running Tests

### Unit Tests

Run all unit tests across the workspace:

```bash
cargo test --all
```

Run tests for a specific crate:

```bash
cargo test -p perfgate-domain
cargo test -p perfgate-types
cargo test -p perfgate-app
```

### BDD Tests

BDD tests use the [cucumber](https://github.com/cucumber-rs/cucumber) crate with Gherkin feature files located in the `features/` directory.

Run all BDD tests:

```bash
cargo test --test cucumber
```

Feature files:
- `features/run_command.feature` - Scenarios for `perfgate run`
- `features/compare_command.feature` - Scenarios for `perfgate compare`
- `features/check_command.feature` - Orchestrated `check` workflows
- `features/aggregate_command.feature` - Fleet aggregation logic
- `features/bisect_command.feature` - Automated performance bisection
- `features/blame.feature` - Dependency change analysis
- `features/explain_command.feature` - AI-ready diagnostic prompts
- `features/auth.feature` - OIDC and API key authentication
- `features/baseline_command.feature` - Server-side baseline management
- `features/template_hub.feature` - Markdown template rendering

### Property-Based Tests

Property-based tests are included in the unit test suite and use [proptest](https://proptest-rs.github.io/proptest/).

Run property tests (included in unit tests):

```bash
cargo test --all
```

### Fuzz Tests

Fuzzing requires the nightly Rust toolchain and [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html).

**Run a fuzz target:**

```bash
cargo fuzz run parse_run_receipt
cargo fuzz run compare_stats
```

### Mutation Tests

Mutation testing uses [cargo-mutants](https://mutants.rs/) to verify test effectiveness.

**Run mutation testing via xtask (recommended):**

```bash
# Run on all configured crates
cargo run -p xtask -- mutants

# Run on a specific crate
cargo run -p xtask -- mutants --crate perfgate-domain
```

## Integration Tests

The `tests/integration` directory contains end-to-end tests verifying cross-crate workflows:

| Test File | Coverage |
|-----------|----------|
| `full_pipeline_flow.rs` | Complete run-compare-report-md pipeline |
| `budget_significance_flow.rs` | Interaction between budgets and Welch's t-test |
| `cross_crate_pipeline.rs` | Integration between domain, app, and adapters |
| `export_render_flow.rs` | Data export followed by Markdown rendering |
| `sensor_flow.rs` | Cockpit mode sensor report generation |
| `host_detect_to_app.rs` | Fingerprinting logic integrated with use-cases |
| `validation_to_types.rs` | Schema validation against core types |

## CI Integration

Tests are automatically run in CI:

- **Every PR**: Unit tests, property tests, BDD tests, short fuzz session (60s/target)
- **Weekly**: Full mutation testing run
- **Coverage**: 80% line coverage minimum enforced

See `.github/workflows/ci.yml` for the complete CI configuration.
