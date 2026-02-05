# BDD feature file for perfgate promote command
# Validates: Promote use case requirements

Feature: Promote Command
  As a CI pipeline on a trusted branch
  I want to promote run receipts to become baselines
  So that subsequent comparisons use updated baselines

  Background:
    Given a temporary directory for test artifacts

  # Basic promote scenarios
  Scenario: Promote creates valid baseline file
    Given a run receipt with wall_ms median of 1000
    When I run perfgate promote
    Then the exit code should be 0
    And the baseline file should exist
    And the baseline file should be valid JSON
    And the baseline should have the same wall_ms median of 1000

  Scenario: Promote preserves all receipt data without normalize
    Given a run receipt with wall_ms median of 500
    And the run receipt has run_id "unique-test-id-123"
    When I run perfgate promote without normalize
    Then the exit code should be 0
    And the baseline should have run_id "unique-test-id-123"

  # Normalize flag scenarios
  Scenario: Promote with normalize strips run_id and timestamps
    Given a run receipt with wall_ms median of 1000
    And the run receipt has run_id "unique-test-id-456"
    And the run receipt has started_at "2024-06-15T10:30:00Z"
    When I run perfgate promote with normalize
    Then the exit code should be 0
    And the baseline should have run_id "baseline"
    And the baseline should have started_at "1970-01-01T00:00:00Z"

  Scenario: Normalize preserves benchmark metadata
    Given a run receipt with wall_ms median of 750
    And the run receipt has bench name "my-benchmark"
    When I run perfgate promote with normalize
    Then the exit code should be 0
    And the baseline should have bench name "my-benchmark"
    And the baseline should have the same wall_ms median of 750

  Scenario: Normalize preserves host info
    Given a run receipt with wall_ms median of 1000
    When I run perfgate promote with normalize
    Then the exit code should be 0
    And the baseline should preserve host os and arch

  # Error scenarios
  Scenario: Promote fails gracefully if source missing
    Given a nonexistent source file
    When I run perfgate promote with missing source
    Then the exit code should be 1
    And the stderr should contain "read"

  Scenario: Promote fails gracefully if source is invalid JSON
    Given an invalid JSON source file
    When I run perfgate promote with invalid source
    Then the exit code should be 1
    And the stderr should contain "parse"

  # Atomic write behavior
  Scenario: Promote uses atomic write
    Given a run receipt with wall_ms median of 1000
    When I run perfgate promote
    Then the exit code should be 0
    And the baseline file should exist
    And no temporary files should remain

  # Pretty print option
  Scenario: Promote with pretty flag formats JSON
    Given a run receipt with wall_ms median of 1000
    When I run perfgate promote with pretty
    Then the exit code should be 0
    And the baseline file should be pretty-printed JSON
