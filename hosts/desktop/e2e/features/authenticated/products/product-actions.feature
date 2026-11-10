@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Product_Actions @allure.label.feature:Product_Actions
Feature: Product actions menu

  The product actions menu (•••) in the address bar exposes per-product actions,
  including opening the product's settings page.

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  @allure.id:14776
  Scenario: TC-5.1.1 Open the product actions menu from the address bar
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    Then the product actions menu is open

  @allure.id:14777
  Scenario: TC-5.1.2 Open product settings from the actions menu
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user opens product settings from the actions menu
    Then the app navigates to the product settings page

  @allure.id:14778
  Scenario: TC-5.1.3 Reload an open product
    Given the user opens "host-playground" in a new tab
    And a reload probe is set in the open product
    When the user reloads the open product via the address bar control
    Then the open product reload probe is cleared

  @allure.id:14779
  Scenario: TC-5.1.4 Product SPA content remains visible over time
    Given the user opens "host-playground" in a new tab
    Then the open product content remains visible after settling

  # @skip: no widget-capable product is published on the nightly (Paseo Next V2)
  # deployment — the Add Widget catalog holds only the native Chat and Favorites
  # entries, and `coinflipgame03` (a PreviewNet-era `.dot` name) does not resolve
  # there at all. Verified by driving the real app: `coinflipgame03.paseo`,
  # `dotns-search.paseo` and `test-dapp-01.paseo` all render "Can't find product",
  # while `host-playground.paseo` loads and offers "Add to Favorites" — it carries
  # no widget executable, so it cannot stand in here. Un-skip once a product with a
  # widget executable is published on the environment `AUTH_ENVIRONMENT_ID` names.
  @allure.id:14728 @skip
  Scenario: TC-3.5.1 Widget product shows Add to Dashboard in the actions menu
    Given the user opens "coinflipgame03" in a new tab
    When the user opens the product actions menu
    Then the actions menu offers "Add to dashboard"

  @allure.id:14729
  Scenario: TC-3.5.2 Widget-less product shows Add to Favorites toggle in the actions menu
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    Then the actions menu offers "Add to Favorites"

  @allure.id:14730
  Scenario: TC-3.5.3 Remove from Favorites via the actions menu
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user adds the product to favorites from the actions menu
    And the user opens the product actions menu
    Then the actions menu offers "Remove from Favorites"
    When the user removes the product from favorites from the actions menu
    And the user opens the product actions menu
    Then the actions menu offers "Add to Favorites"

  # Same address-bar ••• → Open Settings path as TC-5.1.2 (id 14777); a distinct
  # TestOps case (id 14731), so it is linked as its own scenario.
  @allure.id:14731
  Scenario: TC-3.5.4 Open product Settings from the actions menu
    Given the user opens "host-playground" in a new tab
    When the user opens the product actions menu
    And the user opens product settings from the actions menu
    Then the app navigates to the product settings page
