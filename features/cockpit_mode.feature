# BDD feature file for perfgate check --mode cockpit
# Validates: Cockpit mode produces sensor.report.v1 envelope and extras/ artifacts

Feature: Cockpit Mode
  As a CI pipeline integrating with Cockpit
  I want to run checks in cockpit mode
  So that I get sensor.report.v1 envelopes with native artifacts in extras/

  Background:
    Given a temporary directory for test artifacts

  # Basic cockpit mode with baseline
  Scenario: Cockpit mode produces sensor.report.v1 and extras with baseline
    Given a config file with bench "cockpit-basic"
    And a baseline receipt for bench "cockpit-basic" with wall_ms median of 1000
    When I run perfgate check for bench "cockpit-basic" in cockpit mode
    Then the exit code should be 0
    And the report.json artifact should have schema sensor.report.v1
    And the artifact file "extras/perfgate.run.v1.json" should exist
    And the artifact file "extras/perfgate.compare.v1.json" should exist
    And the artifact file "extras/perfgate.report.v1.json" should exist
    And the comment.md artifact should exist

  # Cockpit mode without baseline (no compare artifact)
  Scenario: Cockpit mode without baseline skips compare extras
    Given a config file with bench "cockpit-no-baseline"
    When I run perfgate check for bench "cockpit-no-baseline" in cockpit mode
    Then the exit code should be 0
    And the report.json artifact should have schema sensor.report.v1
    And the artifact file "extras/perfgate.run.v1.json" should exist
    And the artifact file "extras/perfgate.compare.v1.json" should not exist
    And the artifact file "extras/perfgate.report.v1.json" should exist

  # Cockpit mode always exits 0, even when standard mode would exit 2
  Scenario: Cockpit mode exits 0 even with budget violation
    Given a config file with bench "cockpit-fail" with threshold 1.01
    And a baseline receipt for bench "cockpit-fail" with wall_ms median of 1
    When I run perfgate check for bench "cockpit-fail" in cockpit mode
    Then the exit code should be 0
    And the report.json artifact should have schema sensor.report.v1

  # Multi-bench cockpit mode via --all
  Scenario: Cockpit mode with --all writes per-bench extras
    Given a config file with benches "cockpit-a,cockpit-b"
    When I run perfgate check for all benches in cockpit mode
    Then the exit code should be 0
    And the report.json artifact should have schema sensor.report.v1
    And the artifact file "extras/cockpit-a/perfgate.run.v1.json" should exist
    And the artifact file "extras/cockpit-a/perfgate.report.v1.json" should exist
    And the artifact file "extras/cockpit-b/perfgate.run.v1.json" should exist
    And the artifact file "extras/cockpit-b/perfgate.report.v1.json" should exist
    And the comment.md artifact should exist

  # Multi-bench cockpit mode with baselines
  Scenario: Cockpit mode --all with baselines writes compare extras
    Given a config file with benches "cockpit-c,cockpit-d"
    And a baseline receipt for bench "cockpit-c" with wall_ms median of 1000
    And a baseline receipt for bench "cockpit-d" with wall_ms median of 1000
    When I run perfgate check for all benches in cockpit mode
    Then the exit code should be 0
    And the report.json artifact should have schema sensor.report.v1
    And the artifact file "extras/cockpit-c/perfgate.run.v1.json" should exist
    And the artifact file "extras/cockpit-c/perfgate.compare.v1.json" should exist
    And the artifact file "extras/cockpit-c/perfgate.report.v1.json" should exist
    And the artifact file "extras/cockpit-d/perfgate.run.v1.json" should exist
    And the artifact file "extras/cockpit-d/perfgate.compare.v1.json" should exist
    And the artifact file "extras/cockpit-d/perfgate.report.v1.json" should exist

  # Cockpit mode with --require-baseline fails with error report
  Scenario: Cockpit mode with missing baseline writes error in report
    Given a config file with bench "cockpit-missing-bl"
    When I run perfgate check for bench "cockpit-missing-bl" in cockpit mode with --require-baseline
    Then the exit code should be 0
    And the report.json artifact should have schema sensor.report.v1
