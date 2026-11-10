@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Networks @allure.label.feature:Custom_Chains
Feature: Custom chains

  Adding a custom chain validates the endpoint URL client-side, then discovers the
  node over WebSocket. A malformed URL is rejected before any connection; a
  syntactically valid but unreachable endpoint surfaces a connection-failed toast
  once discovery times out.

  Note: the happy-path cases (add a reachable chain, reject a duplicate, remove a
  chain — TC-9.2.1 / 9.2.3 / 9.2.5) require a successful `ws://` discovery against
  a live node, and are still left manual (`-` in
  docs/regression-testplan-900-cases.md). The renderer CSP no longer blocks them —
  `connect-src` permits plain `ws:` for the smoldot light client — so a deterministic
  local fake node is now reachable and these could be automated.

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14893
  Scenario: TC-9.2.2 Reject an invalid (non-ws) endpoint URL
    When the user opens the custom chains settings
    And the user enters the custom chain endpoint "http://not-a-websocket.example"
    And the user submits the custom chain
    Then a custom chain error "Invalid URL" is shown

  @allure.id:14895
  Scenario: TC-9.2.4 Connection failure surfaces an error toast
    When the user opens the custom chains settings
    And the user enters the custom chain endpoint "wss://127.0.0.1:9999"
    And the user submits the custom chain
    Then a custom chain error "Connection failed" is shown
