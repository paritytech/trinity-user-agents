@product-sdk @manual-permissions @allure.label.parentSuite:authenticated @allure.label.suite:Permissions @allure.label.feature:Dialog_Theming
Feature: Permission dialog theming

  Permission request dialogs must render legibly in dark theme. Each scenario
  switches the app to the dark theme, then drives host-playground to pop a
  device-permission dialog (manual-permissions keeps the auto-approver off), and
  captures a screenshot attachment for human pixel-legibility review in Allure
  while asserting the dialog actually rendered.

  @allure.id:14813
  Scenario: TC-6.7.1 Permission modal text is legible in dark theme
    Given the user is authenticated
    And the user switches to the dark theme
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Device Permission: Camera"
    Then the device permission dialog is shown
    And the permission dialog screenshot is taken as "permission-dialog-dark-6-7-1"

  @allure.id:14903
  Scenario: TC-10.2.4 Permission request modal text is readable in dark theme
    Given the user is authenticated
    And the user switches to the dark theme
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Device Permission: Microphone"
    Then the device permission dialog is shown
    And the permission dialog screenshot is taken as "permission-dialog-dark-10-2-4"
