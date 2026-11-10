@chat @allure.label.parentSuite:chat @allure.label.suite:Contact_Search @allure.label.feature:Contact_Search
Feature: Contact search

  Single-client contact search behaviour (no peer needed): empty results for a
  non-matching query, and clearing the query.

  Background:
    Given the user is authenticated

  @allure.id:14814
  Scenario: TC-7.1.1 Open contact search from fullscreen chat
    When the user opens the chat as a tab
    And the user opens the contact search
    Then the contact search is open

  @allure.id:14816
  Scenario: TC-7.1.3 Search with no matches shows an empty result set
    When the user opens the chat as a tab
    And the user opens the contact search
    And the user searches contacts for "zzznomatchuser999000"
    Then the contact search shows no results

  @allure.id:14818
  Scenario: TC-7.1.5 Clear the search query
    When the user opens the chat as a tab
    And the user opens the contact search
    And the user searches contacts for "somequery"
    And the user clears the contact search
    Then the contact search query is empty
