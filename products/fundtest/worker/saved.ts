// What a session keeps with the host through `fundingProvider.save`, so a
// restarted worker resumes it: the step it reached and the figures it cannot
// recompute. Bigints travel as decimal strings. The host keeps at most 4 KiB.

import { fromStored, toStored } from "../shared/channel";

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

export interface Saved {
  v: 1;
  /** Balance units, once known. */
  amount?: string;
  inbound?: {
    phase: InboundPhase;
    plan?: SavedPlan;
    /** When a crypto deposit address stops taking payment, Unix ms. */
    depositExpiresAt?: number;
    /** The faucet drip the session asked for, once asked. */
    drip?: SavedDrip;
    tick?: SavedTick;
    claims: SavedClaim[];
  };
  outbound?: {
    phase: OutboundPhase;
    paymentId?: `0x${string}`;
    /** Where the user asked to be paid, as typed on the payout screen. */
    destination?: string;
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

/** The record as the bytes `save` takes. */
export const writeSaved = (saved: Saved) => toStored(saved);
