// What a session keeps in the worker's own storage, under its `intent`, so a
// restarted worker resumes it: the step it reached and the figures it cannot
// recompute. Bigints travel as decimal strings.

import { fromStored, stateKey, toStored } from "../shared/channel";
import { truapi, unwrap } from "./host";

/** The step an inbound session reached. */
export type InboundPhase =
  /** Nothing reported yet. */
  | "new"
  /** Waiting for the user on the provider's screen, or for a crypto deposit. */
  | "awaiting-user"
  /** The user says the fiat payment left; it has not arrived. */
  | "fiat-sent"
  /** The fiat payment arrived; the deposit is about to be made on chain. */
  | "fiat-received"
  /** The faucet's drip to the session account may be in flight. */
  | "funding"
  /** Waiting for the deposit to be final on Asset Hub. */
  | "arriving"
  /** Converting the deposit and sending the CASH to People. */
  | "converting"
  /** Claiming the CASH into the user's balance. */
  | "crediting"
  | "done";

/** The step an outbound session reached. */
export type OutboundPhase =
  | "new"
  | "awaiting-user"
  /** `payment.request` is about to be made, or was made and not answered. */
  | "requesting"
  /** Waiting for the user's payment to complete. */
  | "collecting"
  /** The payment completed; the simulated payout is under way. */
  | "paying-out"
  /** The payout failed; the user is being refunded from the reserve account. */
  | "refunding"
  | "done";

/** A conversion route as getcash records it (`recordedRoute`). */
export interface SavedRoute {
  tier: "pool" | "psm" | "dotusd";
  external?: "USDT" | "USDC";
  feeRate?: number;
}

/** The figures a conversion was sized with, fixed once at planning. */
export interface SavedPlan {
  route: SavedRoute;
  /** Symbol of the asset the session account receives on Asset Hub. */
  depositSymbol: string;
  /** What the session account must receive, smallest units. */
  deposit: string;
  /** The gate getcash's pipeline waits for on the fee-priced tiers. */
  quotedDeposit?: string;
  remoteFeeBuffer: string;
  keepNativeForFees: string;
  slippagePct: number;
}

/** getcash's `TickState`, persisted between ticks. */
export interface SavedTick {
  attempts: number;
  psmRefusals: number;
  xcmSubmitted: boolean;
  peopleAtXcm: string;
  fundsSeenAt: number | null;
  nonceAtSubmit: number | null;
  inclusionBlock: number | null;
}

/** A drip the faucet was asked for. */
export interface SavedDrip {
  /** When it was asked for, Unix ms. */
  at: number;
  /** The faucet's extrinsic hash on Paseo Asset Hub, when it answered. */
  hash?: `0x${string}`;
}

/** One top-up the session started. */
export interface SavedClaim {
  id: `0x${string}`;
  amount: string;
  attempt: number;
}

/** The cancel offer the session made, and whether the user accepted it. The
 *  terms are kept as offered: the refund an accepted offer settles with must be
 *  the one the user saw. */
export interface SavedCancel {
  state: "offered" | "accepted";
  /** When the offer lapses, Unix ms. The core decides that it lapsed. */
  expiresAt: number;
  /** Balance units. */
  fee: string;
  /** Balance units. */
  refund: string;
}

export interface Saved {
  v: 1;
  /** Balance units, once known. */
  amount?: string;
  /** A cancel offer the user has not declined. */
  cancel?: SavedCancel;
  inbound?: {
    phase: InboundPhase;
    plan?: SavedPlan;
    /** When a crypto deposit address stops taking payment, Unix ms. */
    depositExpiresAt?: number;
    /** When the simulated card or bank payment arrives, Unix ms. Kept so a
     *  restart does not start the wait again. */
    arrivesAt?: number;
    /** Whether part of the user's payment reached the provider, after which a
     *  cancel is a request rather than a stop. */
    paymentSeen?: boolean;
    /** The faucet drip the session asked for, once asked. */
    drip?: SavedDrip;
    tick?: SavedTick;
    claims: SavedClaim[];
  };
  outbound?: {
    phase: OutboundPhase;
    paymentId?: `0x${string}`;
    /** What the user's payment moved, balance units: the requested amount, or
     *  what a partial claim took. Refunds never exceed it. */
    debited?: string;
    /** Whether the host took `Converting` after the release. */
    converted?: boolean;
    /** Whether the host took the payout's outcome. */
    payoutReported?: boolean;
    /** Where the user asked to be paid, as typed on the payout screen. */
    destination?: string;
    /** When the simulated payout arrives, Unix ms. Kept so a restart does
     *  not start the wait again. */
    payoutAt?: number;
    /** The top-up refunding a failed payout, fixed before it is made. */
    refund?: { id: `0x${string}`; amount: string };
  };
}

/** A fresh record for a session that has done nothing yet. */
export const freshSaved = (): Saved => ({ v: 1 });

/** The record a session saved, or a fresh one when it saved none this
 *  provider can read. */
export function readSaved(hex: string | undefined): Saved {
  const saved = fromStored<Saved>(hex);
  return saved?.v === 1 ? saved : freshSaved();
}

/** The record as the bytes kept in storage. */
export const writeSaved = (saved: Saved) => toStored(saved);

/** The record session `intent` keeps in storage, as stored, if any. */
export async function loadSaved(intent: string): Promise<string | undefined> {
  const client = await truapi();
  const { value } = await unwrap(
    client.localStorage.read({ key: stateKey(intent) }),
    "read the session's state",
  );
  return value;
}

/** Keeps `saved` as session `intent`'s record, in place of the one before. */
export async function storeSaved(intent: string, saved: Saved): Promise<void> {
  const client = await truapi();
  await unwrap(
    client.localStorage.write({
      key: stateKey(intent),
      value: writeSaved(saved),
    }),
    "store the session's state",
  );
}
