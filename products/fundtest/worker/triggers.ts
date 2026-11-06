// Failure cases a tester picks by the cents of the amount.

import { ONE_CASH } from "./config";

/** What a session is made to do instead of the happy path.
 *
 * - `refund`: an inbound payment is refunded before anything is credited, and
 *   an outbound payout fails after the release and is refunded from the
 *   reserve account.
 * - `retry`: the credit is split over two top-ups.
 * - `slow`: the payment arrives only after the quoted ETA has passed.
 * - `decline`: a card or bank payment is declined before any money moves.
 * - `payout-retry`: an outbound payout fails twice and is retried before it
 *   goes through.
 * - `cancel-refuse`: a cancel after money moved is refused rather than
 *   answered with an offer. */
export type Trigger =
  | "refund"
  | "retry"
  | "slow"
  | "decline"
  | "payout-retry"
  | "cancel-refuse";

const CENT = ONE_CASH / 100n;

const BY_CENTS: Record<string, Trigger> = {
  "13": "refund",
  "23": "retry",
  "33": "slow",
  "43": "decline",
  "63": "payout-retry",
  "73": "cancel-refuse",
};

/** The trigger an amount in balance units selects, if any. */
export function triggerOf(amount: bigint | undefined): Trigger | undefined {
  if (amount === undefined) return undefined;
  const cents = (amount / CENT) % 100n;
  return BY_CENTS[cents.toString().padStart(2, "0")];
}

/** Where a session stops as if its worker crashed, chosen by cents `.51` to
 *  `.54`: just after an external action and before the step after it is
 *  saved, so the restarted worker must resume from its last checkpoint.
 *
 * | Cents | Inbound                                   | Outbound                              |
 * | ----- | ----------------------------------------- | ------------------------------------- |
 * | `.51` | after the faucet drip is asked for        | after `payment.request`               |
 * | `.52` | after the first conversion tick           | after `Collecting` is reported        |
 * | `.53` | after `payment.topUp`                     | after the payout's `Converting`       |
 * | `.54` | after `Crediting` is reported             | none                                  |
 */
export type KillPoint = 1 | 2 | 3 | 4;

/** The kill point an amount in balance units selects, if any. */
export function killPointOf(amount: bigint | undefined): KillPoint | undefined {
  if (amount === undefined) return undefined;
  const cents = Number((amount / CENT) % 100n);
  return cents >= 51 && cents <= 54 ? ((cents - 50) as KillPoint) : undefined;
}
