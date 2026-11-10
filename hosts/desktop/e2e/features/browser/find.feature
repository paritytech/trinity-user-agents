@browser @allure.label.parentSuite:browser @allure.label.suite:Find @allure.label.feature:Find
Feature: Find in page

  Find-in-page runs the native Chromium search over the on-screen product
  webview's guest content. The link-tests product embeds a deterministic
  fixture paragraph: the token "alfafind" appears exactly 3 times and
  "betafind" exactly once, so match counts are stable.

  Background:
    Given the app is launched
    And the user skips onboarding
    And the link-tests product is open in a tab

  @allure.id:14764
  Scenario: TC-4.6.1 Open the find bar on a product
    When the user opens find in page
    Then the find bar is visible
    And the find input is focused

  @allure.id:14765
  Scenario: TC-4.6.2 Search matches, navigate next/previous, and counter
    When the user opens find in page
    And the user searches for "alfafind"
    Then the find counter shows "1/3"
    When the user steps to the next find match
    Then the find counter shows "2/3"
    When the user steps to the previous find match
    Then the find counter shows "1/3"

  @allure.id:14766
  Scenario: TC-4.6.3 No-results state in the find bar
    When the user opens find in page
    And the user searches for "zzzznope"
    Then the find counter shows "0/0"
    And the find navigation buttons are disabled

  @allure.id:14767
  Scenario: TC-4.6.4 Close the find bar
    When the user opens find in page
    And the user searches for "alfafind"
    Then the find counter shows "1/3"
    When the user closes the find bar
    Then the find bar is not visible

  @allure.id:14768
  Scenario: TC-4.6.5 Find shortcut is inert off product routes
    When the user navigates to the dashboard
    And the user opens find in page
    Then the find bar is not visible
