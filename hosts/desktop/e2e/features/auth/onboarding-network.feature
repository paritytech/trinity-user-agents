@auth @allure.label.parentSuite:auth @allure.label.suite:Onboarding_Network
Feature: Onboarding Network Selection

  Fresh-Electron auth-project scenarios that exercise the onboarding network
  selector (Environment picker) without pairing — no signer required. The
  selected segment carries `aria-pressed=true`; switching one reloads the app and
  restarts the pairing (QR) flow on the new network.

  @allure.label.feature:Network_Default @allure.id:14681
  Scenario: TC-1.2.1 Default environment is Paseo Next V2 on a fresh start
    Given the app is launched in autotest mode
    And the QR code is displayed on onboarding screen
    Then the selected environment is "nightly"

  # Switches to whatever OTHER channel the build offers (not a hard-coded id), and self-skips
  # when fewer than 2 channels are configured.
  @allure.label.feature:Network_Switch @allure.id:14682
  Scenario: TC-1.2.2 Switching environment reloads the app and re-pairs on the new network
    Given the app is launched in autotest mode
    And the QR code is displayed on onboarding screen
    And the selected environment is "nightly"
    When the user switches to a different environment
    Then the selected environment changed from "nightly"
    And the QR code is displayed on onboarding screen

  # TC-1.2.3 (14683) Network segment buttons are disabled while pairing is in progress — SKIPPED.
  # The selector is only locked in a single narrow branch:
  #   isNetworkSelectionDisabled = connectionState === 'pairing' && !showQR && !hasError && !handshakeLoading
  # (OnboardingScreen.tsx). While the QR is displayed and waiting for a phone the buttons stay
  # ENABLED (showQR === true), and they stay enabled during the connection states. The ONLY disabled
  # window is the on-chain Pending phase ("Completing pairing…" spinner) of a live pairing, which
  # requires a real signer handshake AND winning a race against handshake completion + scarce
  # bot slots. The pre-QR-payload window (mount → first payload) is likewise sub-second. This is the
  # same sub-second-race class that already left TC-2.1.5 (connect-gating) skipped. Not
  # deterministically observable; left manual.

  # TC-1.2.4 (14684) Pairing error shows retry and allows re-authentication — SKIPPED.
  # The error UI (inline "retry" Button in renderQrBoxContent + accountSetup `onboardingRetryButton`)
  # is reached only when handshakeState.tag === 'Failed' (host-papp PairingStatus.step === 'pairingError').
  # That Failed/pairingError state is produced by the host-papp peer over the real on-chain handshake;
  # there is no renderer-side fault-injection hook to force it, and pointing at an unreachable network
  # yields the connection-panel offline/reaching states (deriveOnboardingConnectionState), not a
  # handshake 'Failed'. No deterministic way to induce the failure → left manual.

  # TC-1.2.5 (14685) "Logging in" info toast appears when pairing reaches pending — SKIPPED.
  # The toast is not implemented in the current build: the `feature.onboarding.loggingInToast`
  # i18n string ("Logging in... Just a moment") is orphaned — it is defined in en.json but never
  # referenced in source (grep-confirmed). The only Pending-state surface is the in-box
  # "Completing pairing…" spinner (showHandshakeProgress in OnboardingScreen.tsx); the sole toast in
  # the onboarding flow is the `toastError` for generic pairing FAILURES, not a pending info toast.
  # No UI to assert on → left manual until the toast is wired.
