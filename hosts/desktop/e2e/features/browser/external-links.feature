@browser @allure.label.parentSuite:browser @allure.label.suite:Link_Navigation @allure.label.feature:External_Links
Feature: External links

  An external http(s) link inside a product webview is not navigated in place —
  the sandbox denies the navigation and hands the URL to the system browser via
  shell.openExternal.

  The hand-off is gated on the product's Open-External-URL permission, and an
  e2e build never prompts for one (`promptForUnmatchedRemoteAccess: false`), so
  the grant is seeded before the product opens rather than answered in a dialog.

  Background:
    Given the app is launched
    And the user skips onboarding
    And the link-tests product may open "https://example.com/" externally
    And the app reloads with onboarding skipped
    And the link-tests product is open in a tab

  @allure.id:14763
  Scenario: TC-4.5.5 External http(s) link opens in the system browser
    When the system browser opener is stubbed
    And the user clicks the link-tests button "external-link"
    Then the system browser is asked to open "https://example.com/"
    And the webview pathname ends with "/link-tests"
