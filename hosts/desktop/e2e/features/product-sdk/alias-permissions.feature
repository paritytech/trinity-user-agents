@product-sdk @manual-permissions @allure.label.parentSuite:authenticated @allure.label.suite:Permissions @allure.label.feature:Alias_Permissions
Feature: Alias permissions

  A product requesting an account alias (host-playground "Get Product Account
  Alias") pops the host alias-permission dialog. The feature is tagged
  manual-permissions so the auto-approver is off and the test drives the dialog
  itself. The dialog decision is local (resolve of a confirm()), so the dialog
  closing is the deterministic outcome — the on-chain VRF alias round-trip that
  follows an approval is not asserted here.

  The modal offers one Allow: the core answers a review with a boolean and keeps
  the stored decision, so there is no once/always split to drive (TODO(truapi) in
  `AliasPermissionModal`). TC-6.5.2 therefore covers both answers the modal does
  offer — deny, then approve.

  # @skip: the alias request never reaches the dialog on this deployment. Every
  # signer slot is a freshly attested identity, and `Get Product Account Alias`
  # goes through `accountsProvider.listRingVrfKeys(owner)` first, which fails with
  # "No peopl.paseo key is registered for the People Lite ring" — so there is no
  # alias review to assert on, whatever the host does. Un-skip once the signer
  # identities register a People Lite ring key (host-playground exposes a Ring VRF
  # key registration card, so a setup step could do it) or the deployment seeds one.

  @allure.id:14809 @skip
  Scenario: TC-6.5.1 Approve alias access
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Accounts" tab
    When the user runs "Get Product Account Alias"
    Then the alias permission dialog is shown
    When the user approves alias access
    Then the alias permission dialog is dismissed

  @allure.id:14810 @skip
  Scenario: TC-6.5.2 Deny alias access, then approve a second request
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Accounts" tab
    When the user runs "Get Product Account Alias"
    Then the alias permission dialog is shown
    When the user denies alias access
    Then the alias permission dialog is dismissed
    When the user runs "Get Product Account Alias"
    Then the alias permission dialog is shown
    When the user approves alias access
    Then the alias permission dialog is dismissed

  @allure.id:14908 @skip
  Scenario: TC-10.3.5 Alias permission request screen renders correctly
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Accounts" tab
    When the user runs "Get Product Account Alias"
    Then the alias permission dialog is shown
    And the permission dialog screenshot is taken as "alias-permission-request"
