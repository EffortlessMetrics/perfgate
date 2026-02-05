# BDD feature file for perfgate export command
# Validates: Export functionality for trend analysis and time-series ingestion

Feature: Export Command
  As a CI pipeline
  I want to export run and compare receipts to CSV or JSONL
  So that I can ingest performance data into trend analysis systems

  Background:
    Given a temporary directory for test artifacts

  # Run receipt export to CSV
  Scenario: Export run receipt to CSV
    Given a baseline receipt with wall_ms median of 1000
    When I run perfgate export run to csv
    Then the exit code should be 0
    And the export file should exist
    And the export file should contain "bench_name"
    And the export file should contain "wall_ms_median"
    And the export file should contain "test-bench"

  # Run receipt export to JSONL
  Scenario: Export run receipt to JSONL
    Given a baseline receipt with wall_ms median of 1000
    When I run perfgate export run to jsonl
    Then the exit code should be 0
    And the export file should exist
    And the export file should contain "bench_name"
    And the export file should be valid JSONL

  # Compare receipt export to CSV
  Scenario: Export compare receipt to CSV
    Given a compare receipt with pass verdict
    When I run perfgate export compare to csv
    Then the exit code should be 0
    And the export file should exist
    And the export file should contain "metric"
    And the export file should contain "baseline_value"
    And the export file should contain "regression_pct"

  # Compare receipt export to JSONL
  Scenario: Export compare receipt to JSONL
    Given a compare receipt with fail verdict
    When I run perfgate export compare to jsonl
    Then the exit code should be 0
    And the export file should exist
    And the export file should be valid JSONL

  # Stable ordering verification
  Scenario: Export produces stable ordering across runs
    Given a compare receipt with pass verdict
    When I run perfgate export compare to csv twice
    Then the exit code should be 0
    And the two export files should be identical

  # Default format is CSV
  Scenario: Export uses CSV as default format
    Given a baseline receipt with wall_ms median of 1000
    When I run perfgate export run with default format
    Then the exit code should be 0
    And the export file should contain "bench_name,wall_ms_median"

  # Alphabetical metric ordering in compare export
  Scenario: Compare export orders metrics alphabetically
    Given a compare receipt with pass verdict
    When I run perfgate export compare to csv
    Then the exit code should be 0
    And the metrics should be sorted alphabetically
