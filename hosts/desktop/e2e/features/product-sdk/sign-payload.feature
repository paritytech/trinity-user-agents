@product-sdk @allure.label.parentSuite:authenticated @allure.label.suite:host-playground @allure.label.feature:Signing
Feature: Sign Payload review

  Verify the Sign Payload / Transaction review screen that a transaction-producing
  host action opens before signing, using the host-playground test product.
  The review screen is shared by SignPayloadModal and CreateTransactionModal; the
  "Create Transaction with Product Account" action opens it (createTransaction).

  @allure.id:14865
  Scenario: TC-8.2.1 Sign payload review screen shows account, network and call title
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Create Transaction with Product Account"
    And the user opens the signing review screen
    Then the signing review shows account, network and call title
    When the user cancels the signing review

  @allure.id:14866
  Scenario: TC-8.2.2 More details expands arguments and call data
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Create Transaction with Product Account"
    And the user opens the signing review screen
    Then the signing review details show arguments and call data

  # SKIPPED (no host-action path to a custom chain): the custom-chain warning +
  # hidden-fee branch only renders when chainService.canInspectSigning(chain) is
  # false, i.e. signing against a chain the host cannot decode. Every Signing
  # action in host-playground targets a known, inspectable chain, so there is
  # no product action that reaches this branch. Custom chains can only be added
  # via Settings against a live ws: node (blocked by renderer CSP in e2e — see
  # Module 9.2). Left manual until a custom-chain signing fixture exists.
  @allure.id:14867 @skip
  Scenario: TC-8.2.3 Custom-chain signing shows raw-call warning and hides fee
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab

  # SKIPPED (no deterministic batch-review path in e2e): the batch behaviour hint
  # renders only when the review decodes a utility.batch* call. The single batch
  # host action — "Sign & Submit Batch (2 contract writes)" — performs an on-chain
  # utility.batch_all SUBMIT against a contract chain; in the e2e environment it
  # does not surface the shared SignPayload/CreateTransaction review modal at all
  # (no signing modal nor allocation dialog appears — the action's contract-chain
  # submit path never reaches the review screen). The only review-opening action,
  # "Create Transaction with Product Account", builds a non-batch call, so there is
  # no host action that opens the review with a batch call. Testids
  # (sign-review-batch-hint) and the assertion step are in place; left manual until
  # a createTransaction-with-batch fixture exists.
  @allure.id:14868 @skip
  Scenario: TC-8.2.4 Batch call behaviour hint shown for utility.batch variants
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab

  # SKIPPED (gating not deterministically observable): "Continue to Sign" is
  # disabled while peopleChainStatus !== 'connected'. In an authenticated session
  # the People chain connects during sign-in, before any product opens, so by the
  # time the review modal mounts the button is already enabled. The disabled→enabled
  # transition only happens transiently during chain (re)connection, which the test
  # cannot force deterministically. Left manual.
  @allure.id:14869 @skip
  Scenario: TC-8.2.5 Continue to Sign disabled until People chain connected
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab

  @allure.id:14870
  Scenario: TC-8.2.6 Double-click on Continue to Sign does not start two signings
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Signing" tab
    When the user runs "Create Transaction with Product Account"
    And the user opens the signing review screen
    And the user double-clicks Continue to Sign
    Then the result contains "0x"
