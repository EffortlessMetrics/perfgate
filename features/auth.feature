Feature: perfgate authentication
  As a developer
  I want to use different authentication methods
  So that I can securely interact with the perfgate server

  Scenario: API key format validation
    Then API key "pg_live_abcdefghijklmnopqrstuvwxyz123456" should be valid
    And API key "pg_test_abcdefghijklmnopqrstuvwxyz123456" should be valid
    And API key "invalid_key" should be invalid
    And API key "pg_live_short" should be invalid

  Scenario: Role and scope mapping
    Given a role "viewer"
    Then it should have scope "read"
    And it should not have scope "write"

    Given a role "contributor"
    Then it should have scope "read"
    And it should have scope "write"
    And it should not have scope "promote"

    Given a role "promoter"
    Then it should have scope "promote"
    And it should not have scope "delete"

    Given a role "admin"
    Then it should have scope "delete"
    And it should have scope "admin"

  Scenario: Generating API keys
    When I generate a live API key
    Then it should start with "pg_live_"
    And it should be at least 40 characters long

    When I generate a test API key
    Then it should start with "pg_test_"
    And it should be at least 40 characters long
