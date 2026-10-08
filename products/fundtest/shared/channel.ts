// What the worker and the provider's screens tell each other. Both run as the
// same product, so they share one storage namespace: the worker writes a
// session's view before it presents a screen, the screen writes what the user
// did, and `localStorage.subscribe` carries each write to the other side.

/** Route of each provider screen the worker presents. */
export type ScreenRoute = "/card" | "/bank" | "/payout";

/** What a screen shows, written by the worker before it presents the screen. */
export interface SessionView {
  intent: string;
  direction: "In" | "Out";
  rail: "Card" | "Bank" | "Crypto";
  /** Symbol the user pays with (In) or receives (Out). */
  asset: string;
  /** CASH base units, as a decimal string. Absent when the user picks it on
   *  the screen. */
  amount?: string;
  /** What the user pays (In) or receives (Out), in the asset's minor unit,
   *  as a decimal string. */
  price?: string;
  /** Decimals of `asset`. */
  decimals: number;
  /** Bank details for a bank transfer in. */
  bank?: {
    beneficiary: string;
    account: string;
    bankCode: string;
    reference: string;
  };
}

/** What the user did on a screen. `amount`, in CASH base units as a decimal
 *  string, is set when the session left the amount to the screen. */
export type ScreenAction =
  | { kind: "card-paid"; amount?: string }
  | { kind: "bank-sent"; amount?: string }
  | { kind: "payout-confirmed"; destination: string };

/** Storage key of a session's view. */
export const viewKey = (intent: string) => `fundtest:view:${intent}`;

/** Storage key of the user's action on a session's screen. */
export const actionKey = (intent: string) => `fundtest:action:${intent}`;

/** Storage key naming the session a route's screen is for, for a host that
 *  drops the query string of a presented route. */
export const activeKey = (route: ScreenRoute) => `fundtest:active:${route}`;

/** The route the worker presents, with the session on its query string. */
export const screenUrl = (route: ScreenRoute, intent: string) =>
  `${route}?intent=${encodeURIComponent(intent)}`;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** JSON as the hex the storage API takes. */
export function toStored(value: unknown): `0x${string}` {
  const bytes = encoder.encode(JSON.stringify(value));
  return `0x${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

/** The JSON a stored hex value holds, or `undefined` for none or bad data. */
export function fromStored<T>(hex: string | undefined): T | undefined {
  if (!hex || hex.length < 4) return undefined;
  const bytes = Uint8Array.from(hex.slice(2).match(/.{2}/g) ?? [], (pair) =>
    Number.parseInt(pair, 16),
  );
  try {
    return JSON.parse(decoder.decode(bytes)) as T;
  } catch {
    return undefined;
  }
}
