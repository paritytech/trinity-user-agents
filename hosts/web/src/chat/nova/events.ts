import type { RpcTap } from "./statement-store.js";

/**
 * What a Nova session reports about itself, for whoever observes it. The
 * engine and the page's provider know only these contracts; the inspector
 * (`tools/inspector/`) collects and shows what arrives.
 *
 * Events hold metadata only: shortened public accounts and ids, message kind
 * numbers, sizes, store answers, shortened error text, statement-store topics
 * and statement hashes. Never keys, shared secrets, message text or decrypted
 * payloads. Payloads go to a {@link PayloadSink}, which keeps them only while
 * the user has capture on.
 */

/** The statement an event is about. */
interface Carried {
  /** BLAKE2b-256 of the statement's encoded bytes, as 32-byte hex. */
  statement?: string;
  /**
   * For a statement received, how many times it has arrived in this session,
   * this time included. Set when the event is recorded.
   */
  arrival?: number;
}

export type ChatEventBody =
  | {
      type: "lifecycle";
      state: "starting" | "listening" | "stopped" | "failed";
      reason?: string;
      retryInMs?: number;
    }
  | {
      type: "subscription";
      stream: "requests" | "sessions";
      action: "subscribe" | "unsubscribe";
      /** How many topics the subscription's filter names. */
      topics: number;
      /**
       * The topics as 32-byte hex, in filter order. At most
       * {@link TOPIC_ID_LIMIT} are kept, so there can be fewer than `topics`.
       */
      topicIds: string[];
    }
  | ({
      type: "set-aside";
      stream: "requests" | "sessions";
      reason: string;
    } & Carried)
  | ({
      type: "request";
      requestId: string;
      peer: string;
      version: 1 | 2;
      outcome:
        | "new"
        | "known"
        | "looked up again"
        | "declined earlier"
        | "contact active"
        | "answers ours"
        | "blocked";
      problem?: string;
    } & Carried)
  | {
      type: "lookup";
      account: string;
      outcome: "found" | "missing" | "conflict" | "error";
      /** Which chains named an X25519 chat key. */
      from?: ("asset hub" | "people")[];
      reason?: string;
    }
  | ({
      type: "session";
      peer: string;
      data: "request" | "response" | "multi-device";
      requestId?: string;
      /** Content kind of each message, in order; null for one that could not be read. */
      kinds?: (number | null)[];
      duplicates?: number;
      code?: number;
      matched?: boolean;
    } & Carried)
  | ({
      type: "submit";
      purpose: "chat request" | "accept" | "messages" | "leave" | "ack";
      peer: string;
      requestId?: string;
      messages?: number;
      result: "stored" | "refused" | "error";
      status?: string;
      reason?: string;
    } & Carried)
  | { type: "action-failed"; action: string; reason: string };

export type ChatEventSink = (event: ChatEventBody) => void;

/** The most topics one subscription event keeps. */
export const TOPIC_ID_LIMIT = 32;
const REASON_LIMIT = 200;

/** A subscription event for a filter of `topicIds`, given as hex. */
export function subscriptionEvent(
  stream: "requests" | "sessions",
  action: "subscribe" | "unsubscribe",
  topicIds: readonly string[],
): ChatEventBody {
  return {
    type: "subscription",
    stream,
    action,
    topics: topicIds.length,
    topicIds: [...topicIds],
  };
}

/** `0x1234…abcd` for a public account or key. */
export const shortHex = (hex: string): string =>
  hex.length <= 14 ? hex : `${hex.slice(0, 6)}…${hex.slice(-4)}`;

/** The first eight characters of a random request or message id. */
export const shortId = (id: string): string =>
  id.length <= 8 ? id : `${id.slice(0, 8)}…`;

/**
 * Error text fit for an event: long hex runs shortened, and cut to a bounded
 * length. Engine errors carry no message text, so this is a backstop against
 * an error from elsewhere quoting bytes.
 */
export function sanitizeReason(reason: string): string {
  const shortened = reason.replace(/(0x)?[0-9a-fA-F]{16,}/g, (run) =>
    shortHex(run.startsWith("0x") ? run : `0x${run}`),
  );
  return shortened.length <= REASON_LIMIT
    ? shortened
    : `${shortened.slice(0, REASON_LIMIT)}…`;
}

export type CaptureStream = "requests" | "sessions";
export type CaptureDirection = "in" | "out";

/** Where the chat engine hands its payloads. */
export interface PayloadSink {
  statement(
    direction: CaptureDirection,
    stream: CaptureStream,
    encoded: Uint8Array,
  ): void;
  plaintext(hash: string, plaintext: Uint8Array): void;
  gap(hash: string, reason: string): void;
}

/** Where a page's Nova session sends its events and payloads. */
export interface ChatObserver {
  /** Engine events, metadata only. */
  readonly record: ChatEventSink;
  /** Payloads, and a tap for each statement store connection the session opens. */
  readonly payloads: PayloadSink & { connectionTap(): RpcTap };
  /** Forget the session: its events, its payloads, and the choice to capture them. */
  clear(): void;
}
