@browser @allure.label.parentSuite:browser @allure.label.suite:New_Tab @allure.label.feature:New_Tab
Feature: New tab page

  Background:
    Given the app is launched
    And the user skips onboarding

  # The pinned grid is hardcoded to the labels host-playground / coinflipgame03 /
  # test-dapp-01, completed with the network TLD; seeding those three makes all
  # cards render. The recent grid + recents come from the seeded recents list
  # (which needs a reload to hydrate).
  @allure.id:14773
  Scenario: TC-4.8.1 New tab page shows wordmark, address bar, pinned and recent apps
    Given seeded pinned and recent products
    And the app reloads with onboarding skipped
    When the user opens a new tab page
    Then the new tab page shows the wordmark
    And the new tab page shows the address bar
    And the new tab page shows the pinned app grid
    And the new tab page shows the recent app grid

  @allure.id:14774
  Scenario: TC-4.8.2 Clear recents with undo on the new tab page
    Given seeded pinned and recent products
    And the app reloads with onboarding skipped
    When the user opens a new tab page
    Then the new tab page shows the recent app grid
    When the user clears recents on the new tab page
    Then the recents-cleared toast is visible
    And the new tab page shows no recent apps
    When the user undoes clearing recents
    Then the new tab page shows the recent app grid
