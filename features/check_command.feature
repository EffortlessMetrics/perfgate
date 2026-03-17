# BDD feature file for perfgate check command
# Validates: Config-driven one-command workflow

Feature: Check Command
  As a CI pipeline
  I want to run config-driven benchmark checks
  So that I can have a simple one-command workflow for performance testing

  Background:
    Given a temporary directory for test artifacts

  # Basic check workflow scenarios
  Scenario: Check runs bench and produces all artifacts
    Given a config file with bench "my-bench"
    And a baseline receipt for bench "my-bench" with wall_ms median of 1000
    When I run perfgate check for bench "my-bench"
    Then the exit code should be 0
    And the run.json artifact should exist
    And the compare.json artifact should exist
    And the report.json artifact should exist
    And the comment.md artifact should exist

  Scenario: Check handles missing baseline with warning
    Given a config file with bench "new-bench"
    When I run perfgate check for bench "new-bench"
    Then the exit code should be 0
    And the run.json artifact should exist
    And the compare.json artifact should not exist
    And the report.json artifact should exist
    And the comment.md artifact should exist
    And the comment.md should contain "no baseline"

  Scenario: Check with --require-baseline fails if baseline missing
    Given a config file with bench "no-baseline-bench"
    When I run perfgate check for bench "no-baseline-bench" with --require-baseline
    Then the exit code should be 1
    And the stderr should contain "baseline not found"

  # Note: Scenarios that test specific exit codes for budget violations require
  # controlling runtime behavior, which is tested via unit and integration tests.
  # BDD tests focus on verifiable structural outcomes.

  # Config resolution scenarios
  Scenario: Check uses defaults from config when bench does not specify
    Given a config file with defaults repeat 3 and warmup 1
    And a bench "default-bench" without explicit repeat or warmup
    When I run perfgate check for bench "default-bench"
    Then the exit code should be 0
    And the run.json should have 4 samples
    And the run.json should have 1 warmup samples

  Scenario: Check uses bench-specific settings over defaults
    Given a config file with defaults repeat 3
    And a bench "specific-bench" with repeat 5
    When I run perfgate check for bench "specific-bench"
    Then the exit code should be 0
    And the run.json should have 5 samples

  # Baseline path resolution scenarios
  Scenario: Check uses --baseline path when provided
    Given a config file with bench "explicit-baseline-bench"
    And a baseline receipt at "custom/path/baseline.json" with wall_ms median of 1000
    When I run perfgate check for bench "explicit-baseline-bench" with --baseline "custom/path/baseline.json"
    Then the exit code should be 0
    And the compare.json artifact should exist

  Scenario: Check falls back to baseline_dir from config
    Given a config file with bench "config-baseline-bench" and baseline_dir "my-baselines"
    And a baseline receipt at "my-baselines/config-baseline-bench.json" with wall_ms median of 1000
    When I run perfgate check for bench "config-baseline-bench"
    Then the exit code should be 0
    And the compare.json artifact should exist

  Scenario: Check resolves baseline from baseline_pattern
    Given a config file with bench "pattern-bench" and baseline_pattern "custom-baselines/{bench}.json"
    And a baseline receipt at "custom-baselines/pattern-bench.json" with wall_ms median of 1000
    When I run perfgate check for bench "pattern-bench"
    Then the exit code should be 0
    And the compare.json artifact should exist

  Scenario: Check writes GitHub outputs when requested
    Given a config file with bench "github-output-bench"
    And a baseline receipt for bench "github-output-bench" with wall_ms median of 1000
    When I run perfgate check for bench "github-output-bench" with --output-github
    Then the exit code should be 0
    And the github output file should exist
    And the github output should contain "verdict="
    And the github output should contain "bench_count=1"

  Scenario: Check cockpit mode writes sensor envelope and versioned extras
    Given a config file with bench "cockpit-bench"
    And a baseline receipt for bench "cockpit-bench" with wall_ms median of 1000
    When I run perfgate check for bench "cockpit-bench" in cockpit mode
    Then the exit code should be 0
    And the report.json artifact should have schema sensor.report.v1
    And the artifact file "extras/perfgate.run.v1.json" should exist
    And the artifact file "extras/perfgate.compare.v1.json" should exist
    And the artifact file "extras/perfgate.report.v1.json" should exist

  # Bench regex filtering scenarios
  Scenario: Check with --bench-regex selects benchmarks by pattern
    Given a config file with benches "alpha-bench,beta-bench,gamma-bench"
    And a baseline receipt for bench "alpha-bench" with wall_ms median of 1000
    And a baseline receipt for bench "beta-bench" with wall_ms median of 1000
    When I run perfgate check for all benches with --bench-regex "alpha|beta"
    Then the exit code should be 0
    And the artifact file "alpha-bench/run.json" should exist
    And the artifact file "beta-bench/run.json" should exist
    And the artifact file "gamma-bench/run.json" should not exist

  # Config validation error scenarios
  Scenario: Check with malformed TOML config fails
    Given a malformed TOML config file
    When I run perfgate check for bench "any-bench"
    Then the exit code should be 1
    And the stderr should contain "TOML parse error"

  Scenario: Check with missing required command field fails
    Given a config file with bench "no-cmd" missing the command field
    When I run perfgate check for bench "no-cmd"
    Then the exit code should be 1
    And the stderr should contain "command"

  Scenario: Check with invalid threshold type fails
    Given a config file with an invalid threshold type
    When I run perfgate check for bench "bad-thresh"
    Then the exit code should be 1
    And the stderr should contain "TOML parse error"

  Scenario: Check with empty benchmarks list and --all fails
    Given a config file with no benchmarks defined
    When I run perfgate check for all benches
    Then the exit code should be 1
    And the stderr should contain "no benchmarks defined"

  Scenario: Check with non-existent bench name fails
    Given a config file with bench "existing-bench"
    When I run perfgate check for bench "nonexistent-bench"
    Then the exit code should be 1
    And the stderr should contain "not found in config"

  Scenario: Check with invalid metric name in budget fails
    Given a config file with bench "bad-metric" and invalid metric in budgets
    When I run perfgate check for bench "bad-metric"
    Then the exit code should be 1
    And the stderr should contain "TOML parse error"

  # Multi-bench --all exit code aggregation scenarios
  Scenario: Check --all with all benches passing exits 0
    Given a config file with benches "bench-a,bench-b,bench-c"
    When I run perfgate check for all benches
    Then the exit code should be 0
    And the artifact file "bench-a/run.json" should exist
    And the artifact file "bench-b/run.json" should exist
    And the artifact file "bench-c/run.json" should exist

  Scenario: Check --all with one failing bench exits 2
    Given a config file with benches "pass-bench,fail-bench" and tight threshold
    And a baseline receipt for bench "fail-bench" with wall_ms median of 1
    When I run perfgate check for all benches
    Then the exit code should be 2
    And the artifact file "pass-bench/run.json" should exist
    And the artifact file "fail-bench/run.json" should exist

  Scenario: Check --all with one warn bench and --fail-on-warn exits 3
    Given a config file with benches "pass-bench,warn-bench" and lenient threshold
    And a baseline receipt for bench "warn-bench" with wall_ms median of 1
    When I run perfgate check for all benches with --fail-on-warn
    Then the exit code should be 3

  Scenario: Check --all with fail and warn exits 2
    Given a config file with benches "fail-bench" and tight threshold and bench "warn-bench" with lenient threshold
    And a baseline receipt for bench "fail-bench" with wall_ms median of 1
    And a baseline receipt for bench "warn-bench" with wall_ms median of 1
    When I run perfgate check for all benches with --fail-on-warn
    Then the exit code should be 2
