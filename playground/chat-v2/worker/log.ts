/**
 * The event log: one JSON line for each event, query and render request the
 * Worker receives, and one line for each query answer. A collector keeps the
 * lines whose `t` is `"chat-v2"`. The lines hold no wall-clock time.
 */
import { bytesToHex } from "./bytes";

/** The names of the six host-initiated queries. */
export type QueryName =
  | "messages_query"
  | "user_action"
  | "reactions_query"
  | "participants_query"
  | "message_actions_query"
  | "participant_actions_query";

export type LogVia =
  | "action_subscribe"
  | QueryName
  | "render"
  | "renderer_action"
  | "answer"
  /** The preimage key `/post rich` uses, once at each start. */
  | "preimage"
  /** One item of `room_events_subscribe`. */
  | "room_events"
  /** One media item of a message the user sent. */
  | "media";

export interface LogEntry {
  via: LogVia;
  /** For `answer`: the `seq` of the line of the query. */
  re?: number;
  room: string | null;
  kind: string;
  data: unknown;
}

export type LogSink = (line: string) => void;

/**
 * Change a decoded value into a JSON value: a `bigint` becomes a decimal
 * string, bytes become `0x` hex, and an `undefined` value becomes `null`, as
 * an absent `Option` does in the spec.
 */
export function toLogValue(value: unknown): unknown {
  if (typeof value === "bigint") return value.toString();
  if (value instanceof Uint8Array) return bytesToHex(value);
  if (Array.isArray(value)) return value.map((item) => (item === undefined ? null : toLogValue(item)));
  if (value !== null && typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const [key, field] of Object.entries(value)) {
      out[key] = field === undefined ? null : toLogValue(field);
    }
    return out;
  }
  return value;
}

export class EventLog {
  readonly boot: number;
  readonly #sink: LogSink;
  #seq = 0;

  constructor(boot: number, sink: LogSink = (line) => console.info(line)) {
    this.boot = boot;
    this.#sink = sink;
  }

  /** Write one line and give its `seq`. */
  record(entry: LogEntry): number {
    this.#seq += 1;
    const line: Record<string, unknown> = { t: "chat-v2", boot: this.boot, seq: this.#seq, via: entry.via };
    if (entry.re !== undefined) line.re = entry.re;
    line.room = entry.room;
    line.kind = entry.kind;
    line.data = entry.data === undefined ? null : toLogValue(entry.data);
    this.#sink(JSON.stringify(line));
    return this.#seq;
  }
}
