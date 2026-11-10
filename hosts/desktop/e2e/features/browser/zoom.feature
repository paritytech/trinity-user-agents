@browser @allure.label.parentSuite:browser @allure.label.suite:Zoom @allure.label.feature:Zoom
Feature: Product zoom

  Zoom acts on the on-screen product webview's guest content (per product,
  persisted). The transient indicator surfaces only on an actual zoom change
  and auto-fades. The link-tests local product provides a real product route
  with no auth or chain.

  Background:
    Given the app is launched
    And the user skips onboarding
    And the link-tests product is open in a tab

  @allure.id:14769
  Scenario: TC-4.7.1 Zoom in, out, and reset on a product page
    When the user zooms in
    Then the zoom indicator shows "120%"
    When the user zooms in
    Then the zoom indicator shows "144%"
    When the user resets zoom
    Then the zoom indicator shows "100%"
    When the user zooms out
    Then the zoom indicator shows "83%"

  @allure.id:14770
  Scenario: TC-4.7.2 Zoom indicator buttons adjust zoom
    When the user zooms in
    Then the zoom indicator shows "120%"
    When the user clicks the zoom indicator "in" button
    Then the zoom indicator shows "144%"
    When the user clicks the zoom indicator "out" button
    Then the zoom indicator shows "120%"
    When the user clicks the zoom indicator "reset" button
    Then the zoom indicator shows "100%"

  @allure.id:14771
  Scenario: TC-4.7.3 Zoom indicator does not flash on product reload
    When the user zooms in
    Then the zoom indicator shows "120%"
    When the zoom indicator fades
    And the user reloads the product from the menu
    Then the zoom indicator does not appear

  @allure.id:14772
  Scenario: TC-4.7.4 Zoom shortcut is inert off product routes
    When the user navigates to the dashboard
    And the user zooms in
    Then the zoom indicator does not appear
