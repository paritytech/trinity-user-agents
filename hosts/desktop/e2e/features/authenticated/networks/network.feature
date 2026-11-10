@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Networks @allure.label.feature:Environment_Switching
Feature: Environment switching

  The testnet endpoint settings let the user pick the environment. Switching to
  a different one while signed in prompts a logout confirmation (the switch logs
  out and reloads); cancelling keeps the current environment, and re-selecting
  the current one is a no-op.

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14887
  Scenario: TC-9.1.1 Environment selector lists the available environments
    When the user opens the testnet endpoint settings
    And the user opens the environment selector
    Then the environment selector lists at least 2 environments

  @allure.id:14888
  Scenario: TC-9.1.2 Switching environment while signed in prompts logout confirmation
    When the user opens the testnet endpoint settings
    And the user selects a different environment
    Then the network change logout dialog is shown

  @allure.id:14889
  Scenario: TC-9.1.3 Cancelling the environment-change dialog keeps the current environment
    When the user opens the testnet endpoint settings
    And the user selects a different environment
    Then the network change logout dialog is shown
    When the user cancels the network change dialog
    Then the network change logout dialog is not shown
    And the testnet endpoint settings are still shown

  @allure.id:14890
  Scenario: TC-9.1.4 Selecting the same environment is a no-op
    When the user opens the testnet endpoint settings
    And the user selects the current environment
    Then the network change logout dialog is not shown
