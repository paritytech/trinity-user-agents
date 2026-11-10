@product-sdk @allure.label.parentSuite:authenticated @allure.label.suite:Permissions @allure.label.feature:Allowances
Feature: Resource allocation / allowances

  A product requesting a resource allocation (host-playground "Allowances" tab,
  requestResourceAllocation) pops the host "Allowance request" modal. That modal
  is never auto-approved by the e2e dialog handlers (unlike permission/alias
  dialogs), so it always renders and the test asserts on it directly without the
  manual-permissions opt-out. The modal is the same component for a first or an
  increase ("onExisting: Increase") request; only its rendering is asserted here,
  as granting it would require an on-chain Polkadot App round-trip.

  @allure.id:14812
  Scenario: TC-6.6.1 Allowance update modal on existing allocation
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Allowances" tab
    When the user runs "Allocate StatementStore Allowance"
    Then the allowance request dialog is shown

  @allure.id:14789
  Scenario: TC-5.4.4 Resource allocation request modal
    Given the user is authenticated
    And the test product "host-playground" is opened
    And the user clicks the "Allowances" tab
    When the user runs "Allocate Bulletin Allowance"
    Then the allowance request dialog is shown
