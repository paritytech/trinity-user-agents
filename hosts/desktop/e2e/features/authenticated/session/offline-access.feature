@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Offline_Access @allure.label.feature:Offline_Access
Feature: Offline access

  @allure.id:14931
  Scenario: TC-13.1.1 User pins a product for offline use
    Given the user is authenticated
    And the user is on the dashboard
    And the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user selects "Enable offline access"
    And the user confirms the offline access dialog
    Then the pin indicator appears next to the product address

  @allure.id:14932
  Scenario: TC-13.1.2 Cancel the Enable offline dialog
    Given the user is authenticated
    And the user is on the dashboard
    And the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user selects "Enable offline access"
    And the user cancels the offline access dialog
    Then the offline access dialog is dismissed
    And the pin indicator does not appear next to the product address

  @allure.id:14933
  Scenario: TC-13.2.1 Remove offline access from a pinned product
    Given the user is authenticated
    And the user is on the dashboard
    And the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user selects "Enable offline access"
    And the user confirms the offline access dialog
    Then the pin indicator appears next to the product address
    When the user opens the product actions menu
    And the user selects "Remove offline use"
    And the user confirms the remove offline access dialog
    Then the pin indicator disappears from the product address

  @allure.id:14937
  Scenario: TC-13.5.1 Forgetting a pinned product clears its offline cache and local storage
    Given the user is authenticated
    And the user is on the dashboard
    And the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user selects "Enable offline access"
    And the user confirms the offline access dialog
    Then the pin indicator appears next to the product address
    When the user opens the product actions menu
    And the user opens product settings from the actions menu
    And the user clicks the "Forget App" product setting
    And the user confirms forgetting the product
    Then the "host-playground" product is removed from the apps settings list
