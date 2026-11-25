@chat @allure.label.parentSuite:chat @allure.label.suite:Chat_List @allure.label.feature:Chat_List
Feature: Chat list

  Background:
    Given the user is authenticated

  @allure.id:14854
  Scenario: TC-7.6.1 Empty chat list state
    When the user opens the chat as a tab
    Then the chat room list is empty
