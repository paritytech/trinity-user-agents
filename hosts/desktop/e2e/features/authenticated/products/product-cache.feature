@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Clear_Cache_Forget @allure.label.feature:Clear_Cache_Forget
Feature: Clear cache and forget app

  The per-product settings page (/settings/privacy/apps/<id>) exposes Clear Cache
  and Forget App, each behind a confirmation dialog.

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14783
  Scenario: TC-5.3.1 Clear product cache from settings
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user opens product settings from the actions menu
    And the user clicks the "Clear Cache" product setting
    And the user confirms clearing the cache
    Then the product cache is cleared

  # Overlaps TC-13.5.1 (a different TestOps case, id 14784): forgetting an app
  # purges its persisted row and local storage, so it disappears from the apps
  # settings list. Unlike TC-13.5.1, no offline pin is established first.
  @allure.id:14784
  Scenario: TC-5.3.2 Forget an app removes it and its local storage
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user opens product settings from the actions menu
    And the user clicks the "Forget App" product setting
    And the user confirms forgetting the product
    Then the "host-playground" product is removed from the apps settings list
