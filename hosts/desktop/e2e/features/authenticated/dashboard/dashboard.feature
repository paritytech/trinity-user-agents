@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Dashboard @allure.label.feature:Dashboard
Feature: Dashboard

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14705
  Scenario: TC-3.1.1 Dashboard renders after onboarding
    Then the dashboard grid is shown

  @allure.id:14706
  Scenario: TC-3.1.2 Empty dashboard shows the empty state with Add Widget CTA
    When the user removes all dashboard widgets
    Then the empty dashboard state is shown

  @allure.id:14707
  Scenario: TC-3.1.3 Home button returns to the dashboard
    Given the user opens "host-playground" in a new tab
    When the user returns to the dashboard via the home button
    Then the dashboard is shown

  # A two-page dashboard layout is seeded directly (each page one widget card),
  # so the toolbar renders two page-indicator dots and selecting the second dot
  # activates page two. Seeding sidesteps the manual "fill a page with 8+ heavy
  # widgets / cross-page drag" path that left this case manual before.
  @allure.id:14708
  Scenario: TC-3.1.4 Pagination dots switch dashboard pages
    Given a two-page dashboard is seeded
    Then the dashboard shows 2 page indicator dots
    And dashboard page 1 is active
    When the user selects dashboard page 2
    Then dashboard page 2 is active

  @allure.id:14709
  Scenario: TC-3.1.5 Dashboard scrolls horizontally on a small window width
    When the user shrinks the window
    Then the dashboard grid scrolls horizontally

  @allure.id:14711
  Scenario: TC-3.2.1 Open the Add Widget modal from the toolbar
    When the user opens the Add Widget modal
    Then the Add Widget modal is open

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14712 @skip
  Scenario: TC-3.2.2 Add Widget modal lists widget-capable products
    When the user opens the Add Widget modal
    Then the Add Widget modal is open
    And the Add Widget catalog lists the published widget product "coinflip"

  @allure.id:14713
  Scenario: TC-3.2.3 Search filters the Add Widget product list
    When the user opens the Add Widget modal
    And the user searches the Add Widget catalog for "Favorites"
    Then the Add Widget catalog lists a matching entry "Favorites"
    When the user searches the Add Widget catalog for "zzzqqxno"
    Then the Add Widget catalog shows no results

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14714 @skip
  Scenario: TC-3.2.4 Adding a product as a widget updates the dashboard layout
    Given the user opens "coinflipgame03" in a new tab
    When the user adds the open product to the dashboard as a "Large" widget
    And the user navigates back to the dashboard
    Then a product widget appears on the dashboard

  @allure.id:14715
  Scenario: TC-3.2.5 Adding a product to Favorites places a 1x1 icon
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user adds the product to favorites from the actions menu
    And the user navigates back to the dashboard
    Then a favorites folder with an icon appears on the dashboard

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14716 @skip
  Scenario: TC-3.2.6 Already-on-dashboard widget shows Open instead of Add
    Given the user opens "coinflipgame03" in a new tab
    And the user adds the open product to the dashboard as a "Large" widget
    Then re-opening the add-to-dashboard modal shows Open for the widget

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14717 @skip
  Scenario: TC-3.2.7 Adding to favorites that are already present is a no-op
    Given the user opens "host-playground" in a new tab
    And the user opens the product actions menu
    And the user adds the product to favorites from the actions menu
    Then re-opening the add-to-dashboard modal shows the product already in favorites

  # --- 3.3 Product Widget Card ------------------------------------------------
  # Each scenario first clears the seeded default widget, then adds CoinFlip as a
  # Large widget so the sole product widget on the grid is deterministic.

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14718 @skip
  Scenario: TC-3.3.1 Product widget body loads its webview
    Given the user starts with only a CoinFlip widget on the dashboard
    Then the product widget body loads its webview

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14719 @skip
  Scenario: TC-3.3.2 Reload a product widget from its card topbar
    Given the user starts with only a CoinFlip widget on the dashboard
    And a reload probe is set in the product widget
    When the user reloads the product widget from its card menu
    Then the product widget reload probe is cleared

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14720 @skip
  Scenario: TC-3.3.3 Open product widget fullscreen from its card
    Given the user starts with only a CoinFlip widget on the dashboard
    When the user opens the product widget fullscreen from its card
    Then the app navigates to the product fullscreen route

  @allure.id:14721
  Scenario: TC-3.3.4 1x1 product shortcut card opens product fullscreen
    Given the user removes all dashboard widgets
    And the user opens "host-playground" in a new tab
    And the user opens the product actions menu
    And the user adds the product to favorites from the actions menu
    And the user navigates back to the dashboard
    When the user opens the favorited product from its dashboard icon
    Then the app navigates to the product fullscreen route

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14722 @skip
  Scenario: TC-3.3.5 Resize a widget via its card menu
    Given the user starts with only a CoinFlip widget on the dashboard
    Then resizing the product widget to a smaller size shrinks its footprint

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14723 @skip
  Scenario: TC-3.3.6 Clean up dashboard via widget menu
    Given the user starts with only a CoinFlip widget on the dashboard
    When the user cleans up the dashboard from the widget card menu
    Then the product widget remains on the dashboard

  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14724 @skip
  Scenario: TC-3.3.7 Remove a widget via its card menu
    Given the user starts with only a CoinFlip widget on the dashboard
    When the user removes the product widget via its card menu
    Then no product widget remains on the dashboard

  # --- 3.4 Favorites Folder ---------------------------------------------------

  @allure.id:14726
  Scenario: TC-3.4.1 Favorites folder appears once a favorite is added
    Given the user removes all dashboard widgets
    And the dashboard has no favorites folder
    When the user opens "host-playground" in a new tab
    And the user opens the product actions menu
    And the user adds the product to favorites from the actions menu
    And the user navigates back to the dashboard
    Then a favorites folder with an icon appears on the dashboard

  @allure.id:14727
  Scenario: TC-3.4.2 Remove folder keeps the page open
    Given the user removes all dashboard widgets
    And the user opens "host-playground" in a new tab
    And the user opens the product actions menu
    And the user adds the product to favorites from the actions menu
    And the user navigates back to the dashboard
    And a favorites folder with an icon appears on the dashboard
    When the user removes the favorites folder via its card menu
    Then the favorites folder is removed
    And the dashboard is shown
