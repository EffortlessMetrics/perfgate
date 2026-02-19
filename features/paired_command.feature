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
