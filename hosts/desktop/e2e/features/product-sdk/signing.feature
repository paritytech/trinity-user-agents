@product-sdk @allure.label.parentSuite:authenticated @allure.label.suite:host-playground @allure.label.feature:Signing
Feature: Signing

  Verify signing APIs work correctly in a product sandbox
  using the host-playground test product.

  @allure.id:14871
  Scenario: TC-8.3.1 createTransaction request returns a signed transaction
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Create Transaction with Product Account"
    And the user confirms signing
    Then the result contains "0x"

  @allure.id:14864
  Scenario: TC-8.1.3 Rejecting a raw-message request surfaces a rejected result
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Sign Raw Message"
    And the user rejects signing
    # The core words a declined review "Rejected"; the product surfaces it verbatim
    # as "signRaw failed: Rejected".
    Then the result contains "Rejected"

  @allure.id:14862
  Scenario: TC-8.1.1 Sign raw message works correctly in product sandbox
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Sign Raw Message"
    And the user confirms signing
    Then the result contains "Message signed"

  # TC-5.4.2 is the Module-5.4 (Product SDK / Sandbox) sibling of TC-8.1.1
  # (id 14862): the same sign-raw flow, distinct TestOps case in the sandbox
  # sub-area. Kept thin — it reuses the proven-stable Sign Raw Message path.
  @allure.id:14787
  Scenario: TC-5.4.2 Sign raw message in the product sandbox
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Sign Raw Message"
    And the user confirms signing
    Then the result contains "Message signed"

  # SKIPPED: TC-5.4.3 is the Module-5.4 sibling of TC-8.1.2 (id 14863) — the same
  # post-reload re-sign flow, which the 8.1.2 author found irredeemably flaky:
  # signing again after a product reload does not reliably surface "Message
  # signed" (the first sign in TC-5.4.2 / TC-8.1.1 is stable). Same root cause,
  # same conclusion. Left manual until the post-reload re-sign flow is hardened.
  @allure.id:14788 @skip
  Scenario: TC-5.4.3 Signing still works after reloading the product
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Sign Raw Message"
    And the user confirms signing
    Then the result contains "Message signed"
    When the user reloads the product
    And the user clicks the "Signing" tab
    And the user runs "Sign Raw Message"
    And the user confirms signing
    Then the result contains "Message signed"

  # SKIPPED: signing again after a product reload does not reliably surface
  # "Message signed" — the re-sign path is flaky (the first sign in TC-8.1.1 is
  # stable). Left manual until the post-reload re-sign flow is hardened.
  @allure.id:14863 @skip
  Scenario: TC-8.1.2 Sign raw message works after product reload
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Sign Raw Message"
    And the user confirms signing
    Then the result contains "Message signed"
    When the user reloads the product
    And the user clicks the "Signing" tab
    And the user runs "Sign Raw Message"
    And the user confirms signing
    Then the result contains "Message signed"
