@browser @allure.label.parentSuite:browser @allure.label.suite:History @allure.label.feature:History
Feature: History navigation

  Each tab keeps its own back/forward history. An in-product pushState on the
  visible link-tests tab updates the host route, which records a new history
  entry — so the toolbar back/forward buttons enable and disable accordingly.
  The tab opens at "/link-tests", so going back from "/push-target" returns
  there.

  Background:
    Given the app is launched
    And the user skips onboarding
    And the link-tests product is open in a tab

  # Opening the product goes new-tab -> product, so a new-tab history entry already
  # precedes it (back is enabled). The invariant under test is the enable/disable
  # transition tied to the per-tab stack, anchored on the forward tip.
  @allure.id:14756
  Scenario: TC-4.4.1 Back and Forward buttons enable per tab history
    Then the browser forward button is disabled
    When the user dispatches a pushState in the link-tests product to "/push-target"
    Then the host route pathname ends with "/push-target"
    And the browser back button is enabled
    And the browser forward button is disabled
    When the user clicks the browser back button
    Then the host route pathname ends with "/link-tests"
    And the browser forward button is enabled
    When the user clicks the browser forward button
    Then the host route pathname ends with "/push-target"
    And the browser forward button is disabled

  @allure.id:14757
  Scenario: TC-4.4.2 Back and Forward via menu accelerators
    When the user dispatches a pushState in the link-tests product to "/push-target"
    Then the host route pathname ends with "/push-target"
    When the user navigates back via the menu
    Then the host route pathname ends with "/link-tests"
    When the user navigates forward via the menu
    Then the host route pathname ends with "/push-target"

  @allure.id:14758
  Scenario: TC-4.4.3 Back and Forward buttons are toolbar-positioned and left-aligned
    Then the back button is positioned left of the forward button
