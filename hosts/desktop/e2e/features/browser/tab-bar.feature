@browser @allure.label.parentSuite:browser @allure.label.suite:Tab_Bar @allure.label.feature:Tab_Bar
Feature: Tab bar

  Background:
    Given the app is launched
    And the user skips onboarding
    And the link-tests product is open in a tab

  @allure.id:14742
  Scenario: TC-4.2.1 Tab bar hidden in single-active-tab (one-tab) mode
    Then the tab bar is hidden

  @allure.id:14743
  Scenario: TC-4.2.2 Open multiple tabs and switch between them
    When the user opens an additional new tab
    Then the tab bar shows 2 tabs
    And the active tab is a new tab
    When the user switches to the link-tests product tab
    Then the active tab is the link-tests product
    When the user switches to the new tab
    Then the active tab is a new tab

  @allure.id:14747
  Scenario: TC-4.2.6 Reorder tabs by drag
    When the user opens an additional new tab
    Then the tab bar shows 2 tabs
    When the user drags the first tab past the second tab
    Then the link-tests product tab moves to the second position

  # Pin glyph is not asserted: the no-auth browser project pins no product,
  # so only the RAM-usage row of the hover card is covered here.
  @allure.id:14748
  Scenario: TC-4.2.7 Tab hover card shows RAM usage
    When the user opens an additional new tab
    Then the tab bar shows 2 tabs
    When the user hovers over the link-tests product tab
    Then the tab hover card shows the RAM usage

  @allure.id:14749
  Scenario: TC-4.2.8 Closing the last tab returns to the dashboard
    When the user closes the current tab with the keyboard
    Then the host is on the dashboard
