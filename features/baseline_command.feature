Feature: Baseline Command
  As a user
  I want to manage my baselines remotely
  So that I can use centralized baseline storage

  Background:
    Given a temporary directory for test artifacts

  Scenario: Baseline command fails without subcommand
    When I run perfgate baseline with no args
    Then the exit code should be 2
    And the stderr should contain "Usage:"

  Scenario: Baseline list fails without baseline server
    When I run perfgate baseline list without server
    Then the exit code should be 1
    And the stderr should contain "baseline server is not configured"

  Scenario: Baseline upload fails without baseline server
    When I run perfgate baseline upload without server
    Then the exit code should be 1
    And the stderr should contain "baseline server is not configured"

  Scenario: Baseline download fails without baseline server
    When I run perfgate baseline download without server
    Then the exit code should be 1
    And the stderr should contain "baseline server is not configured"

  Scenario: Baseline delete fails without baseline server
    When I run perfgate baseline delete without server
    Then the exit code should be 1
    And the stderr should contain "baseline server is not configured"
