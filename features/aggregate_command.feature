Feature: aggregate command
  As a developer
  I want to aggregate multiple run receipts into a single weighted receipt
  So that I can see combined results from a fleet of runners

  Background:
    Given a temporary directory for test artifacts

  Scenario: Aggregate two run receipts
    Given a run receipt exists at "run1.json" with wall_ms median 100
    And a run receipt exists at "run2.json" with wall_ms median 110
    When I run "perfgate aggregate run1.json run2.json --out aggregated.json"
    Then the exit code should be 0
    And the file "aggregated.json" should exist
    And the file "aggregated.json" should contain valid JSON
    And the aggregate receipt should have 2 inputs
    And the aggregate receipt benchmark should be "test-bench"
