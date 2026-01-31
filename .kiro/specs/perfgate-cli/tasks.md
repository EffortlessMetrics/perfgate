# Implementation Plan: perfgate CLI

## Overview

This plan completes the perfgate CLI implementation. The workspace scaffold exists with most core logic implemented. The remaining work focuses on:
1. Fixing dependency issues in Cargo.toml files
2. Adding missing dependency to CLI crate
3. Adding property-based tests for domain logic
4. Adding integration tests for CLI commands
5. Ensuring all tests pass

## Tasks

- [x] 1. Fix workspace dependency issues
  - [x] 1.1 Add perfgate-adapters dependency to perfgate-cli Cargo.toml
    - Add `perfgate-adapters = { path = "../perfgate-adapters" }` to dependencies
    - _Requirements: 1.1, 2.1_
  
  - [x] 1.2 Add serde dependency to xtask Cargo.toml
    - Add `serde.workspace = true` to xtask dependencies
    - _Requirements: 9.3_
  
  - [x] 1.3 Verify workspace builds without errors
    - Run `cargo build --all`
    - _Requirements: 1.1_

- [x] 2. Checkpoint - Verify build passes
  - Ensure `cargo build --all` succeeds, ask the user if questions arise.

- [ ] 3. Add property-based tests for domain logic
  - [x] 3.1 Add proptest dependency to perfgate-domain
    - Add `proptest = "1"` to dev-dependencies
    - _Requirements: 3.1_
  
  - [-] 3.2 Write property test for statistics computation
    - **Property 1: Statistics Computation Correctness**
    - Generate random u64 vectors, verify median/min/max are correct
    - **Validates: Requirements 3.1, 3.2, 3.3**
  
  - [~] 3.3 Write property test for warmup exclusion
    - **Property 2: Warmup Sample Exclusion**
    - Generate samples with warmup flags, verify warmup samples don't affect stats
    - **Validates: Requirements 3.4**
  
  - [~] 3.4 Write property test for metric status determination
    - **Property 4: Metric Status Determination**
    - Generate random baseline/current/threshold values, verify status logic
    - **Validates: Requirements 5.1, 5.2, 5.3**
  
  - [~] 3.5 Write property test for verdict aggregation
    - **Property 5: Verdict Aggregation**
    - Generate random metric status sets, verify verdict logic
    - **Validates: Requirements 5.4, 5.5, 5.6**

- [ ] 4. Add property-based tests for serialization
  - [~] 4.1 Add proptest and arbitrary dependencies to perfgate-types
    - Add `proptest = "1"` and `proptest-derive = "0.4"` to dev-dependencies
    - _Requirements: 10.1_
  
  - [~] 4.2 Write property test for RunReceipt round-trip
    - **Property 8: Serialization Round-Trip (RunReceipt)**
    - Generate arbitrary RunReceipt, serialize to JSON, deserialize, compare
    - **Validates: Requirements 10.1**
  
  - [~] 4.3 Write property test for CompareReceipt round-trip
    - **Property 8: Serialization Round-Trip (CompareReceipt)**
    - Generate arbitrary CompareReceipt, serialize to JSON, deserialize, compare
    - **Validates: Requirements 10.2**

- [ ] 5. Add property-based tests for rendering
  - [~] 5.1 Add proptest dependency to perfgate-app
    - Add `proptest = "1"` to dev-dependencies
    - _Requirements: 7.2_
  
  - [~] 5.2 Write property test for markdown rendering completeness
    - **Property 6: Markdown Rendering Completeness**
    - Generate arbitrary CompareReceipt, verify output contains required elements
    - **Validates: Requirements 7.2, 7.3, 7.4, 7.5**
  
  - [~] 5.3 Write property test for GitHub annotation generation
    - **Property 7: GitHub Annotation Generation**
    - Generate arbitrary CompareReceipt, verify annotation format and count
    - **Validates: Requirements 8.2, 8.3, 8.4, 8.5**

- [~] 6. Checkpoint - Verify property tests pass
  - Ensure `cargo test --all` succeeds, ask the user if questions arise.

- [ ] 7. Add CLI integration tests
  - [~] 7.1 Create test fixtures directory and baseline receipts
    - Create `crates/perfgate-cli/tests/fixtures/` directory
    - Add sample Run_Receipt JSON files for testing
    - _Requirements: 4.1_
  
  - [~] 7.2 Write integration test for `perfgate run` command
    - Test basic run with `--name test -- true`
    - Verify output file is valid JSON with correct schema
    - **Validates: Requirements 1.1, 1.2, 9.1**
  
  - [~] 7.3 Write integration test for `perfgate compare` command
    - Test compare with fixture baseline and current receipts
    - Verify exit codes for pass/warn/fail scenarios
    - **Validates: Requirements 4.1, 6.1, 6.2, 6.3**
  
  - [~] 7.4 Write integration test for `perfgate md` command
    - Test markdown generation from compare receipt
    - Verify output contains expected table structure
    - **Validates: Requirements 7.1, 7.6**
  
  - [~] 7.5 Write integration test for `perfgate github-annotations` command
    - Test annotation output format
    - **Validates: Requirements 8.1**

- [~] 8. Final checkpoint - Ensure all tests pass
  - Run `cargo test --all` and verify all tests pass
  - Run `cargo clippy --all-targets --all-features -- -D warnings`
  - Run `cargo fmt --all -- --check`
  - Ask the user if questions arise.

## Notes

- All tasks are required for comprehensive testing
- The existing codebase has most core logic implemented
- Property tests use proptest library with minimum 100 iterations
- Integration tests use assert_cmd and predicates crates
- Fuzz targets already exist in `fuzz/` directory for parser robustness
