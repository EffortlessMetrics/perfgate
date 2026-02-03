# BDD feature file for perfgate check command
# Validates: Config-driven one-command workflow

Feature: Check Command
  As a CI pipeline
  I want to run config-driven benchmark checks
  So that I can have a simple one-command workflow for performance testing

  Background:
    Given a temporary directory for test artifacts

  # Basic check workflow scenarios
  Scenario: Check runs bench and produces all artifacts
    Given a config file with bench "my-bench"
    And a baseline receipt for bench "my-bench" with wall_ms median of 1000
    When I run perfgate check for bench "my-bench"
    Then the exit code should be 0
    And the run.json artifact should exist
    And the compare.json artifact should exist
    And the report.json artifact should exist
    And the comment.md artifact should exist

  Scenario: Check handles missing baseline with warning
    Given a config file with bench "new-bench"
    When I run perfgate check for bench "new-bench"
    Then the exit code should be 0
    And the run.json artifact should exist
    And the compare.json artifact should not exist
    And the comment.md artifact should exist
    And the comment.md should contain "no baseline"

  Scenario: Check with --require-baseline fails if baseline missing
    Given a config file with bench "no-baseline-bench"
    When I run perfgate check for bench "no-baseline-bench" with --require-baseline
    Then the exit code should be 1
    And the stderr should contain "baseline required"

  # Note: Scenarios that test specific exit codes for budget violations require
  # controlling runtime behavior, which is tested via unit and integration tests.
  # BDD tests focus on verifiable structural outcomes.

  # Config resolution scenarios
  Scenario: Check uses defaults from config when bench does not specify
    Given a config file with defaults repeat 3 and warmup 1
    And a bench "default-bench" without explicit repeat or warmup
    When I run perfgate check for bench "default-bench"
    Then the exit code should be 0
    And the run.json should have 4 samples
    And the run.json should have 1 warmup samples

  Scenario: Check uses bench-specific settings over defaults
    Given a config file with defaults repeat 3
    And a bench "specific-bench" with repeat 5
    When I run perfgate check for bench "specific-bench"
    Then the exit code should be 0
    And the run.json should have 5 samples

  # Baseline path resolution scenarios
  Scenario: Check uses --baseline path when provided
    Given a config file with bench "explicit-baseline-bench"
    And a baseline receipt at "custom/path/baseline.json" with wall_ms median of 1000
    When I run perfgate check for bench "explicit-baseline-bench" with --baseline "custom/path/baseline.json"
    Then the exit code should be 0
    And the compare.json artifact should exist

  Scenario: Check falls back to baseline_dir from config
    Given a config file with bench "config-baseline-bench" and baseline_dir "my-baselines"
    And a baseline receipt at "my-baselines/config-baseline-bench.json" with wall_ms median of 1000
    When I run perfgate check for bench "config-baseline-bench"
    Then the exit code should be 0
    And the compare.json artifact should exist
