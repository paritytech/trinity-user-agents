/**
 * Standard timeouts for e2e waits.
 *
 * Use one of these instead of hardcoding numbers in tests/page objects.
 */
/**
 * For an action that is retried by an outer poll and must therefore fail fast
 * instead of burning the whole budget on one attempt (e.g. clicking a control
 * that the app may re-render away under us).
 */
export const SHORT_TIMEOUT = 5_000;
export const DEFAULT_TIMEOUT = 30_000;
export const LONG_TIMEOUT = 60_000;
export const VERY_LONG_TIMEOUT = 90_000;

/**
 * How long the "Completing pairing…" handshake spinner may stay up before the
 * sign-in helper declares the pairing wedged (nightly attestation lag) and
 * aborts the attempt early to relaunch + retry. Safely above a healthy
 * handshake (a few seconds) yet well under `VERY_LONG_TIMEOUT`, so a stuck
 * attempt fails fast enough to fit a 3rd retry inside the project budget.
 */
export const STUCK_PAIRING_TIMEOUT = 45_000;
