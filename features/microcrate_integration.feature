# BDD feature file for microcrate integration tests
# Tests the individual microcrates work correctly in isolation and integration

Feature: Microcrate Integration
  As a perfgate developer
  I want each microcrate to work correctly in isolation and integration
  So that the codebase is modular and maintainable

  Background:
    Given a working perfgate installation

  Scenario: SHA-256 microcrate produces correct hashes
    When I compute SHA-256 of "hello world"
    Then the hash should be "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"

  Scenario: Stats microcrate computes median correctly
    Given a list of values "10, 20, 30, 40, 50"
    When I compute the median
    Then the median should be 30

  Scenario: Stats microcrate handles even-length lists
    Given a list of values "10, 20, 30, 40"
    When I compute the median
    Then the median should be 25

  Scenario: Validation microcrate accepts valid bench names
    When I validate bench name "my-benchmark"
    Then the validation should pass

  Scenario: Validation microcrate rejects invalid bench names
    When I validate bench name "My-Benchmark"
    Then the validation should fail with "invalid characters"

  Scenario: Validation microcrate rejects path traversal
    When I validate bench name "../escape"
    Then the validation should fail with "path traversal"

  Scenario: Host detect microcrate detects OS mismatch
    Given baseline host with os "linux"
    And current host with os "windows"
    When I detect host mismatch
    Then a mismatch should be detected
    And the reason should contain "OS mismatch"

  Scenario: Host detect microcrate ignores minor CPU differences
    Given baseline host with cpu_count 8
    And current host with cpu_count 12
    When I detect host mismatch
    Then no mismatch should be detected

  Scenario: Export microcrate produces valid CSV
    Given a run receipt for bench "test-bench"
    When I export to CSV format
    Then the output should be valid CSV
    And the header should contain "bench_name"

  Scenario: Render microcrate produces markdown table
    Given a compare receipt with status "fail"
    When I render markdown
    Then the output should contain "perfgate: fail"
    And the output should contain a markdown table

  Scenario: Sensor microcrate builds sensor report
    Given a perfgate report with status "pass"
    When I build a sensor report
    Then the sensor report should have schema "sensor.report.v1"
    And the verdict status should be "pass"

  # ============================================================================
  # ERROR MICROCRATE SCENARIOS
  # ============================================================================

  Scenario: Error microcrate converts validation error to perfgate error
    When I convert ValidationError::Empty to PerfgateError
    Then the error category should be "validation"
    And the error should not be recoverable

  Scenario: Error microcrate converts stats error to perfgate error
    When I convert StatsError::NoSamples to PerfgateError
    Then the error category should be "stats"
    And the error should not be recoverable

  Scenario: Error microcrate converts adapter error to perfgate error
    When I convert AdapterError::Timeout to PerfgateError
    Then the error category should be "adapter"
    And the error should be recoverable

  Scenario: Error microcrate converts config error to perfgate error
    When I convert ConfigValidationError::BenchName to PerfgateError
    Then the error category should be "config"
    And the error should not be recoverable

  Scenario: Error microcrate converts io error to perfgate error
    When I convert IoError::BaselineResolve to PerfgateError
    Then the error category should be "io"
    And the error should be recoverable

  Scenario: Error microcrate converts paired error to perfgate error
    When I convert PairedError::NoSamples to PerfgateError
    Then the error category should be "paired"
    And the error should not be recoverable

  Scenario: Error microcrate provides correct exit codes
    When I convert ValidationError::Empty to PerfgateError
    Then the error exit code should be positive

  Scenario: Error microcrate propagates std io error
    When I convert std::io::Error to PerfgateError
    Then the error category should be "io"

  # ============================================================================
  # BUDGET MICROCRATE SCENARIOS
  # ============================================================================

  Scenario: Budget passes when regression is below threshold
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Lower
    When I evaluate budget with baseline 100.0 and current 105.0
    Then the budget status should be "pass"
    And the regression should be 0.05

  Scenario: Budget warns when regression approaches threshold
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Lower
    When I evaluate budget with baseline 100.0 and current 115.0
    Then the budget status should be "warn"
    And the regression should be 0.15

  Scenario: Budget fails when regression exceeds threshold
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Lower
    When I evaluate budget with baseline 100.0 and current 130.0
    Then the budget status should be "fail"
    And the regression should be 0.30

  Scenario: Budget handles improvement with Direction::Lower
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Lower
    When I evaluate budget with baseline 100.0 and current 90.0
    Then the budget status should be "pass"
    And the regression should be 0.0

  Scenario: Budget handles regression with Direction::Higher
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Higher
    When I evaluate budget with baseline 100.0 and current 70.0
    Then the budget status should be "fail"
    And the regression should be 0.30

  Scenario: Budget handles improvement with Direction::Higher
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Higher
    When I evaluate budget with baseline 100.0 and current 120.0
    Then the budget status should be "pass"
    And the regression should be 0.0

  Scenario: Budget rejects zero baseline
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Lower
    When I evaluate budget with baseline 0.0 and current 100.0
    Then the budget evaluation should fail with "baseline value must be > 0"

  Scenario: Budget rejects negative baseline
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Lower
    When I evaluate budget with baseline -10.0 and current 100.0
    Then the budget evaluation should fail with "baseline value must be > 0"

  Scenario: Budget aggregates verdict correctly with fail
    Given budget statuses "pass, warn, fail"
    When I aggregate the verdict
    Then the aggregated verdict should be "fail"
    And the verdict counts should be pass 1, warn 1, fail 1

  Scenario: Budget aggregates verdict correctly with warn only
    Given budget statuses "pass, warn, pass"
    When I aggregate the verdict
    Then the aggregated verdict should be "warn"
    And the verdict counts should be pass 2, warn 1, fail 0

  Scenario: Budget aggregates verdict correctly with all pass
    Given budget statuses "pass, pass, pass"
    When I aggregate the verdict
    Then the aggregated verdict should be "pass"
    And the verdict counts should be pass 3, warn 0, fail 0

  Scenario: Budget generates correct reason token
    When I generate a reason token for Metric::WallMs with status "warn"
    Then the reason token should be "wall_ms_warn"

  Scenario: Budget at exact threshold boundary
    Given a budget with threshold 0.20 and warn_threshold 0.10 for Direction::Lower
    When I evaluate budget with baseline 100.0 and current 120.0
    Then the budget status should be "warn"

  # ============================================================================
  # SIGNIFICANCE MICROCRATE SCENARIOS
  # ============================================================================

  Scenario: Significance detected with large sample difference
    Given baseline samples "100, 102, 98, 101, 99, 100, 101, 99"
    And current samples "110, 112, 108, 111, 109, 110, 111, 109"
    When I compute significance with alpha 0.05 and min_samples 8
    Then the result should be significant
    And the p-value should be less than 0.05

  Scenario: No significance with small sample difference
    Given baseline samples "100, 102, 98, 101, 99, 100, 101, 99"
    And current samples "100, 102, 98, 101, 99, 100, 101, 99"
    When I compute significance with alpha 0.05 and min_samples 8
    Then the result should not be significant
    And the p-value should be approximately 1.0

  Scenario: Significance returns none for insufficient samples
    Given baseline samples "100, 101, 102"
    And current samples "100, 101, 102, 103, 104, 105, 106, 107"
    When I compute significance with alpha 0.05 and min_samples 8
    Then the result should be none

  Scenario: Significance returns none for single sample
    Given baseline samples "100"
    And current samples "100"
    When I compute significance with alpha 0.05 and min_samples 1
    Then the result should be none

  Scenario: Significance handles zero variance equal means
    Given baseline samples "100, 100, 100, 100, 100, 100, 100, 100"
    And current samples "100, 100, 100, 100, 100, 100, 100, 100"
    When I compute significance with alpha 0.05 and min_samples 8
    Then the result should not be significant
    And the p-value should be 1.0

  Scenario: Significance handles zero variance different means
    Given baseline samples "100, 100, 100, 100, 100, 100, 100, 100"
    And current samples "110, 110, 110, 110, 110, 110, 110, 110"
    When I compute significance with alpha 0.05 and min_samples 8
    Then the result should be significant
    And the p-value should be 0.0

  Scenario: Significance records sample counts correctly
    Given baseline samples "100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100, 100"
    And current samples "110, 110, 110, 110, 110, 110, 110, 110, 110, 110, 110, 110"
    When I compute significance with alpha 0.05 and min_samples 8
    Then the baseline sample count should be 15
    And the current sample count should be 12

  Scenario: Significance respects alpha threshold
    Given baseline samples "100, 101, 99, 100, 101, 99, 100, 101"
    And current samples "102, 103, 101, 102, 103, 101, 102, 103"
    When I compute significance with alpha 0.10 and min_samples 8
    Then the alpha value should be 0.10

  Scenario: Significance with noisy data not significant
    Given baseline samples "97.5, 100.0, 102.5, 97.5, 100.0, 102.5, 97.5, 100.0, 102.5, 97.5, 100.0, 102.5, 97.5, 100.0, 102.5, 97.5, 100.0, 102.5, 97.5, 100.0"
    And current samples "98.0, 100.5, 103.0, 98.0, 100.5, 103.0, 98.0, 100.5, 103.0, 98.0, 100.5, 103.0, 98.0, 100.5, 103.0, 98.0, 100.5, 103.0, 98.0, 100.5"
    When I compute significance with alpha 0.05 and min_samples 8
    Then the result should not be significant

  # ============================================================================
  # ERROR PROPAGATION ACROSS CRATES
  # ============================================================================

  Scenario: Error propagates correctly from validation to budget
    When I validate bench name "My-Invalid-Bench"
    Then the validation should fail with "invalid characters"

  Scenario: Error propagates correctly across crate boundaries
    When I convert ValidationError::Empty to PerfgateError
    Then the error exit code should be 1
