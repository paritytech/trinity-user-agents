@browser @allure.label.parentSuite:browser @allure.label.suite:Tab_Shortcuts @allure.label.feature:Tab_Shortcuts
Feature: Tab keyboard shortcuts

  Tab keyboard shortcuts go through the Tab/View application menus (the same
  path the accelerators take). The selected tab is observed via the host route:
  a product tab is /product/<id>, a new tab is /new-tab/<id>. TC-4.3.1 (cycle
  tabs) is already covered by the authenticated tab-switching suite.

  Background:
    Given the app is launched
    And the user skips onboarding
    And the link-tests product is open in a tab

  @allure.id:14752
  Scenario: TC-4.3.2 Jump to a tab by number
    When the user opens an additional new tab
    Then the active tab is a new tab
    When the user jumps to tab 1
    Then the active tab is the link-tests product
    When the user jumps to tab 2
    Then the active tab is a new tab

  @allure.id:14753
  Scenario: TC-4.3.3 New-tab shortcut focuses the address bar
    When the user opens a new tab with the keyboard
    Then the address bar is focused

  @allure.id:14754
  Scenario: TC-4.3.4 Reload shortcut reloads the product webview
    When a reload probe is set in the product
    And the user reloads the current tab with the keyboard
    Then the reload probe is cleared

  @allure.id:14755
  Scenario: TC-4.3.5 Close-tab shortcut closes the current tab
    When the user opens an additional new tab
    Then the active tab is a new tab
    When the user closes the current tab with the keyboard
    Then the active tab is the link-tests product
