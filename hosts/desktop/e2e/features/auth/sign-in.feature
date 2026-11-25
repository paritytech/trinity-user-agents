@auth @allure.label.parentSuite:auth @allure.label.suite:Sign_in
Feature: Sign In

  @allure.label.feature:Sign_in_PaseoNextV2 @allure.id:14689
  Scenario: TC-2.1.1 Sign in on Paseo Next V2 via signing bot reaches the dashboard
    Given the app is launched in autotest mode
    And the user selects the "nightly" environment
    And the QR code is displayed on onboarding screen
    When the user signs in on "nightly"
    Then the user is redirected to dashboard
    And user info is visible in the top bar

  # TC-2.1.3 (14691) Sign in on Paseo Next (v1) — SKIPPED: the e2e environment catalog
  # (VITE_ENVIRONMENTS) configures only two channels — `nightly` (Paseo Next V2 →
  # network paseo-next-v2) and `unstable` (PreviewNet → previewnet). There is no
  # separate "Paseo Next v1" channel to select or pair against, so the case is not
  # drivable. Left manual until a v1 channel is configured for e2e.


  @allure.label.feature:Sign_in_Previewnet @allure.id:14690 @skip
  Scenario: TC-2.1.2 Sign in on Previewnet environment
    Given the app is launched in autotest mode
    And the user selects the "unstable" environment
    And the QR code is displayed on onboarding screen
    When the user signs in on "unstable"
    Then the user is redirected to dashboard
    And session data exists in localStorage
    And user info is visible in the top bar

  @allure.label.feature:Log_out @allure.id:14698
  Scenario: TC-2.3.1 Logout clears session and redirects to onboarding
    Given the app is launched in autotest mode
    And the user is signed in on "nightly"
    When the user clicks logout
    Then user secrets are removed from localStorage
    And the user is redirected to onboarding screen
