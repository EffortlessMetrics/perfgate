Feature: summary command
  As a developer
  I want to summarize multiple compare receipts
  So that I can quickly see an overview of performance across many benchmarks

  Background:
    Given a temporary directory for test artifacts

  Scenario: Summary command prints a table of results
    Given a compare receipt exists at "run1.json" with:
      | metric  | status | current | pct  |
      | wall_ms | pass   | 100.0   | -0.1 |
    And a compare receipt exists at "run2.json" with:
      | metric  | status | current | pct  |
      | wall_ms | fail   | 150.0   | 0.5  |
    When I run "perfgate summary run*.json"
    Then the command should succeed
    And the stdout should contain "run1"
    And the stdout should contain "run2"
    And the stdout should contain "pass"
    And the stdout should contain "fail"
    And the stdout should contain "-10.0%"
    And the stdout should contain "50.0%"
