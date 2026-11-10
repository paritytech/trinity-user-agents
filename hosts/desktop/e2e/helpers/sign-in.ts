import { type Page } from '@playwright/test';

import { OnboardingPage } from '../page-objects/OnboardingPage';

import { errorMessage } from './errors';
import { STUCK_PAIRING_TIMEOUT, VERY_LONG_TIMEOUT } from './timeouts';

/**
 * Sign-in resilience helpers shared by the `authenticated` and `chatPair`
 * fixtures.
 *
 * Both fixtures pair against the nightly chain (`paseo-next-v2`) whose
 * attestation/finality occasionally lags: the chain reports the identity before it is
 * usable, so the core wedges in `AuthState.Authenticating` ("Completing pairing…") and
 * never reaches `Connected`. A full relaunch (or storage reset) after a short settle
 * gives the chain time to catch up.
 *
 * The two levers here:
 *  - `waitForDashboardOrStuck` fails a wedged attempt early (≈`STUCK_PAIRING_TIMEOUT`)
 *    instead of burning the whole `VERY_LONG_TIMEOUT`, so retries are cheap;
 *  - `withSignInRetries` runs the attempt up to `SIGN_IN_ATTEMPTS` times with a
 *    settle delay, in one place so both fixtures share the same policy.
 */

export const SIGN_IN_ATTEMPTS = 3;
export const SIGN_IN_RETRY_DELAY_MS = 10_000;

/** Thrown when pairing is wedged in `AuthState.Authenticating` past the threshold. */
export class StuckPairingError extends Error {
  constructor(seconds: number) {
    super(`Pairing stuck on "Completing pairing…" for ${seconds}s without reaching /dashboard`);
    this.name = 'StuckPairingError';
  }
}

/**
 * Thrown when the app shows the "Limit Reached" pairing error — the peer reported no
 * free allowance slots (`no free statement-store slot for device registration`): the
 * identity's per-day device-registration budget (`LiteStmtStoreSlotsPerPeriod`) is
 * exhausted. Retrying with the same identity is pointless within a run (the budget is
 * daily and each attempt uses a fresh device account), so retries abort and the caller
 * clears the signer's saved pairings instead — see `signInWithReset`.
 */
export class PairingLimitError extends Error {
  constructor() {
    super('Pairing rejected with "Limit Reached" — this identity\'s daily allowance-slot budget is exhausted');
    this.name = 'PairingLimitError';
  }
}

/**
 * Resolve once the app reaches `/dashboard`. Aborts early with:
 *  - `StuckPairingError` if the handshake spinner stays up continuously for
 *    `STUCK_PAIRING_TIMEOUT` without navigating — the nightly-lag wedge;
 *  - `PairingLimitError` if the "Limit Reached" (no-free-slots) error panel
 *    appears — deterministic for the rest of the day, so waiting the full
 *    `VERY_LONG_TIMEOUT` (and retrying) only wastes budget.
 * A genuinely slow-but-healthy handshake that clears late still wins the
 * race; a false abort merely triggers one extra relaunch, never a hard failure.
 */
export async function waitForDashboardOrStuck(page: Page): Promise<void> {
  const onboarding = new OnboardingPage(page);

  const navigated = page.waitForURL(/dashboard/, { timeout: VERY_LONG_TIMEOUT });

  const stuck = (async () => {
    // Wait for the spinner to appear at all (bounded by VERY_LONG_TIMEOUT). If it
    // never does, the navigated promise governs the outcome instead.
    await onboarding.completingPairing.waitFor({ state: 'visible', timeout: VERY_LONG_TIMEOUT });
    // It's up — give the handshake a window to clear. If navigation happens in
    // the meantime, `navigated` settles the race first and this branch is
    // abandoned. If we reach the end with the spinner still visible, it's wedged.
    await page.waitForTimeout(STUCK_PAIRING_TIMEOUT);
    if (await onboarding.completingPairing.isVisible()) {
      throw new StuckPairingError(Math.round(STUCK_PAIRING_TIMEOUT / 1000));
    }
    // Cleared but not navigated yet — defer to `navigated` for the final word.
    await navigated;
  })();

  const limitReached = (async () => {
    await onboarding.pairingLimitReachedError.waitFor({ state: 'visible', timeout: VERY_LONG_TIMEOUT });
    throw new PairingLimitError();
  })();
  // When another racer settles first, this one still rejects later (waitFor
  // timeout or the error panel appearing after navigation is impossible, but
  // the timeout rejection is guaranteed) — mark it handled to avoid an
  // unhandled-rejection crash after the race is over.
  limitReached.catch(() => {});

  await Promise.race([navigated, stuck, limitReached]);
}

/**
 * Run a sign-in `attempt` up to `attempts` times. Between failures it waits
 * `delayMs` (chain settle) and runs the optional `beforeRetry` hook (e.g. a
 * storage reset) before the next try. Surfaces the last error if all attempts
 * fail. Logging format matches the prior per-fixture loops.
 */
export async function withSignInRetries<T>(
  attempt: () => Promise<T>,
  opts: {
    label: string;
    attempts?: number;
    delayMs?: number;
    beforeRetry?: () => Promise<void>;
  },
): Promise<T> {
  const attempts = opts.attempts ?? SIGN_IN_ATTEMPTS;
  const delayMs = opts.delayMs ?? SIGN_IN_RETRY_DELAY_MS;
  let lastError: unknown;

  for (let i = 1; i <= attempts; i++) {
    try {
      return await attempt();
    } catch (err) {
      lastError = err;
      if (i === attempts) break;
      // The slot budget is per-day: every retry pairs a fresh device account, so
      // it hits the same exhausted budget. Surface immediately so the caller can
      // reset the signer (`signInWithReset`) instead of burning attempts here.
      if (err instanceof PairingLimitError) break;
      console.warn(`[${opts.label}] sign-in ${i}/${attempts} failed (${errorMessage(err)}), retry in ${delayMs / 1000}s`);
      await new Promise(resolve => setTimeout(resolve, delayMs));
      if (opts.beforeRetry) await opts.beforeRetry();
    }
  }

  throw lastError;
}

/**
 * Both failures are unrecoverable by retry against the same signer state: a wedged
 * pairing means the chain never finalised this device, and "Limit Reached" means the
 * identity's daily statement-store slot budget is spent. Clearing the saved pairings is
 * what unblocks recovery — `truapi-host` refuses to rotate an identity while any paired
 * device still depends on it.
 *
 * Unlike the bot-era heal this replaces, there is no permanent-vs-random distinction to
 * make: every slot is a durable identity, so the reset applies to all of them.
 */
export function shouldResetSigner(err: unknown): boolean {
  return err instanceof StuckPairingError || err instanceof PairingLimitError;
}

/**
 * The two methods a reset needs. Narrower than `SigningHost` on purpose: it states the
 * dependency honestly and lets a test hand in a plain object with no type assertion.
 */
type ResettableSigner = {
  stop(): Promise<void>;
  removeDevices(): Promise<void>;
};

/**
 * Sign in with the shared retry policy; if the attempts exhaust on a wedged pairing or
 * an exhausted slot budget, clear the signer's saved pairings and try once more.
 *
 * The first phase is deliberately short (2 + 2): the real recovery is the reset, so
 * spending the full budget before reaching it only delays it.
 */
export async function signInWithReset(opts: {
  label: string;
  signingHost: ResettableSigner;
  attempt: () => Promise<void>;
  beforeRetry?: () => Promise<void>;
  retryDelayMs?: number;
}): Promise<void> {
  try {
    await withSignInRetries(opts.attempt, {
      label: opts.label,
      attempts: 2,
      delayMs: opts.retryDelayMs,
      beforeRetry: opts.beforeRetry,
    });

    return;
  } catch (err) {
    if (!shouldResetSigner(err)) throw err;

    console.warn(`[${opts.label}] signer wedged (${errorMessage(err)}), clearing pairings and retrying`);
    // Order matters: the process must be down before its pairings are cleared.
    await opts.signingHost.stop();
    await opts.signingHost.removeDevices();
    if (opts.beforeRetry) await opts.beforeRetry();

    await withSignInRetries(opts.attempt, {
      label: `${opts.label}:reset`,
      attempts: 2,
      delayMs: opts.retryDelayMs,
      beforeRetry: opts.beforeRetry,
    });
  }
}
