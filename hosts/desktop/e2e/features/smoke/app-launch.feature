@smoke @allure.label.parentSuite:smoke @allure.label.suite:App_Launch @allure.label.feature:App_Launch
Feature: App Launch

  @allure.id:14675
  Scenario: TC-1.1.1 App launches successfully
    Given the app is launched
    Then the app window is visible
