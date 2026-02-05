# BDD feature file for perfgate report command
# Validates: Report generation from compare receipts

Feature: Report Command
  As a CI pipeline
  I want to generate cockpit-compatible reports from compare receipts
  So that I can integrate with dashboards and track performance trends

  Background:
    Given a temporary directory for test artifacts

  # Pass verdict scenarios
  Scenario: Report from pass verdict compare receipt
    Given a compare receipt with pass verdict
    When I run perfgate report
    Then the exit code should be 0
    And the report should have schema perfgate.report.v1
    And the report verdict should be pass
    And the report should have no findings
    And the report summary pass count should be 1
    And the report summary warn count should be 0
    And the report summary fail count should be 0

  # Warn verdict scenarios
  Scenario: Report from warn verdict compare receipt
    Given a compare receipt with warn verdict
    When I run perfgate report
    Then the exit code should be 0
    And the report should have schema perfgate.report.v1
    And the report verdict should be warn
    And the report should have findings with code metric_warn
    And the report summary warn count should be 1

  # Fail verdict scenarios
  Scenario: Report from fail verdict compare receipt
    Given a compare receipt with fail verdict
    When I run perfgate report
    Then the exit code should be 0
    And the report should have schema perfgate.report.v1
    And the report verdict should be fail
    And the report should have findings with code metric_fail
    And the report summary fail count should be 1

  # Determinism scenarios
  Scenario: Report output is deterministic
    Given a compare receipt with fail verdict
    When I run perfgate report twice
    Then both reports should be identical

  # Markdown output scenarios
  Scenario: Report with markdown output
    Given a compare receipt with pass verdict
    When I run perfgate report with markdown output
    Then the exit code should be 0
    And the report file should exist
    And the markdown file should exist
    And the markdown file should contain "perfgate"

  # Pretty print scenarios
  Scenario: Report with pretty print
    Given a compare receipt with pass verdict
    When I run perfgate report with pretty flag
    Then the exit code should be 0
    And the report file should contain indented JSON
