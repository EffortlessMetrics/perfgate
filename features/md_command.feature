# BDD feature file for perfgate md command
# Validates: Requirements 2.3

Feature: Markdown Command
  As a CI pipeline
  I want to render comparison results as Markdown
  So that I can post formatted tables in PR comments

  Background:
    Given a temporary directory for test artifacts

  # Markdown output to stdout scenarios
  Scenario: Markdown output to stdout with pass verdict
    Given a compare receipt with pass verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "test-bench"

  Scenario: Markdown output to stdout with warn verdict
    Given a compare receipt with warn verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "test-bench"

  Scenario: Markdown output to stdout with fail verdict
    Given a compare receipt with fail verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "test-bench"

  # Markdown output to file scenarios
  Scenario: Markdown output to file with pass verdict
    Given a compare receipt with pass verdict
    When I run perfgate md with output file
    Then the exit code should be 0
    And the output file should exist
    And the markdown file should contain "test-bench"

  Scenario: Markdown output to file with warn verdict
    Given a compare receipt with warn verdict
    When I run perfgate md with output file
    Then the exit code should be 0
    And the output file should exist
    And the markdown file should contain "test-bench"

  Scenario: Markdown output to file with fail verdict
    Given a compare receipt with fail verdict
    When I run perfgate md with output file
    Then the exit code should be 0
    And the output file should exist
    And the markdown file should contain "test-bench"

  # Verdict emoji rendering scenarios (✅, ⚠️, ❌)
  Scenario: Pass verdict renders with checkmark emoji
    Given a compare receipt with pass verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "✅"

  Scenario: Warn verdict renders with warning emoji
    Given a compare receipt with warn verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "⚠️"

  Scenario: Fail verdict renders with cross emoji
    Given a compare receipt with fail verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "❌"

  # Table structure with all columns scenarios
  Scenario: Markdown table contains metric column
    Given a compare receipt with pass verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "metric"

  Scenario: Markdown table contains baseline column
    Given a compare receipt with pass verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "baseline (median)"

  Scenario: Markdown table contains current column
    Given a compare receipt with pass verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "current (median)"

  Scenario: Markdown table contains delta column
    Given a compare receipt with pass verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "delta"

  Scenario: Markdown table contains wall_ms metric row
    Given a compare receipt with pass verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "wall_ms"

  # Verdict reasons in markdown
  Scenario: Fail verdict includes reasons in markdown
    Given a compare receipt with fail verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "threshold"

  Scenario: Warn verdict includes reasons in markdown
    Given a compare receipt with warn verdict
    When I run perfgate md
    Then the exit code should be 0
    And the markdown should contain "warn"

  # File output preserves all content
  Scenario: File output contains verdict emoji
    Given a compare receipt with pass verdict
    When I run perfgate md with output file
    Then the exit code should be 0
    And the markdown file should contain "✅"

  Scenario: File output contains table structure
    Given a compare receipt with fail verdict
    When I run perfgate md with output file
    Then the exit code should be 0
    And the markdown file should contain "metric"
    And the markdown file should contain "baseline (median)"
    And the markdown file should contain "current (median)"
