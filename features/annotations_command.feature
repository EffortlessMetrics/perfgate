# BDD feature file for perfgate github-annotations command
# Validates: Requirements 2.4
# Property 7: GitHub Annotation Generation

Feature: GitHub Annotations Command
  As a CI pipeline
  I want to generate GitHub Actions workflow annotations
  So that performance issues are highlighted directly in the PR

  Background:
    Given a temporary directory for test artifacts

  # Error annotations for fail status
  # Property 7: Metrics with Fail status SHALL produce exactly one ::error:: annotation
  Scenario: Error annotation generated for fail verdict
    Given a compare receipt with fail verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the output should contain an error annotation

  Scenario: Error annotation contains bench name for fail verdict
    Given a compare receipt with fail verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the output should contain an error annotation
    And the annotation should contain bench name "test-bench"

  Scenario: Error annotation contains metric name for fail verdict
    Given a compare receipt with fail verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should contain "wall_ms"

  Scenario: Error annotation contains delta percentage for fail verdict
    Given a compare receipt with fail verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should contain "%"

  # Warning annotations for warn status
  # Property 7: Metrics with Warn status SHALL produce exactly one ::warning:: annotation
  Scenario: Warning annotation generated for warn verdict
    Given a compare receipt with warn verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the output should contain a warning annotation

  Scenario: Warning annotation contains bench name for warn verdict
    Given a compare receipt with warn verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the output should contain a warning annotation
    And the annotation should contain bench name "test-bench"

  Scenario: Warning annotation contains metric name for warn verdict
    Given a compare receipt with warn verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should contain "wall_ms"

  Scenario: Warning annotation contains delta percentage for warn verdict
    Given a compare receipt with warn verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should contain "%"

  # No annotations for pass status
  # Property 7: Metrics with Pass status SHALL produce no annotations
  Scenario: No annotations generated for pass verdict
    Given a compare receipt with pass verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the output should contain no annotations

  Scenario: Pass verdict produces empty or minimal output
    Given a compare receipt with pass verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should be empty

  # Annotation format validation
  # Property 7: Each annotation SHALL contain the bench name, metric name, and delta percentage
  Scenario: Error annotation follows GitHub Actions format
    Given a compare receipt with fail verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should contain "::error::"

  Scenario: Warning annotation follows GitHub Actions format
    Given a compare receipt with warn verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should contain "::warning::"

  Scenario: Annotation format includes all required information
    Given a compare receipt with fail verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
    And the stdout should contain "::error::"
    And the stdout should contain "test-bench"
    And the stdout should contain "wall_ms"
    And the stdout should contain "%"

  # Exit code validation
  Scenario: Exit code is 0 regardless of verdict status
    Given a compare receipt with fail verdict
    When I run perfgate github-annotations
    Then the exit code should be 0

  Scenario: Exit code is 0 for warn verdict
    Given a compare receipt with warn verdict
    When I run perfgate github-annotations
    Then the exit code should be 0

  Scenario: Exit code is 0 for pass verdict
    Given a compare receipt with pass verdict
    When I run perfgate github-annotations
    Then the exit code should be 0
