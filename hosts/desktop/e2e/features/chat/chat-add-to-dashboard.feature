@chat @allure.label.parentSuite:chat @allure.label.suite:Chat_Add_To_Dashboard @allure.label.feature:Chat_Add_To_Dashboard
Feature: Chat — Add to Dashboard

  From the fullscreen chat (SPA) the address bar shows a single "Add to Dashboard"
  affordance (not the product ••• menu). It opens a modal where chat can be added
  as an S/M/L widget and as a favourites icon — independently — and both land on
  the dashboard.

  Background:
    Given the user is authenticated

  Scenario: Add chat to the dashboard as a widget and a favourite from the chat address bar
    When the user opens the chat fullscreen view
    And the user opens the Add to Dashboard modal from the chat address bar
    Then the chat Add to Dashboard modal is shown without the product actions menu
    When the user adds chat as a "Large" widget
    And the user adds chat to favourites
    Then chat is shown as added to favourites
    When the user closes the Add to Dashboard modal
    And the user navigates to the dashboard
    Then the chat widget is shown on the dashboard
