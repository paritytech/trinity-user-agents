@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Product_Widgets @allure.label.feature:Product_Widgets
Feature: Product widgets

  Product widgets render a product's widget executable inside a dashboard card.
  These cases overlap Module 3 (widget cards) but are distinct TestOps cases, so
  each links its own id and reuses the dashboard widget-card flow.

  Background:
    Given the user is authenticated
    And the user is on the dashboard

  # Overlaps TC-3.2.4 / TC-3.3.1 (a different TestOps case, id 14780): a thin
  # render check that a product widget mounts and loads its webview body.
  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14780 @skip
  Scenario: TC-5.2.1 Render a product widget on the dashboard
    Given the user starts with only a CoinFlip widget on the dashboard
    Then the product widget body loads its webview

  # Seed a dashboard widget card pointing at a committed product that carries NO
  # widget executable, so `loadExecutableArchive(kind='widget')` returns null
  # synchronously and `ProductWidgetBody` renders its "Domain not found" state.
  @allure.id:14781
  Scenario: TC-5.2.2 Widget shows not found when its executable cannot be resolved
    Given a dashboard widget for an unresolvable product is seeded
    Then the product widget shows the not-found state

  # Overlaps TC-3.3.2 (a different TestOps case, id 14782): reload the widget via
  # its card reload control and prove the webview remounted (probe cleared).
  # @skip: blocked by the deployment, not by the app — see the note in
  # `products/product-actions.feature` (no widget-capable product is published on
  # the environment `AUTH_ENVIRONMENT_ID` names).
  @allure.id:14782 @skip
  Scenario: TC-5.2.3 Reload a product widget via its reload control
    Given the user starts with only a CoinFlip widget on the dashboard
    And a reload probe is set in the product widget
    When the user reloads the product widget from its card menu
    Then the product widget reload probe is cleared
