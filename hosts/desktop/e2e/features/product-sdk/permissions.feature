@product-sdk @manual-permissions @allure.label.parentSuite:authenticated @allure.label.suite:Permissions @allure.label.feature:Device_Permissions
Feature: Device permissions

  Device permission requests from a product pop a host permission dialog. The
  feature is tagged manual-permissions so the auto-approver is off and the test
  drives the dialog itself.

  The core takes a `PermissionDecision` and owns the stored decision, and the host
  renders the same `RemotePermissionRequestDialog` for this prompt as for its own
  remote-URL grant, headed by the requesting product. Deny, Allow Once and Allow
  Always are offered for every capability; Allow Once also opens the host's native
  device gate (camera, microphone, location) for that product until it is closed.

  TC-6.1.2 (14792) and TC-6.2.2 (14797) cover Allow Once and remain dropped; neither
  scenario has been rewritten for the button.

  @allure.id:14791
  Scenario: TC-6.1.1 Approving grants a device permission
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Device Permission: Camera"
    And the user approves the permission
    Then the result contains "Camera"

  @allure.id:14795
  Scenario: TC-6.1.5 A granted permission is not prompted again
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Device Permission: Notifications"
    And the user approves the permission
    Then the result contains "Notifications"
    When the user runs "Device Permission: Notifications"
    Then the result contains "Notifications"

  @allure.id:14793
  Scenario: TC-6.1.3 Deny blocks a device permission
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Device Permission: Location"
    And the user denies the permission
    Then the result contains "Location"

  @allure.id:14794
  Scenario: TC-6.1.4 Dismissing the permission dialog defaults to denied
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Device Permission: Bluetooth"
    And the user dismisses the permission dialog
    Then the result contains "Bluetooth"

  @allure.id:14796
  Scenario: TC-6.2.1 Approving an external (remote) request
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Remote Permission: Chain Submit"
    And the user approves the permission
    Then the result contains "Chain"

  @allure.id:14798
  Scenario: TC-6.2.3 Deny an external request blocks it
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Permissions" tab
    When the user runs "Remote Permission: Statement Submit"
    And the user denies the permission
    Then the result contains "Statement"
