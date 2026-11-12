import type { TrUApiClient } from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import { blake2b256 } from "../../bridges/primitives/crypto.js";
import {
  fromHex,
  fromUtf8,
  toHex,
  utf8,
} from "../../bridges/primitives/bytes.js";
import { readMessage, type BoardMessage } from "./messages.js";

/**
 * Small Market's board: its messages as statements in the People chain's
 * Statement Store, through the market's own TrUAPI `statementStore` and
 * `resourceAllocation`. The host asks before the first post spends an
 * allowance; the core signs as the market's allowance account. Nothing here
 * goes through the Locket bridge.
 *
 * The store keeps the latest statement per account and channel, so one
 * account has one live listing and one live order at a time, and a seller
 * who is away misses an order replaced in the meantime.
 */

/** How long a post lives. */
const LIFETIME_S = 3600;
/** The furthest ahead a statement may expire to be read: a lifetime and some clock skew. */
const LATEST_EXPIRY_MS = 2 * LIFETIME_S * 1000;
const MAX_DATA_BYTES = 4096;

const TOPIC = toHex(blake2b256(utf8("small-market/board/v1"))) as HexString;
const channelOf = (type: BoardMessage["type"]) =>
  toHex(blake2b256(utf8(`small-market/channel/${type}`))) as HexString;

/** A message read from the board, with its statement's signer and expiry. */
export interface Heard {
  /** The statement's signer, `0x` hex: the poster's allowance account. */
  signer: string;
  message: BoardMessage;
  /** Milliseconds since the epoch. */
  expires: number;
}

export interface Board {
  /** Post `message`; answers when it expires, in milliseconds since the epoch. */
  post(message: BoardMessage): Promise<number>;
  close(): void;
}

type BoardClient = Pick<TrUApiClient, "statementStore" | "resourceAllocation">;

/** What a TrUAPI error says, for a person. Never the raw value. */
function errorText(error: unknown): string {
  let value = error;
  for (let depth = 0; depth < 3; depth++) {
    if (typeof value !== "object" || value === null) break;
    const { reason, value: inner } = value as {
      reason?: unknown;
      value?: unknown;
    };
    if (typeof reason === "string") return reason;
    value = inner;
  }
  const kind = (error as { tag?: unknown } | null)?.tag;
  return typeof kind === "string" ? kind : "unknown error";
}

/** Follow the board and post to it as this product. */
export function openBoard(
  client: BoardClient,
  hear: (heard: Heard) => void,
  problem: (reason: string) => void,
): Board {
  let allowed = false;
  let lastExpiry = 0n;
  const subscription = client.statementStore
    .subscribe({ request: { tag: "MatchAll", value: [TOPIC] } })
    .subscribe({
      next: (page) => {
        const now = Date.now();
        for (const statement of page.statements) {
          if (statement.proof.tag !== "Sr25519") continue;
          const expires = Number((statement.expiry ?? 0n) >> 32n) * 1000;
          const data = fromHex(statement.data ?? "0x");
          if (
            expires <= now ||
            expires > now + LATEST_EXPIRY_MS ||
            data.length > MAX_DATA_BYTES
          )
            continue;
          let message: BoardMessage | null;
          try {
            message = readMessage(JSON.parse(fromUtf8(data)));
          } catch {
            continue;
          }
          if (message !== null)
            hear({
              signer: statement.proof.value.signer.toLowerCase(),
              message,
              expires,
            });
        }
      },
      error: (error) => problem(errorText(error)),
      complete: () => problem("the board subscription ended"),
    });
  return {
    async post(message) {
      const data = utf8(JSON.stringify(message));
      if (data.length > MAX_DATA_BYTES) throw new Error("Message too large.");
      if (!allowed) {
        const granted = await client.resourceAllocation.request({
          resources: [{ tag: "StatementStoreAllowance" }],
        });
        if (granted.isErr())
          throw new Error(`No posting allowance: ${errorText(granted.error)}`);
        if (granted.value.outcomes[0] !== "Allocated")
          throw new Error(
            granted.value.outcomes[0] === "Rejected"
              ? "Posting needs an allowance. You declined it."
              : "No posting allowance is available.",
          );
        allowed = true;
      }
      const now = BigInt(Math.floor(Date.now() / 1000) + LIFETIME_S) << 32n;
      const expiry = now > lastExpiry ? now : lastExpiry + 1n;
      lastExpiry = expiry;
      const statement = {
        expiry,
        channel: channelOf(message.type),
        topics: [TOPIC],
        data: toHex(data) as HexString,
      };
      const proof =
        await client.statementStore.createProofAuthorized(statement);
      if (proof.isErr())
        throw new Error(`Signing failed: ${errorText(proof.error)}`);
      const submitted = await client.statementStore.submit({
        ...statement,
        proof: proof.value.proof,
      });
      if (submitted.isErr()) {
        allowed = false;
        throw new Error(`The store refused it: ${errorText(submitted.error)}`);
      }
      return Number(expiry >> 32n) * 1000;
    },
    close: () => subscription.unsubscribe(),
  };
}
