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
                    │  Integration    │  CLI command testing
                    │     Tests       │  End-to-end workflows
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

### Test Types by Crate

| Crate | Unit Tests | Property Tests | BDD Coverage | Fuzz Targets |
|-------|------------|----------------|--------------|--------------|
| perfgate-types | Serialization examples | Round-trip properties | N/A | `parse_run_receipt`, `parse_compare_receipt`, `parse_config` |
| perfgate-domain | Edge cases, errors | Statistics & comparison properties | N/A | `compare_stats` |
| perfgate-adapters | Mock-based tests | Output truncation property | N/A | N/A |
| perfgate-app | Rendering examples | Markdown & annotation properties | N/A | `render_markdown` |
| perfgate-cli | N/A | N/A | Full command coverage | N/A |

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

Run a specific test by name:

```bash
cargo test summarize_u64_median_even_rounds_down
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
- `features/md_command.feature` - Scenarios for `perfgate md`
- `features/annotations_command.feature` - Scenarios for `perfgate github-annotations`

### Property-Based Tests

Property-based tests are included in the unit test suite and use [proptest](https://proptest-rs.github.io/proptest/).

Run property tests (included in unit tests):

```bash
cargo test --all
```

Property tests are configured to run at least 100 iterations per property. In CI, a fixed seed is used for reproducibility.

### Fuzz Tests

Fuzzing requires the nightly Rust toolchain and [cargo-fuzz](https://rust-fuzz.github.io/book/cargo-fuzz.html).

**Setup:**

```bash
rustup toolchain install nightly
cargo +nightly install cargo-fuzz
```

**List available fuzz targets:**

```bash
cargo fuzz list
```

**Run a fuzz target:**

```bash
cargo fuzz run parse_run_receipt
cargo fuzz run parse_compare_receipt
cargo fuzz run parse_config
cargo fuzz run parse_duration
cargo fuzz run compare_stats
cargo fuzz run render_markdown
```

**Recommended fuzzing durations:**
- **CI (PR)**: 60 seconds per target
- **Scheduled**: 10 minutes per target (weekly)
- **Local development**: As needed

### Mutation Tests

Mutation testing uses [cargo-mutants](https://mutants.rs/) to verify test effectiveness.

**Setup:**

```bash
cargo install cargo-mutants
```

**Run mutation testing via xtask (recommended):**

```bash
# Run on all configured crates
cargo run -p xtask -- mutants

# Run on a specific crate
cargo run -p xtask -- mutants --crate perfgate-domain

# Run with summary report
cargo run -p xtask -- mutants --crate perfgate-domain --summary
```

**Run directly with cargo-mutants:**

```bash
cargo mutants --package perfgate-domain
```

For detailed mutation testing guidance, see [docs/MUTATION_TESTING.md](docs/MUTATION_TESTING.md).

## Test Examples

### Unit Test Example

Unit tests verify specific examples and edge cases:

```rust
// crates/perfgate-domain/src/lib.rs
#[test]
fn summarize_u64_median_even_rounds_down() {
    let s = summarize_u64(&[10, 20]).unwrap();
    assert_eq!(s.median, 15);
}

#[test]
fn summarize_u64_empty_returns_error() {
    let result = summarize_u64(&[]);
    assert!(matches!(result, Err(DomainError::NoSamples)));
}
```

**Best practices:**
- Use descriptive test names that explain what is being tested
- Test both success and error cases
- Test boundary conditions (empty input, single element, etc.)

### BDD Scenario Example

BDD scenarios document user-facing behavior in Gherkin syntax:

```gherkin
# features/compare_command.feature
Feature: Compare Command
  As a CI pipeline
  I want to compare benchmark results against baselines
  So that I can detect performance regressions

  Background:
    Given a temporary directory for test artifacts

  Scenario: Pass verdict when performance improves
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 900
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 0
    And the verdict should be pass
    And the compare receipt should contain wall_ms delta

  Scenario: Fail verdict when regression exceeds threshold
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1500
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 2
    And the verdict should be fail
    And the reasons should mention regression percentage
```

**Best practices:**
- Use the Background section for common setup
- Write scenarios from the user's perspective
- Include both happy path and error scenarios
- Document exit codes and expected outputs

### Property-Based Test Example

Property tests verify universal properties across all valid inputs:

```rust
// crates/perfgate-domain/src/lib.rs
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,
        ..ProptestConfig::default()
    })]

    /// **Validates: Requirements 4.6**
    ///
    /// Property 2: Statistics Ordering Invariant
    ///
    /// For any non-empty list of finite f64 values, the computed summary
    /// SHALL satisfy: min <= median <= max
    #[test]
    fn prop_summarize_f64_ordering(
        values in prop::collection::vec(finite_f64_strategy(), 1..100)
    ) {
        let summary = summarize_f64(&values).expect("non-empty vec should succeed");

        prop_assert!(
            summary.min <= summary.median,
            "min ({}) should be <= median ({})",
            summary.min, summary.median
        );
        prop_assert!(
            summary.median <= summary.max,
            "median ({}) should be <= max ({})",
            summary.median, summary.max
        );
    }
}
```

**Best practices:**
- Reference the requirement being validated in a doc comment
- Use descriptive property names
- Configure at least 100 test cases
- Use custom strategies to generate valid inputs

### Fuzz Target Example

Fuzz targets test robustness against arbitrary inputs:

```rust
// fuzz/fuzz_targets/compare_stats.rs
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;

#[derive(Arbitrary, Debug)]
struct CompareStatsInput {
    baseline: FuzzStats,
    current: FuzzStats,
    budget_entries: Vec<FuzzBudgetEntry>,
}

fuzz_target!(|input: CompareStatsInput| {
    let baseline = input.baseline.to_perfgate();
    let current = input.current.to_perfgate();

    let mut budgets: BTreeMap<perfgate_types::Metric, perfgate_types::Budget> = BTreeMap::new();
    for entry in input.budget_entries.iter().take(3) {
        budgets.insert(entry.metric.to_perfgate(), entry.budget.to_perfgate());
    }

    // Should never panic regardless of input
    let _ = perfgate_domain::compare_stats(&baseline, &current, &budgets);
});
```

**Best practices:**
- Use structure-aware fuzzing with `Arbitrary` trait for complex types
- Ensure invariants are maintained when converting fuzz input to domain types
- The target should never panic - errors are acceptable

## Property-Based Testing Patterns

### Pattern 1: Round-Trip Serialization

Verify that serializing and deserializing produces equivalent values:

```rust
proptest! {
    #[test]
    fn run_receipt_serialization_round_trip(receipt in run_receipt_strategy()) {
        let json = serde_json::to_string(&receipt)
            .expect("should serialize");
        let deserialized: RunReceipt = serde_json::from_str(&json)
            .expect("should deserialize");
        
        prop_assert_eq!(&receipt.schema, &deserialized.schema);
        // ... compare other fields
    }
}
```

### Pattern 2: Ordering Invariants

Verify mathematical invariants hold for all inputs:

```rust
proptest! {
    #[test]
    fn prop_summarize_ordering(values in prop::collection::vec(any::<u64>(), 1..100)) {
        let summary = summarize_u64(&values).unwrap();
        
        prop_assert!(summary.min <= summary.median);
        prop_assert!(summary.median <= summary.max);
    }
}
```

### Pattern 3: Reference Implementation Comparison

Compare against a known-correct (but possibly slower) implementation:

```rust
/// Reference implementation using u128 to avoid overflow
fn reference_median_u64(sorted: &[u64]) -> u64 {
    let n = sorted.len();
    let mid = n / 2;
    if n % 2 == 1 {
        sorted[mid]
    } else {
        let a = sorted[mid - 1] as u128;
        let b = sorted[mid] as u128;
        ((a + b) / 2) as u64
    }
}

proptest! {
    #[test]
    fn prop_median_matches_reference(values in prop::collection::vec(large_u64_strategy(), 2..50)) {
        let summary = summarize_u64(&values).unwrap();
        let mut sorted = values.clone();
        sorted.sort_unstable();
        
        prop_assert_eq!(summary.median, reference_median_u64(&sorted));
    }
}
```

### Pattern 4: Custom Strategies

Create strategies that generate valid domain objects:

```rust
/// Strategy for generating valid Stats with consistent invariants
fn stats_strategy() -> impl Strategy<Value = Stats> {
    (
        u64_summary_strategy(),
        proptest::option::of(u64_summary_strategy()),
        proptest::option::of(f64_summary_strategy()),
    )
        .prop_map(|(wall_ms, max_rss_kb, throughput_per_s)| Stats {
            wall_ms,
            max_rss_kb,
            throughput_per_s,
        })
}

/// Strategy for U64Summary ensuring min <= median <= max
fn u64_summary_strategy() -> impl Strategy<Value = U64Summary> {
    (0u64..1000000, 0u64..1000000, 0u64..1000000).prop_map(|(a, b, c)| {
        let mut vals = [a, b, c];
        vals.sort();
        U64Summary {
            min: vals[0],
            median: vals[1],
            max: vals[2],
        }
    })
}
```

## Mutation Testing Coverage Targets

| Crate | Target Kill Rate | Rationale |
|-------|------------------|-----------|
| perfgate-domain | 100% | Pure logic, fully testable |
| perfgate-types | 95% | Serialization logic, some derive macros |
| perfgate-app | 90% | Rendering logic, some formatting edge cases |
| perfgate-adapters | 80% | Platform-specific code, harder to test |
| perfgate-cli | 70% | I/O heavy, integration tested instead |

### Addressing Surviving Mutants

When mutants survive, review `mutants.out/missed.txt` to identify:

1. **Missing test coverage**: Add tests for the uncovered code path
2. **Weak assertions**: Strengthen assertions to detect the mutation
3. **Equivalent mutants**: Some mutations don't change behavior (rare)

Example surviving mutant:
```
replace summarize_u64 -> Option<Summary<u64>> with None in crates/perfgate-domain/src/lib.rs
```

This indicates a test should verify that `summarize_u64` returns `Some(...)` for valid inputs.

## Writing New Tests

When adding new functionality, follow this checklist:

### 1. Unit Tests
- [ ] Test the happy path with typical inputs
- [ ] Test edge cases (empty input, boundary values)
- [ ] Test error conditions
- [ ] Use descriptive test names

### 2. Property-Based Tests
- [ ] Identify universal properties that should hold
- [ ] Create strategies for generating valid inputs
- [ ] Reference requirements in doc comments
- [ ] Configure at least 100 test cases

### 3. BDD Scenarios (for CLI features)
- [ ] Write scenarios from the user's perspective
- [ ] Cover success and failure cases
- [ ] Document expected exit codes
- [ ] Use Background for common setup

### 4. Fuzz Targets (for parsing/input handling)
- [ ] Create structure-aware fuzz inputs with `Arbitrary`
- [ ] Ensure the target never panics
- [ ] Maintain domain invariants in input conversion

### 5. Verify with Mutation Testing
- [ ] Run mutation testing on changed code
- [ ] Address any surviving mutants
- [ ] Aim for the target kill rate for the crate

## CI Integration

Tests are automatically run in CI:

- **Every PR**: Unit tests, property tests, BDD tests, short fuzz session (60s/target)
- **Weekly**: Full mutation testing run
- **Coverage**: 80% line coverage minimum enforced

See `.github/workflows/ci.yml` for the complete CI configuration.
