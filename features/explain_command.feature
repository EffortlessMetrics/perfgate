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

  Scenario: Explain a binary size regression with blame
    Given a baseline Cargo.lock with:
      """
      [[package]]
      name = "serde"
      version = "1.0.0"
      """
    And a current Cargo.lock with:
      """
      [[package]]
      name = "serde"
      version = "1.0.1"
      """
    And a compare receipt exists at "compare-binary.json" with:
      | metric       | status | current | pct  |
      | binary_bytes | fail   | 1000000 | 0.20 |
    When I run "perfgate explain --compare compare-binary.json --baseline-lock baseline.lock --current-lock current.lock"
    Then the exit code should be 0
    And the stdout should contain "Binary Size Playbook"
    And the stdout should contain "Binary Blame Analysis"
    And the stdout should contain "Detected 1 dependency changes"
    And the stdout should contain "serde (Updated)"
    And the stdout should contain "LLM Prompt"
    And the stdout should contain "Detected Dependency Changes (Binary Blame)"
