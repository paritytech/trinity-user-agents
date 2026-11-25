@browser @allure.label.parentSuite:browser @allure.label.suite:Address_Bar @allure.label.feature:Address_Bar
Feature: Address bar

  The browser address bar is a button showing where you are; pressing it opens
  the input surface over the screen, and that surface owns the typing,
  suggestions, ghost-completion and submit. Together they open products by
  typed domain, ghost-complete a bare label with the network's TLD, show a
  loading-progress bar while a product resolves, and normalize slashes in typed
  paths. The link-tests local product is a real product route with no auth or
  chain. Suggestions render only what is persisted, so the scenarios that just
  need the surface open assert on the surface, and the one about suggestions
  seeds products first.

  Background:
    Given the app is launched
    And the user skips onboarding

  @allure.id:14732
  Scenario: TC-4.1.1 Open a product by typing a domain
    When the link-tests product is open in a tab
    Then the active tab is the link-tests product

  # Recents + saved products are seeded into the product DB + recents localStorage
  # (no chain/auth needed) so the suggestions surface both sections. Opened from
  # the dashboard rather than over a product: the bar hands the surface whatever
  # it was showing, and a product identifier in the field filters the suggestions
  # down to that one product.
  @allure.id:14734
  Scenario: TC-4.1.3 Address bar suggestions show recents and saved products
    Given seeded products with a recent visit
    And the app reloads with onboarding skipped
    When the user focuses the address bar
    Then the address bar suggestions are visible
    And the address bar suggestions show the recents section
    And the address bar suggestions show the saved section

  @allure.id:14735
  Scenario: TC-4.1.4 Keyboard navigation and ghost-suffix autocomplete
    Given the link-tests product is open in a tab
    When the user focuses the address bar
    And the user types "foo" into the address bar
    Then the address bar ghost-completes "foo" on Tab

  # Escape and an outside click now dismiss the whole input surface, not a
  # dropdown under a still-focused field — the surface is what the suggestions
  # live in, so its disappearance is the same guarantee, observable without
  # seeding anything.
  @allure.id:14736
  Scenario: TC-4.1.5 Escape closes the address input surface
    Given the link-tests product is open in a tab
    When the user focuses the address bar
    Then the address input surface is visible
    When the user presses "Escape" in the address bar
    Then the address input surface is not visible

  @allure.id:14737
  Scenario: TC-4.1.6 Click outside closes the address input surface
    Given the link-tests product is open in a tab
    When the user focuses the address bar
    Then the address input surface is visible
    When the user clicks outside the address bar
    Then the address input surface is not visible

  @allure.id:14740
  Scenario: TC-4.1.9 Address bar shows loading progress while a product resolves
    When the user opens the link-tests product with a slow response
    Then the address bar shows the loading progress bar
    And the address bar loading progress bar disappears once the product loads

  @allure.id:14741
  Scenario: TC-4.1.10 Trailing/leading-slash normalization in typed paths
    Given the link-tests product is open in a tab
    When the user submits the link-tests domain with a trailing slash in the address bar
    Then the link-tests webview pathname is "/"
    When the user submits the link-tests path with surrounding slashes in the address bar
    Then the link-tests webview pathname is "/link-tests/"
