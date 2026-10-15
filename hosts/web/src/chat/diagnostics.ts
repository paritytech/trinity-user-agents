/**
 * What the chat engine did, for the inspector's Chat view. These are not
 * TrUAPI frames: chat runs in the host page and talks to the statement store
 * and chain storage directly.
 *
 * Events hold metadata only: shortened public accounts, shortened request and
 * message ids, message kind numbers, sizes, store answers and error text with
 * long hex runs shortened. Never keys, shared secrets, message text, push
 * tokens or decrypted payloads. They are kept in memory, at most
 * {@link DIAGNOSTICS_LIMIT} of them, and cleared when the session changes.
 */

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
      topics: number;
    }
  | {
      type: "set-aside";
      stream: "requests" | "sessions";
      reason: string;
    }
  | {
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
        | "answers ours";
      problem?: string;
    }
  | {
      type: "lookup";
      account: string;
      outcome: "found" | "missing" | "conflict" | "error";
      /** Which chains named an X25519 chat key. */
      from?: ("asset hub" | "people")[];
      reason?: string;
    }
  | {
      type: "session";
      peer: string;
      data: "request" | "response" | "multi-device";
      requestId?: string;
      /** Content kind of each message, in order; null for one that could not be read. */
      kinds?: (number | null)[];
      duplicates?: number;
      code?: number;
      matched?: boolean;
    }
  | {
      type: "submit";
      purpose: "chat request" | "accept" | "messages" | "ack";
      peer: string;
      requestId?: string;
      messages?: number;
      result: "stored" | "refused" | "error";
      status?: string;
      reason?: string;
    }
  | { type: "action-failed"; action: string; reason: string };

export type ChatEvent = ChatEventBody & { seq: number; at: number };

export type ChatEventSink = (event: ChatEventBody) => void;

export const DIAGNOSTICS_LIMIT = 300;
const REASON_LIMIT = 200;

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

/** A bounded, in-memory list of chat events. */
export class ChatDiagnostics {
  private events: ChatEvent[] = [];
  private seq = 0;
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly limit = DIAGNOSTICS_LIMIT,
    private readonly now: () => number = Date.now,
  ) {}

  readonly record: ChatEventSink = (event) => {
    this.events.push({ ...event, seq: ++this.seq, at: this.now() });
    if (this.events.length > this.limit)
      this.events.splice(0, this.events.length - this.limit);
    this.notify();
  };

  /** Events, oldest first. */
  list(): readonly ChatEvent[] {
    return this.events;
  }

  clear(): void {
    this.events = [];
    this.notify();
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify(): void {
    for (const listener of this.listeners) listener();
  }
}

const join = (parts: (string | undefined | false)[]): string =>
  parts.filter((part) => typeof part === "string" && part !== "").join(" · ");

/** One line describing an event. */
export function describeEvent(event: ChatEventBody): string {
  switch (event.type) {
    case "lifecycle":
      return join([
        event.state,
        event.reason,
        event.retryInMs !== undefined && `retry in ${event.retryInMs / 1000}s`,
      ]);
    case "subscription":
      return `${event.action} ${event.stream} (${event.topics} topic${event.topics === 1 ? "" : "s"})`;
    case "set-aside":
      return `${event.stream}: ${event.reason}`;
    case "request":
      return join([
        `v${event.version} ${event.requestId} from ${event.peer}`,
        event.outcome,
        event.problem,
      ]);
    case "lookup":
      return join([
        `${event.account}: ${event.outcome}`,
        event.from !== undefined &&
          event.from.length > 0 &&
          `key on ${event.from.join(" and ")}`,
        event.reason,
      ]);
    case "session":
      return join([
        `${event.data} ${event.requestId ?? ""} from ${event.peer}`.replace(
          /\s+/g,
          " ",
        ),
        event.kinds !== undefined &&
          `kinds [${event.kinds.map((kind) => kind ?? "?").join(", ")}]`,
        event.duplicates !== undefined &&
          event.duplicates > 0 &&
          `${event.duplicates} already seen`,
        event.code !== undefined && `code ${event.code}`,
        event.matched !== undefined &&
          (event.matched ? "matches ours" : "not ours"),
      ]);
    case "submit":
      return join([
        `${event.purpose} to ${event.peer}`,
        event.requestId,
        event.messages !== undefined &&
          `${event.messages} message${event.messages === 1 ? "" : "s"}`,
        event.result,
        event.status,
        event.reason,
      ]);
    case "action-failed":
      return `${event.action}: ${event.reason}`;
  }
}
