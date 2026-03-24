Feature: explain command
  As a developer
  I want to understand performance regressions
  So that I can quickly diagnose and fix them

  Background:
    Given a temporary directory for test artifacts

  Scenario: Explain a regression with playbooks
    Given a compare receipt exists at "compare.json" with:
      | metric  | status | current | pct |
      | wall_ms | fail   | 150.0   | 0.5 |
    When I run "perfgate explain --compare compare.json"
    Then the exit code should be 0
    And the stdout should contain "# Performance Analysis"
    And the stdout should contain "Performance Regressions Detected"
    And the stdout should contain "Wall Time Playbook"
    And the stdout should contain "LLM Prompt"

  Scenario: Explain a pass result
    Given a compare receipt exists at "pass.json" with:
      | metric  | status | current | pct  |
      | wall_ms | pass   | 100.0   | 0.0  |
    When I run "perfgate explain --compare pass.json"
    Then the exit code should be 0
    And the stdout should contain "Great news!"
