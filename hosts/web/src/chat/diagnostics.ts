/**
 * What the chat engine did, for the inspector's Chat view. These are not
 * TrUAPI frames: chat runs in the host page and talks to the statement store
 * and chain storage directly.
 *
 * Events hold metadata only: shortened public accounts, shortened request and
 * message ids, message kind numbers, sizes, store answers, error text with
 * long hex runs shortened, and the statement-store topics a subscription
 * names, in full. A topic is a public routing identifier the store sees on
 * every statement, a hash and not a key. Never keys, shared secrets, message
 * text, push tokens or decrypted payloads. They are kept in memory, at most
 * {@link DIAGNOSTICS_LIMIT} of them, and cleared when the session changes.
 */

import { ACKNOWLEDGED } from "./protocol.js";

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
/** The most topics one subscription event keeps. */
export const TOPIC_ID_LIMIT = 32;
const REASON_LIMIT = 200;
const TOPIC_ID = /^0x[0-9a-f]{64}$/;

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

/**
 * Keep only what a topic field may hold: 32-byte lowercase hex, at most
 * {@link TOPIC_ID_LIMIT}. Anything else is dropped, so no other value can be
 * recorded in full through this field.
 */
const allowedTopicIds = (topicIds: readonly string[]): string[] =>
  topicIds
    .map((topic) => topic.toLowerCase())
    .filter((topic) => TOPIC_ID.test(topic))
    .slice(0, TOPIC_ID_LIMIT);

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
  private evicted = 0;
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly limit = DIAGNOSTICS_LIMIT,
    private readonly now: () => number = Date.now,
  ) {}

  readonly record: ChatEventSink = (event) => {
    const body: ChatEventBody =
      event.type === "subscription"
        ? { ...event, topicIds: allowedTopicIds(event.topicIds) }
        : event;
    this.events.push({ ...body, seq: ++this.seq, at: this.now() });
    if (this.events.length > this.limit) {
      const over = this.events.length - this.limit;
      this.events.splice(0, over);
      this.evicted += over;
    }
    this.notify();
  };

  /** Events, oldest first. */
  list(): readonly ChatEvent[] {
    return this.events;
  }

  /** Events dropped to keep the limit since the list was last cleared. */
  dropped(): number {
    return this.evicted;
  }

  clear(): void {
    this.events = [];
    this.evicted = 0;
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

/** A short name for each event type. */
export const EVENT_TYPE_LABELS: Record<ChatEventBody["type"], string> = {
  lifecycle: "engine",
  subscription: "subscribe",
  "set-aside": "set aside",
  request: "request",
  lookup: "lookup",
  session: "session",
  submit: "submit",
  "action-failed": "failed",
};

/**
 * Where an event's data went: received from the statement store, sent to
 * it, read from chain storage, or the engine's own state.
 */
export type EventDirection = "in" | "out" | "read" | "engine";

export function eventDirection(event: ChatEventBody): EventDirection {
  switch (event.type) {
    case "request":
    case "session":
    case "set-aside":
      return "in";
    case "submit":
      return "out";
    case "lookup":
      return "read";
    default:
      return "engine";
  }
}

/**
 * How an event ended: `ok` for a success, `warning` for data set aside or a
 * refusal, `error` for a failure, and `info` for an event that reports
 * neither.
 */
export type EventOutcome = "ok" | "info" | "warning" | "error";

export function eventOutcome(event: ChatEventBody): EventOutcome {
  switch (event.type) {
    case "lifecycle":
      if (event.state === "failed") return "error";
      return event.state === "listening" ? "ok" : "info";
    case "set-aside":
      return "warning";
    case "action-failed":
      return "error";
    case "request":
      return event.problem !== undefined ? "warning" : "info";
    case "lookup":
      if (event.outcome === "found") return "ok";
      return event.outcome === "error" ? "error" : "warning";
    case "submit":
      if (event.result === "stored") return "ok";
      return event.result === "error" ? "error" : "warning";
    case "session":
      return answersOurs(event) ? "ok" : "info";
    case "subscription":
      return "info";
  }
}

const answersOurs = (event: ChatEventBody): boolean =>
  event.type === "session" &&
  event.data === "response" &&
  event.matched === true &&
  event.code === ACKNOWLEDGED;

/** Whether an event is a warning or an error. */
export const isProblem = (event: ChatEventBody): boolean => {
  const outcome = eventOutcome(event);
  return outcome === "warning" || outcome === "error";
};

/** One or two words for an event's outcome, or null when it reports none. */
export function outcomeWord(event: ChatEventBody): string | null {
  if (eventOutcome(event) === "info") return null;
  switch (event.type) {
    case "lifecycle":
      return event.state;
    case "set-aside":
      return "set aside";
    case "action-failed":
      return "failed";
    case "request":
      return "problem";
    case "lookup":
      return event.outcome;
    case "submit":
      return event.result;
    case "session":
      return "acknowledged";
    case "subscription":
      return null;
  }
}

/** Names of the message content kinds in `protocol.ts`. */
const KIND_LABELS: Record<number, string> = {
  0: "text",
  1: "push token",
  7: "reply",
  14: "chat accepted",
  15: "rich text",
  17: "device added",
  18: "device removed",
  20: "device chat accepted",
};

/** A message content kind's name, keeping its number: `device added (17)`. */
export function kindLabel(kind: number | null): string {
  if (kind === null) return "unreadable";
  const name = KIND_LABELS[kind];
  return name === undefined ? `kind ${kind}` : `${name} (${kind})`;
}

/** Consecutive messages of the same kind, counted. */
export function kindRuns(
  kinds: readonly (number | null)[],
): { kind: number | null; count: number }[] {
  const runs: { kind: number | null; count: number }[] = [];
  for (const kind of kinds) {
    const last = runs.at(-1);
    if (last !== undefined && last.kind === kind) last.count += 1;
    else runs.push({ kind, count: 1 });
  }
  return runs;
}

const describeKinds = (kinds: readonly (number | null)[]): string =>
  kindRuns(kinds)
    .map(({ kind, count }) =>
      count === 1 ? kindLabel(kind) : `${kindLabel(kind)} ×${count}`,
    )
    .join(", ");

const responseCode = (code: number): string =>
  code === ACKNOWLEDGED ? `acknowledged (${code})` : `code ${code}`;

const plural = (count: number, noun: string): string =>
  `${count} ${noun}${count === 1 ? "" : "s"}`;

const FREE_TEXT = new Set(["reason", "problem", "status"]);

/** The event with its free text shortened again, as a backstop. */
function withTextSanitized<Event extends ChatEventBody>(event: Event): Event {
  const copy: Record<string, unknown> = { ...event };
  for (const key of FREE_TEXT)
    if (typeof copy[key] === "string")
      copy[key] = sanitizeReason(copy[key] as string);
  return copy as Event;
}

/** One line describing an event, with message kinds and answer codes named. */
export function eventSummary(event: ChatEventBody): string {
  if (event.type !== "session") return describeEvent(withTextSanitized(event));
  return join([
    `${event.data} ${event.requestId ?? ""} from ${event.peer}`.replace(
      /\s+/g,
      " ",
    ),
    event.kinds !== undefined &&
      `${plural(event.kinds.length, "message")}: ${describeKinds(event.kinds)}`,
    event.duplicates !== undefined &&
      event.duplicates > 0 &&
      `${event.duplicates} already seen`,
    event.code !== undefined && responseCode(event.code),
    event.matched !== undefined &&
      (event.matched ? "matches ours" : "not ours"),
  ]);
}

/** What links events to each other: the shortened account or id they carry. */
export type RelatedBy = "account" | "id";

/** The shortened account and request id an event names, if any. */
export function relatedKeys(
  event: ChatEventBody,
): Partial<Record<RelatedBy, string>> {
  switch (event.type) {
    case "request":
    case "submit":
    case "session":
      return {
        account: event.peer,
        ...(event.requestId !== undefined ? { id: event.requestId } : {}),
      };
    case "lookup":
      return { account: event.account };
    default:
      return {};
  }
}

/** One labelled value of an event, for its detail view. */
export interface EventField {
  label: string;
  value: string;
  /** Set when other events can be found by this value. */
  related?: RelatedBy;
}

const field = (
  label: string,
  value: string | number | undefined,
  related?: RelatedBy,
): EventField[] =>
  value === undefined
    ? []
    : [{ label, value: String(value), ...(related ? { related } : {}) }];

/** A field of free text, shortened again like {@link eventRecord} does. */
const textField = (label: string, value: string | undefined): EventField[] =>
  field(label, value === undefined ? undefined : sanitizeReason(value));

/** The event's own values, labelled. Message kinds are left to the caller. */
export function eventFields(event: ChatEventBody): EventField[] {
  switch (event.type) {
    case "lifecycle":
      return [
        ...field("State", event.state),
        ...textField("Reason", event.reason),
        ...field(
          "Retry in",
          event.retryInMs !== undefined
            ? `${event.retryInMs / 1000} s`
            : undefined,
        ),
      ];
    case "subscription":
      return [
        ...field("Stream", event.stream),
        ...field("Action", event.action),
        ...field(
          "Topics",
          event.topicIds.length === event.topics
            ? event.topics
            : `${event.topics}, the first ${event.topicIds.length} kept`,
        ),
        ...event.topicIds.map((topic, index) => ({
          label:
            event.stream === "requests"
              ? "Request inbox topic"
              : `Session topic ${index + 1}`,
          value: topic,
        })),
        ...field(
          "Topic is",
          event.stream === "requests"
            ? 'blake2b-256("chat-request" ‖ identity account)'
            : "keyed by a contact's shared chat secret, one per contact",
        ),
      ];
    case "set-aside":
      return [
        ...field("Stream", event.stream),
        ...textField("Reason", event.reason),
      ];
    case "request":
      return [
        ...field("Request id", event.requestId, "id"),
        ...field("From", event.peer, "account"),
        ...field("Version", `v${event.version}`),
        ...field("Outcome", event.outcome),
        ...textField("Problem", event.problem),
      ];
    case "lookup":
      return [
        ...field("Account", event.account, "account"),
        ...field("Outcome", event.outcome),
        ...field(
          "Chat key on",
          event.from === undefined
            ? undefined
            : event.from.length === 0
              ? "no chain"
              : event.from.join(" and "),
        ),
        ...textField("Reason", event.reason),
      ];
    case "session":
      return [
        ...field("Data", event.data),
        ...field("From", event.peer, "account"),
        ...field("Request id", event.requestId, "id"),
        ...field("Messages", event.kinds?.length),
        ...field("Already seen", event.duplicates),
        ...field(
          "Answer",
          event.code === undefined ? undefined : responseCode(event.code),
        ),
        ...field(
          "Answers our request",
          event.matched === undefined
            ? undefined
            : event.matched
              ? "yes"
              : "no",
        ),
      ];
    case "submit":
      return [
        ...field("Purpose", event.purpose),
        ...field("To", event.peer, "account"),
        ...field("Request id", event.requestId, "id"),
        ...field("Messages", event.messages),
        ...field("Result", event.result),
        ...textField("Store status", event.status),
        ...textField("Reason", event.reason),
      ];
    case "action-failed":
      return [
        ...field("Action", event.action),
        ...textField("Reason", event.reason),
      ];
  }
}

/** Lowercase text that a search of the event view matches against. */
export function eventSearchText(event: ChatEventBody): string {
  return [
    EVENT_TYPE_LABELS[event.type],
    eventDirection(event),
    eventOutcome(event),
    eventSummary(event),
    describeEvent(event),
    ...(event.type === "subscription" ? event.topicIds : []),
  ]
    .join(" ")
    .toLowerCase();
}

/**
 * An event as plain data for copying. It holds the event's metadata only,
 * and free text goes through {@link sanitizeReason} again.
 */
export function eventRecord(event: ChatEvent): Record<string, unknown> {
  const { seq, at, ...body } = withTextSanitized(event);
  return {
    seq,
    time: new Date(at).toISOString(),
    direction: eventDirection(event),
    outcome: eventOutcome(event),
    ...(event.type === "session" && event.kinds !== undefined
      ? { kindNames: event.kinds.map(kindLabel) }
      : {}),
    event: body,
  };
}
