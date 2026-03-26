Feature: bisect command
  As a developer
  I want to automatically find the commit that introduced a performance regression
  So that I can quickly identify the root cause

  Background:
    Given a temporary directory for test artifacts

  Scenario: Bisect command help
    When I run "perfgate bisect --help"
    Then the exit code should be 0
    And the stdout should contain "good"
    And the stdout should contain "bad"
    And the stdout should contain "executable"
