# Implementation Plan: Comprehensive Test Coverage

## Overview

This implementation plan establishes comprehensive test coverage for the perfgate project through BDD tests, expanded property-based tests, mutation testing improvements, fuzzing expansion, and CI integration. Tasks are organized to build incrementally, with testing infrastructure established first, followed by test implementation per crate.

## Tasks

- [x] 1. Set up BDD testing framework
  - [x] 1.1 Add cucumber dependency and configure test runner
    - Add `cucumber = "0.21"` to workspace dev-dependencies in Cargo.toml
    - Create `tests/cucumber.rs` with World struct and main function
    - Configure feature file discovery in `features/` directory
    - _Requirements: 1.1, 1.2, 1.5_

  - [x] 1.2 Create BDD step definitions for CLI commands
    - Implement Given steps for fixture creation (baseline/current receipts)
    - Implement When steps for CLI command execution
    - Implement Then steps for exit code and output assertions
    - Create helper functions for temp directory management
    - _Requirements: 1.2, 1.3_

  - [x] 1.3 Write BDD feature file for run command
    - Create `features/run_command.feature` with scenarios for:
      - Basic command execution with name and command
      - Repeat and warmup options
      - Timeout handling
      - Work units and throughput
      - Output file generation
    - _Requirements: 2.1_

  - [x] 1.4 Write BDD feature file for compare command
    - Create `features/compare_command.feature` with scenarios for:
      - Pass verdict (performance improved)
      - Warn verdict (near threshold)
      - Fail verdict (regression exceeds threshold)
      - Exit codes (0, 2, 3)
      - Custom threshold configuration
      - --fail-on-warn flag behavior
    - _Requirements: 2.2_

  - [x] 1.5 Write BDD feature file for md command
    - Create `features/md_command.feature` with scenarios for:
      - Markdown output to stdout
      - Markdown output to file
      - Verdict emoji rendering (✅, ⚠️, ❌)
      - Table structure with all columns
    - _Requirements: 2.3_

  - [x] 1.6 Write BDD feature file for github-annotations command
    - Create `features/annotations_command.feature` with scenarios for:
      - Error annotations for fail status
      - Warning annotations for warn status
      - No annotations for pass status
      - Annotation format validation
    - _Requirements: 2.4_

- [x] 2. Checkpoint - Verify BDD tests pass
  - Ensure all BDD tests pass with `cargo test --test cucumber`
  - Ask the user if questions arise

- [x] 3. Expand property-based tests in perfgate-types
  - [x] 3.1 Add property test for ConfigFile round-trip serialization
    - Create strategy for generating valid ConfigFile instances
    - Test JSON serialization round-trip
    - Test TOML serialization round-trip
    - **Property 1: JSON Serialization Round-Trip**
    - **Validates: Requirements 4.2, 4.5**
    - _Requirements: 4.2, 4.5_

  - [x] 3.2 Add property test for BenchConfigFile serialization
    - Create strategy for BenchConfigFile with all optional fields
    - Verify round-trip preserves all fields
    - **Property 1: JSON Serialization Round-Trip**
    - **Validates: Requirements 4.2, 4.5**
    - _Requirements: 4.2, 4.5_

  - [x] 3.3 Add property test for Budget and BudgetOverride types
    - Create strategies for Budget and BudgetOverride
    - Verify serialization preserves threshold relationships
    - **Property 1: JSON Serialization Round-Trip**
    - **Validates: Requirements 4.2, 4.5**
    - _Requirements: 4.2, 4.5_

- [x] 4. Expand property-based tests in perfgate-domain
  - [x] 4.1 Add property test for summarize_f64 ordering invariant
    - Verify min <= median <= max for all generated f64 lists
    - Handle NaN and infinity edge cases
    - **Property 2: Statistics Ordering Invariant**
    - **Validates: Requirements 4.6**
    - _Requirements: 4.6, 8.2_

  - [x] 4.2 Add property test for median algorithm edge cases
    - Test overflow handling for large u64 values
    - Verify even/odd length behavior matches specification
    - **Property 3: Median Algorithm Correctness**
    - **Validates: Requirements 8.5**
    - _Requirements: 8.5_

  - [x] 4.3 Add unit tests for domain error conditions
    - Test DomainError::NoSamples with empty input
    - Test DomainError::NoSamples with all-warmup samples
    - Test DomainError::InvalidBaseline with zero/negative baseline
    - _Requirements: 11.1, 11.2_

- [x] 5. Expand property-based tests in perfgate-adapters
  - [x] 5.1 Add property test for output truncation
    - Generate random byte sequences and cap values
    - Verify truncated length equals min(original, cap)
    - **Property 8: Output Truncation Invariant**
    - **Validates: Requirements 9.3**
    - _Requirements: 9.3_

  - [x] 5.2 Add unit tests for adapter error conditions
    - Test AdapterError::EmptyArgv with empty command
    - Test timeout behavior on Unix (if applicable)
    - _Requirements: 11.3_

- [x] 6. Checkpoint - Verify property tests pass
  - Run `cargo test --all` and verify all property tests pass
  - Ask the user if questions arise

- [x] 7. Expand fuzzing targets
  - [x] 7.1 Add fuzz target for config file parsing
    - Create `fuzz/fuzz_targets/parse_config.rs`
    - Fuzz TOML parsing of ConfigFile
    - Ensure no panics on malformed input
    - _Requirements: 5.1, 5.2_

  - [x] 7.2 Add fuzz target for duration string parsing
    - Create `fuzz/fuzz_targets/parse_duration.rs`
    - Fuzz humantime duration parsing
    - Verify graceful handling of invalid strings
    - _Requirements: 5.3_

  - [x] 7.3 Add fuzz target for compare_stats function
    - Create `fuzz/fuzz_targets/compare_stats.rs`
    - Use Arbitrary trait for structured input generation
    - Verify no panics with arbitrary Stats and Budget inputs
    - _Requirements: 5.4, 5.6_

  - [x] 7.4 Add Arbitrary derive to types for structure-aware fuzzing
    - Add `arbitrary` feature flag to perfgate-types
    - Derive Arbitrary for Stats, Budget, Delta types
    - Update fuzz/Cargo.toml dependencies
    - _Requirements: 5.6_

- [x] 8. Configure mutation testing
  - [x] 8.1 Create mutants.toml configuration file
    - Configure include/exclude globs for each crate
    - Set appropriate timeout values
    - Document expected kill rates per crate
    - _Requirements: 3.1, 3.2_

  - [x] 8.2 Add mutation testing xtask command improvements
    - Add per-crate mutation testing option
    - Add summary report generation
    - _Requirements: 3.2, 3.6_

  - [x] 8.3 Run initial mutation testing baseline
    - Execute mutation testing on perfgate-domain
    - Document surviving mutants and required tests
    - _Requirements: 3.3_

- [x] 9. Checkpoint - Verify fuzzing and mutation setup
  - Verify fuzz targets compile with `cargo +nightly fuzz build`
  - Run short fuzz session to verify no immediate crashes
  - Ask the user if questions arise

- [x] 10. Update CI workflows
  - [x] 10.1 Add coverage reporting to CI
    - Install cargo-llvm-cov in CI workflow
    - Generate lcov coverage report
    - Upload to coverage service (Codecov)
    - _Requirements: 6.4_

  - [x] 10.2 Add coverage threshold enforcement
    - Configure coverage minimum (80% line coverage)
    - Fail build if coverage drops below threshold
    - _Requirements: 6.5_

  - [x] 10.3 Add BDD tests to CI workflow
    - Add step to run cucumber tests
    - Ensure BDD tests run on every PR
    - _Requirements: 6.1_

  - [x] 10.4 Create scheduled mutation testing workflow
    - Create `.github/workflows/mutation.yml`
    - Schedule weekly mutation testing runs
    - Upload mutation report as artifact
    - _Requirements: 6.2_

  - [x] 10.5 Add fuzzing to CI workflow
    - Add short fuzzing session (60s per target) on PRs
    - Cache fuzzing corpus between runs
    - _Requirements: 6.6, 6.7_

- [x] 11. Create testing documentation
  - [x] 11.1 Create TESTING.md documentation file
    - Document testing strategy overview
    - Explain how to run each test type
    - Provide examples of well-written tests
    - Document property-based testing patterns
    - Document mutation testing coverage targets
    - _Requirements: 7.1, 7.2, 7.3, 7.4, 7.5_

- [x] 12. Final checkpoint - Comprehensive test verification
  - Run full test suite: `cargo test --all`
  - Run BDD tests: `cargo test --test cucumber`
  - Verify CI workflow passes
  - Ensure all tests pass, ask the user if questions arise

## Notes

- Each task references specific requirements for traceability
- Checkpoints ensure incremental validation
- Property tests validate universal correctness properties
- BDD tests provide living documentation of CLI behavior
- Fuzzing requires nightly Rust toolchain
- Mutation testing may take significant time (run scheduled, not on every PR)
