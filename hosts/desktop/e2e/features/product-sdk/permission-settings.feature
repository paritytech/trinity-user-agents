@product-sdk @allure.label.parentSuite:authenticated @allure.label.suite:Permissions @allure.label.feature:Permission_Settings
Feature: Permission settings entity pages

  The Settings → Privacy area exposes two permission entity views: per-app
  (/settings/privacy/apps/<id>) listing an app's requested permissions and per-
  permission (/settings/privacy/permissions/<id>) listing the apps that use a
  permission. Both are populated only by apps that have requested a permission,
  so each scenario first drives host-playground to grant a device permission
  (the auto-approver answers the dialog — no @manual-permissions here), then
  asserts on the settings pages.

  @allure.id:14800
  Scenario: TC-6.3.1 View an app's requested permissions
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    And the user runs "Device Permission: Notifications"
    And the result contains "Notifications"
    And the user is on the dashboard
    When the user opens settings
    And the user opens the settings page "Apps"
    And the user opens the "host-playground" app from the apps settings list
    Then the requested permissions list shows "Notifications"

  @allure.id:14801
  Scenario: TC-6.3.2 Change an app's permission status via the per-app entity page
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    And the user runs "Device Permission: Notifications"
    And the result contains "Notifications"
    And the user is on the dashboard
    When the user opens settings
    And the user opens the settings page "Apps"
    And the user opens the "host-playground" app from the apps settings list
    And the user opens the "Notifications" permission for the app
    And the user sets the permission status to "Denied"
    Then the permission status is "Denied"

  @allure.id:14802
  Scenario: TC-6.3.3 Reset an app permission to default
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    And the user runs "Device Permission: Notifications"
    And the result contains "Notifications"
    And the user is on the dashboard
    When the user opens settings
    And the user opens the settings page "Apps"
    And the user opens the "host-playground" app from the apps settings list
    And the user opens the "Notifications" permission for the app
    And the user sets the permission status to "Denied"
    And the user resets the permission to default
    Then the permission status is "Ask (Default)"

  @allure.id:14805
  Scenario: TC-6.4.1 View permissions list grouped by category
    Given the user is authenticated
    And the user is on the dashboard
    When the user opens settings
    And the user opens the settings page "Permissions"
    Then the permissions list shows the "Device & System Access", "On-Chain Actions" and "Access to Services" categories

  @allure.id:14806
  Scenario: TC-6.4.2 Permission detail lists apps using it
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    And the user runs "Device Permission: Notifications"
    And the result contains "Notifications"
    And the user is on the dashboard
    When the user opens settings
    And the user opens the settings page "Permissions"
    And the user opens the "Notifications" permission
    Then the "host-playground" app is listed for the permission

  # @skip: needs a granted alias context, and the alias request cannot reach its
  # dialog on this deployment — see the note in `alias-permissions.feature`.
  @allure.id:14811 @skip
  Scenario: TC-6.5.3 Manage alias contexts from app settings
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Accounts" tab
    And the user runs "Get Product Account Alias"
    And the user is on the dashboard
    When the user opens settings
    And the user opens the settings page "Apps"
    And the user opens the "host-playground" app from the apps settings list
    And the user opens the "Alias" permission for the app
    And the user opens the alias contexts dialog
    Then the alias contexts dialog lists a granted context

  @allure.id:14807
  Scenario: TC-6.4.3 Change a permission's status per app from the permission detail page
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    And the user runs "Device Permission: Notifications"
    And the result contains "Notifications"
    And the user is on the dashboard
    When the user opens settings
    And the user opens the settings page "Permissions"
    And the user opens the "Notifications" permission
    And the user opens the "host-playground" app from the permission detail
    And the user sets the permission status to "Denied"
    Then the permission status is "Denied"
