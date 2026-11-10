@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Chat_List @allure.label.feature:QuickChat
Feature: QuickChat popover

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14859
  Scenario: TC-7.6.6 QuickChat "View more" opens the chat tab
    When the user opens the quick chat popover
    And the user expands quick chat to the chat tab
    Then the host is on the chat route
