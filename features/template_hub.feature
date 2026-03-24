Feature: Template Hub
  As a developer
  I want to use custom Handlebars templates for performance reports
  So that I can customize the output for different CI systems and stakeholders

  Background:
    Given a temporary directory for test artifacts

  Scenario: Render markdown using a custom template
    Given a compare receipt exists at "compare.json" with:
      | metric  | status | current | pct |
      | wall_ms | fail   | 150.0   | 0.5 |
    And a template file "custom.hbs" with:
      """
      Custom Report for {{bench.name}}
      Status: {{verdict.status}}
      """
    When I run "perfgate md --compare compare.json --template custom.hbs --out report.md"
    Then the exit code should be 0
    And the file "report.md" should exist
    And the file "report.md" should contain "Custom Report for compare"
    And the file "report.md" should contain "Status: fail"
