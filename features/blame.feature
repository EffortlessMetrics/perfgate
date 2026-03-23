Feature: Binary Delta Blame
  As a developer
  I want to understand why my binary size changed
  So that I can identify which dependency updates caused regressions

  Scenario: Identify dependency updates in paired mode
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
    When I run the binary blame analysis
    Then the blame report should show "serde" was updated from "1.0.0" to "1.0.1"

  Scenario: Identify added and removed dependencies
    Given a baseline Cargo.lock with:
      """
      [[package]]
      name = "old-dep"
      version = "1.0.0"
      """
    And a current Cargo.lock with:
      """
      [[package]]
      name = "new-dep"
      version = "2.0.0"
      """
    When I run the binary blame analysis
    Then the blame report should show "old-dep" was removed
    And the blame report should show "new-dep" was added
