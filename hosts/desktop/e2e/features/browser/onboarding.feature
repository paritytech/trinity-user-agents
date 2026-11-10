@browser @allure.label.parentSuite:browser @allure.label.suite:Skip_Onboarding @allure.label.feature:Onboarding
Feature: Skip onboarding

  Background:
    Given the app is launched
    And the user skips onboarding

  # The "log in" half is covered by reaching the login entry point (the
  # onboarding QR screen); actually pairing needs a signer (auth project).
  @allure.id:14687
  Scenario: TC-1.3.2 After skipping, the user can still open Settings and log in
    When the user opens settings from the user menu
    Then the host is on the settings page
    When the user opens the login flow from the user menu
    Then the QR code is visible on onboarding screen

  @allure.id:14688
  Scenario: TC-1.3.3 Dashboard button does not get stuck in an endless loading state
    When the user opens settings from the user menu
    Then the host is on the settings page
    When the user clicks the dashboard home button
    Then the dashboard is ready and not stuck loading
