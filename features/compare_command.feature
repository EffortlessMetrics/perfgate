# BDD feature file for perfgate compare command
# Validates: Requirements 2.2

Feature: Compare Command
  As a CI pipeline
  I want to compare benchmark results against baselines
  So that I can detect performance regressions

  Background:
    Given a temporary directory for test artifacts

  # Pass verdict scenarios - performance improved or within threshold
  Scenario: Pass verdict when performance improves
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 900
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 0
    And the verdict should be pass
    And the compare receipt should contain wall_ms delta
    And the compare receipt should have schema perfgate.compare.v1

  Scenario: Pass verdict when performance is unchanged
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1000
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 0
    And the verdict should be pass

  Scenario: Pass verdict when regression is within threshold
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1100
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 0
    And the verdict should be pass

  # Fail verdict scenarios - regression exceeds threshold
  Scenario: Fail verdict when regression exceeds threshold
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1500
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 2
    And the verdict should be fail
    And the reasons should mention regression percentage

  Scenario: Fail verdict at exact threshold boundary
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1210
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 2
    And the verdict should be fail

  Scenario: Fail verdict with large regression
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 2000
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 2
    And the verdict should be fail
    And the reasons should mention regression percentage

  # Warn verdict scenarios - near threshold
  # Note: warn_threshold = threshold * warn_factor
  # With threshold=0.20 and warn_factor=0.90, warn_threshold=0.18
  # Warn occurs when: warn_threshold <= regression <= threshold (i.e., 18% to 20%)
  Scenario: Warn verdict when regression is near threshold
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1190
    When I run perfgate compare with threshold 0.20 and warn-factor 0.90
    Then the exit code should be 0
    And the verdict should be warn

  Scenario: Warn verdict at warn threshold boundary
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1185
    When I run perfgate compare with threshold 0.20 and warn-factor 0.90
    Then the exit code should be 0
    And the verdict should be warn

  # Exit code scenarios
  Scenario: Exit code 0 for pass verdict
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 950
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 0

  Scenario: Exit code 2 for fail verdict
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1300
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 2

  # Exit code 3: warn with --fail-on-warn flag
  # With threshold=0.20 and warn_factor=0.90, warn_threshold=0.18
  # 1190/1000 = 1.19 = 19% regression, which is in warn range (18%-20%)
  Scenario: Exit code 3 for warn with fail-on-warn flag
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1190
    And the --fail-on-warn flag is set
    When I run perfgate compare with threshold 0.20 and warn-factor 0.90
    Then the exit code should be 3
    And the verdict should be warn

  # Custom threshold configuration scenarios
  Scenario: Custom threshold allows larger regression
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1400
    When I run perfgate compare with threshold 0.50
    Then the exit code should be 0
    And the verdict should be pass

  Scenario: Strict threshold catches small regression
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1060
    When I run perfgate compare with threshold 0.05
    Then the exit code should be 2
    And the verdict should be fail

  # Custom warn-factor: threshold=0.20, warn_factor=0.50 => warn_threshold=0.10
  # 1100/1000 = 10% regression, which is exactly at warn_threshold boundary
  Scenario: Custom warn-factor adjusts warn threshold
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1100
    When I run perfgate compare with threshold 0.20 and warn-factor 0.50
    Then the exit code should be 0
    And the verdict should be warn

  # --fail-on-warn flag behavior scenarios
  # With threshold=0.20 and warn_factor=0.90, warn_threshold=0.18
  # 1190/1000 = 19% regression, which is in warn range (18%-20%)
  Scenario: Warn without fail-on-warn returns exit code 0
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1190
    When I run perfgate compare with threshold 0.20 and warn-factor 0.90
    Then the exit code should be 0
    And the verdict should be warn

  Scenario: Warn with fail-on-warn returns exit code 3
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1190
    And the --fail-on-warn flag is set
    When I run perfgate compare with threshold 0.20 and warn-factor 0.90
    Then the exit code should be 3
    And the verdict should be warn

  Scenario: Pass verdict unaffected by fail-on-warn flag
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 900
    And the --fail-on-warn flag is set
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 0
    And the verdict should be pass

  Scenario: Fail verdict unaffected by fail-on-warn flag
    Given a baseline receipt with wall_ms median of 1000
    And a current receipt with wall_ms median of 1500
    And the --fail-on-warn flag is set
    When I run perfgate compare with threshold 0.20
    Then the exit code should be 2
    And the verdict should be fail
