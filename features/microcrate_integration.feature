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
