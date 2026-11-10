@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Profile @allure.label.feature:Profile
Feature: Profile / user popover

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14905
  Scenario: TC-10.3.2 Full username is shown in the profile popup
    When the user opens the user popover
    Then the profile popover shows a full username

  @isolated @allure.id:14907
  Scenario: TC-10.3.4 Log out from the user popover ends the session
    When the user opens the user popover
    And the user logs out from the user popover
    Then the user is returned to the onboarding screen

  @isolated @allure.id:14699
  Scenario: TC-2.3.2 Logout with a product open returns to onboarding
    Given the user opens "coinflipgame03" in a new tab
    When the user opens the user popover
    And the user logs out from the user popover
    Then the user is returned to the onboarding screen
