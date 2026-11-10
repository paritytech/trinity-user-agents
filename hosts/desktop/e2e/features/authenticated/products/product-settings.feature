@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Product_SDK @allure.label.feature:Clear_Cache_Forget
Feature: Product settings — clear cache & forget

  Background:
    Given the user is authenticated
    And the user is on the dashboard
    And the user opens "host-playground" in a new tab

  @allure.id:14785
  Scenario: TC-5.3.3 Cancel Forget app dialog leaves the product intact
    When the user opens the product actions menu
    And the user opens product settings from the actions menu
    And the user clicks the "Forget App" product setting
    And the user cancels the product settings dialog
    Then the product settings dialog is dismissed
    And the "Forget App" product setting is still available
