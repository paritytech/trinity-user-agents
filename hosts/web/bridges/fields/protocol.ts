/** Noncanonical, web-only field-provider demo. No shared TrUAPI contract. */
export const FIELD_DEMO = "web-field-demo/1";

/** The two fields this experiment understands. */
export type Field = "address" | "phone";

/** Which Statement Store the demo posts to. Simulation is always named. */
export interface FieldNetwork {
  kind: "live" | "simulated";
  /** The network's display name, such as `Paseo Next v2`. */
  name: string;
}

/**
 * Why a bridge request failed, for a product to choose its reaction without
 * reading the message: `cancelled` is the user's choice in a host sheet.
 */
export type FieldErrorCode =
  | "cancelled"
  | "invalid"
  | "unavailable"
  | "limit"
  | "busy"
  | "network"
  | "session"
  | "unsupported";

/** A bridge failure with its code. The message is for people; it never names a protected value. */
export class FieldError extends Error {
  constructor(
    readonly code: FieldErrorCode,
    message: string,
  ) {
    super(message);
  }
}

/** A failed reply as it crosses the bridge. */
export interface FieldErrorReply {
  code: FieldErrorCode;
  message: string;
}

/** What a provider product may learn about itself: never the values. */
export interface ProviderStatus {
  registered: boolean;
  name: string | null;
  fields: Field[];
  /** The declared fields that hold a value in this host session. */
  saved: Field[];
}

/** Public listing. The key needs explicit trusted demo pairing, not just its signature. */
export interface Listing {
  v: 1;
  type: "listing";
  id: string;
  item: string;
  seller: string;
  key: string;
}

/** Public purchase envelope. Only the recipient host opens `box`. */
export interface Purchase {
  v: 1;
  type: "purchase";
  id: string;
  listing: string;
  box: string;
}

/** A public message verified against a Statement Store signature. */
export interface Received {
  sender: string;
  message: Listing | Purchase;
  /** When its statement expires, in milliseconds since the epoch. */
  expires: number;
}

const id = (value: unknown): value is string =>
  typeof value === "string" && /^[0-9a-f-]{36}$/.test(value);

/** Whether a public key has exactly 32 bytes of lowercase hex. */
export const publicKey = (value: unknown): value is string =>
  typeof value === "string" && /^0x[0-9a-f]{64}$/.test(value);

/** Read bounded public demo messages, without reflecting untrusted text in errors. */
export function readMessage(value: unknown): Listing | Purchase | null {
  if (typeof value !== "object" || value === null) return null;
  const message = value as Record<string, unknown>;
  if (message.v !== 1 || !id(message.id)) return null;
  if (
    message.type === "listing" &&
    typeof message.item === "string" &&
    message.item.length > 0 &&
    message.item.length <= 80 &&
    publicKey(message.seller) &&
    publicKey(message.key)
  )
    return {
      v: 1,
      type: "listing",
      id: message.id,
      item: message.item,
      seller: message.seller,
      key: message.key,
    };
  if (
    message.type === "purchase" &&
    id(message.listing) &&
    typeof message.box === "string" &&
    /^0x[0-9a-f]{96,3000}$/.test(message.box) &&
    message.box.length % 2 === 0
  )
    return {
      v: 1,
      type: "purchase",
      id: message.id,
      listing: message.listing,
      box: message.box,
    };
  return null;
}
