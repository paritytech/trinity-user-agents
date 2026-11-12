import { blake2b256 } from "../../bridges/primitives/crypto.js";
import { toHex, utf8 } from "../../bridges/primitives/bytes.js";
import {
  readRecipientKey,
  type RecipientKey,
} from "../../bridges/locket/protocol.js";

/**
 * Small Market's own messages on its Statement Store board. They are the
 * market's protocol, not the host's: the host only makes the seller's
 * recipient key and seals and opens the envelope an order carries.
 */

/** A listing: the item and the seller's recipient key, which names the seller's account. */
export interface Listing {
  v: 1;
  type: "listing";
  id: string;
  item: string;
  seller: RecipientKey;
}

/** An order: which listing, and the buyer's details sealed to the seller's key. */
export interface Order {
  v: 1;
  type: "order";
  id: string;
  listing: string;
  /** The sealed envelope, `0x` hex. */
  envelope: string;
}

export type BoardMessage = Listing | Order;

const isId = (value: unknown): value is string =>
  typeof value === "string" && /^[0-9a-f-]{36}$/.test(value);

/** Read a board message from untrusted JSON, or null. Copies only known members. */
export function readMessage(value: unknown): BoardMessage | null {
  if (typeof value !== "object" || value === null) return null;
  const message = value as Record<string, unknown>;
  if (message.v !== 1 || !isId(message.id)) return null;
  if (message.type === "listing") {
    const seller = readRecipientKey(message.seller);
    return typeof message.item === "string" &&
      message.item.length > 0 &&
      message.item.length <= 80 &&
      seller !== null
      ? { v: 1, type: "listing", id: message.id, item: message.item, seller }
      : null;
  }
  if (
    message.type === "order" &&
    isId(message.listing) &&
    typeof message.envelope === "string" &&
    /^0x(?:[0-9a-f]{2}){64,1200}$/.test(message.envelope)
  )
    return {
      v: 1,
      type: "order",
      id: message.id,
      listing: message.listing,
      envelope: message.envelope,
    };
  return null;
}

/**
 * The context an order's envelope is sealed under: the listing and the
 * order. It keeps an envelope from opening as another order of this market;
 * it proves nothing about who ordered.
 */
export const orderContext = (listing: string, order: string): string =>
  toHex(blake2b256(utf8(`small-market/order/v1|${listing}|${order}`)));
