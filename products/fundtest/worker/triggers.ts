// Failure cases a tester picks by the cents of the amount.

import { ONE_CASH } from "./config";

/** What a session is made to do instead of the happy path.
 *
 * - `refund`: an inbound payment is refunded before anything is credited, and
 *   an outbound payout fails after the release.
 * - `retry`: the credit is split over two top-ups.
 * - `slow`: the payment arrives only after the quoted ETA has passed. */
export type Trigger = "refund" | "retry" | "slow";

const CENT = ONE_CASH / 100n;

const BY_CENTS: Record<string, Trigger> = {
  "13": "refund",
  "23": "retry",
  "33": "slow",
};

/** The trigger an amount in balance units selects, if any. */
export function triggerOf(amount: bigint | undefined): Trigger | undefined {
  if (amount === undefined) return undefined;
  const cents = (amount / CENT) % 100n;
  return BY_CENTS[cents.toString().padStart(2, "0")];
}
