@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Appearance @allure.label.feature:Theme
Feature: Appearance settings

  # The appearance redesign (PR #744) replaced the System/Light/Dark cards with a
  # "Color mode" segmented control labeled Device/Day/Night (plus a separate
  # theme-name picker). The TC titles keep their TestOps ids; assertions target
  # the new color-mode labels.
  @allure.id:14900
  Scenario: TC-10.2.1 Theme settings expose Device, Day and Night color modes
    Given the user is authenticated
    And the user is on the dashboard
    When the user opens the appearance settings
    Then the color mode options Device, Day and Night are available

  @allure.id:14902
  Scenario: TC-10.2.3 Switching color mode to Device applies and is selected
    Given the user is authenticated
    And the user is on the dashboard
    When the user opens the appearance settings
    And the user selects the "Day" color mode
    Then the "Day" color mode is selected
    When the user selects the "Device" color mode
    Then the "Device" color mode is selected

  @allure.id:14901
  Scenario: TC-10.2.2 Switch to dark theme via user popover
    Given the user is authenticated
    And the user is on the dashboard
    When the user toggles the theme to dark
    Then the authenticated dashboard screenshot is taken as "dark-theme"
