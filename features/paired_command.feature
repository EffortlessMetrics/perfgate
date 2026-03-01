# BDD feature file for perfgate paired command
# Validates: Paired interleaved benchmark execution

Feature: Paired Command
  As a CI pipeline
  I want to run paired baseline/current benchmarks
  So that I can reduce environmental noise in regressions

  Background:
    Given a temporary directory for test artifacts

  Scenario: Paired run with shell commands produces receipt
    When I run perfgate paired with shell commands
    Then the exit code should be 0
    And the output file should exist
    And the paired receipt should have schema perfgate.paired.v1
    And the paired receipt should have bench name "paired-bench"

  Scenario: Paired run includes warmup pairs in samples
    When I run perfgate paired with repeat 2 and warmup 1
    Then the exit code should be 0
    And the paired receipt should have 3 samples
    And the paired receipt should have 1 warmup samples

  Scenario: Paired run fails on nonzero command without allow-nonzero
    When I run perfgate paired with a failing baseline command
    Then the exit code should be 1
    And the stderr should contain "paired benchmark failed"

  Scenario: Paired run allows nonzero with allow-nonzero
    When I run perfgate paired with allow-nonzero and a failing baseline command
    Then the exit code should be 0
    And the output file should exist

  Scenario: Paired run with higher warmup excludes warmup from stats
    When I run perfgate paired with repeat 2 and warmup 3
    Then the exit code should be 0
    And the paired receipt should have 5 samples
    And the paired receipt should have 3 warmup samples
    And the paired receipt stats diff count should be 2

  Scenario: Paired run with custom repeat count
    When I run perfgate paired with repeat 4 and warmup 0
    Then the exit code should be 0
    And the paired receipt should have 4 samples
    And the paired receipt should have 0 warmup samples

  Scenario: Paired run with pretty-printed JSON
    When I run perfgate paired with pretty output
    Then the exit code should be 0
    And the output file should exist
    And the output file should contain valid JSON
    And the output file should be pretty-printed

  Scenario: Paired run output has all required receipt fields
    When I run perfgate paired with shell commands
    Then the exit code should be 0
    And the output file should contain valid JSON
    And the paired receipt should have schema perfgate.paired.v1
    And the paired receipt should contain stats with baseline and current wall_ms
    And the paired receipt should contain run metadata

  Scenario: Paired run fails on failing current command
    When I run perfgate paired with a failing current command
    Then the exit code should be 1
    And the stderr should contain "paired benchmark failed"

  Scenario: Paired run with custom bench name
    When I run perfgate paired with bench name "my-custom-bench-123"
    Then the exit code should be 0
    And the output file should exist
    And the paired receipt should have bench name "my-custom-bench-123"

  Scenario: Paired run with work units enables throughput stats
    When I run perfgate paired with work units 1000
    Then the exit code should be 0
    And the output file should exist
    And the paired receipt should have throughput stats
