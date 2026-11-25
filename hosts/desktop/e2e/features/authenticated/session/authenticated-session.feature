@authenticated @allure.label.parentSuite:authenticated @allure.label.suite:Authenticated_Session @allure.label.feature:Authenticated_Session
Feature: Authenticated Session (Stable)

  These tests run on a shared pre-authenticated session on the Paseo Next network.
  Sign-in happens once per worker, all tests reuse the same session.

  @allure.id:14694
  Scenario: TC-2.2.1 SSO root & identity keys fetched from the PApp on sign-in
    Given the user is authenticated
    Then the SSO session and identity keys are persisted in localStorage

  @allure.id:14696
  Scenario: TC-2.2.3 User info is visible in authenticated session
    Given the user is authenticated
    Then the authenticated user info is visible in the top bar

  @allure.id:14697
  Scenario: TC-2.2.4 Session data persists in authenticated session
    Given the user is authenticated
    Then the authenticated session data exists in localStorage
