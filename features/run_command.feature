# BDD feature file for perfgate run command
# Validates: Requirements 2.1

Feature: Run Command
  As a CI pipeline
  I want to run benchmarks and capture performance metrics
  So that I can track performance over time

  Background:
    Given a temporary directory for test artifacts

  # Basic command execution scenarios
  Scenario: Basic command execution with name
    When I run perfgate run with name "my-benchmark"
    Then the exit code should be 0
    And the output file should exist
    And the output file should contain valid JSON
    And the run receipt should have bench name "my-benchmark"
    And the run receipt should have schema perfgate.run.v1

  Scenario: Run command with different benchmark name
    When I run perfgate run with name "performance-test"
    Then the exit code should be 0
    And the output file should exist
    And the run receipt should have bench name "performance-test"

  # Repeat and warmup options scenarios
  # Note: Total samples = repeat + warmup
  Scenario: Run command with repeat option only
    When I run perfgate run with repeat 3 and warmup 0
    Then the exit code should be 0
    And the output file should exist
    And the output file should contain valid JSON
    And the run receipt should have 3 samples
    And the run receipt should have 0 warmup samples

  Scenario: Run command with warmup option
    # repeat=5 + warmup=2 = 7 total samples
    When I run perfgate run with repeat 5 and warmup 2
    Then the exit code should be 0
    And the output file should exist
    And the run receipt should have 7 samples
    And the run receipt should have 2 warmup samples

  Scenario: Run command with warmup samples excluded from stats
    # repeat=4 + warmup=1 = 5 total samples
    When I run perfgate run with repeat 4 and warmup 1
    Then the exit code should be 0
    And the output file should contain valid JSON
    And the run receipt should have 5 samples
    And the run receipt should have 1 warmup samples

  # Timeout handling scenarios
  # Note: Timeout is only supported on Unix platforms.
  # On Windows, these scenarios will fail with exit code 1 (tool error).
  @unix
  Scenario: Run command with timeout option succeeds on Unix
    When I run perfgate run with timeout "10s"
    Then the exit code should be 0
    And the output file should exist
    And the output file should contain valid JSON

  @unix
  Scenario: Run command with short timeout format on Unix
    When I run perfgate run with timeout "5s"
    Then the exit code should be 0
    And the output file should exist

  @unix
  Scenario: Run command with millisecond timeout on Unix
    When I run perfgate run with timeout "500ms"
    Then the exit code should be 0
    And the output file should exist

  # Work units and throughput scenarios
  Scenario: Run command with work units calculates throughput
    When I run perfgate run with work units 1000
    Then the exit code should be 0
    And the output file should exist
    And the output file should contain valid JSON
    And the run receipt should have throughput_per_s stats

  Scenario: Run command with different work units value
    When I run perfgate run with work units 500
    Then the exit code should be 0
    And the run receipt should have throughput_per_s stats

  # Output file generation scenarios
  Scenario: Output file contains valid run receipt schema
    When I run perfgate run with name "schema-test"
    Then the exit code should be 0
    And the output file should exist
    And the output file should contain valid JSON
    And the run receipt should have schema perfgate.run.v1

  Scenario: Output file is generated with correct structure
    When I run perfgate run with repeat 2 and warmup 0
    Then the exit code should be 0
    And the output file should exist
    And the output file should contain valid JSON
    And the run receipt should have 2 samples
