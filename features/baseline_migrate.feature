Feature: baseline migrate command
  As a developer
  I want to migrate local baseline files to the server
  So that I can centralize my performance data

  Background:
    Given a temporary directory for test artifacts
    And a mock baseline server is running

  Scenario: Migrate multiple baselines to the server
    Given a baseline file exists at "baselines/bench1.json"
    And a baseline file exists at "baselines/bench2.json"
    When I run "perfgate baseline migrate --dir baselines --project my-project"
    Then the command should succeed
    And the stdout should contain "Migrating 2 baselines"
    And the stdout should contain "Migrated: bench1"
    And the stdout should contain "Migrated: bench2"
    And the stdout should contain "Migration complete: 2 succeeded, 0 failed"
