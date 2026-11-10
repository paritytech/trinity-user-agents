@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Settings @allure.label.feature:Settings_Navigation
Feature: Settings navigation

  The settings sidebar groups the sub-pages (Preferences, Privacy & Security,
  Development) and keeps its own back/forward history so navigating between
  sub-pages can be undone/redone.

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14897
  Scenario: TC-10.1.1 Settings sidebar shows Preferences, Privacy and Development groups
    When the user opens settings
    Then the settings sidebar shows the "Preferences", "Privacy & Security" and "Development" groups

  # Navigate two distinct, non-default sub-pages so the history index advances
  # twice (the tracker collapses a navigation back to an existing entry, so
  # re-visiting the default Appearance page would move the index back instead).
  @allure.id:14899
  Scenario: TC-10.1.3 Settings back/forward navigation preserves the selected sub-page
    When the user opens settings
    And the user opens the settings page "Apps"
    Then the active settings page is "/settings/privacy/apps"
    When the user opens the settings page "Permissions"
    Then the active settings page is "/settings/privacy/permissions"
    When the user navigates settings back
    Then the active settings page is "/settings/privacy/apps"
    When the user navigates settings forward
    Then the active settings page is "/settings/privacy/permissions"
