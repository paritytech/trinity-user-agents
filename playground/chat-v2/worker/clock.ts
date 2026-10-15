/**
 * Message ids and timestamps from counters. The product never reads the
 * clock for a value it sends, so two hosts that get the same commands see the
 * same ids and times.
 */

/** 2026-01-01T00:00:00Z, in milliseconds since the Unix epoch. */
export const EPOCH_MS = 1_767_225_600_000n;
/** The time between two live messages. */
export const LIVE_STEP_MS = 1_000n;
/** The time between two messages of the generated history. */
export const HISTORY_STEP_MS = 60_000n;

/** A thread room of the product: `thread-<room>-<message id>`. */
export type ThreadRoomId = `thread-${string}`;

/** The rooms of the product. */
export type RoomId = "host" | "source" | "host-empty" | ThreadRoomId;

/** The thread room of message `messageId` of `room`. */
export function threadRoomId(room: RoomId, messageId: string): ThreadRoomId {
  return `thread-${room}-${messageId}`;
}

export function isThreadRoom(room: string): room is ThreadRoomId {
  return room.startsWith("thread-");
}

/** The counters, as the product keeps them in local storage. */
export interface ClockState {
  /** How many messages the product created. */
  created: number;
  /** The last product message counter of each room. */
  product: Partial<Record<RoomId, number>>;
  /** The last counter of the user messages of `source`. */
  user: number;
}

export function emptyClockState(): ClockState {
  return { created: 0, product: {}, user: 0 };
}

export function pad(value: number, width: number): string {
  return String(value).padStart(width, "0");
}

/** The id of message `index` (from 1) of the generated history. */
export function historyId(index: number): string {
  return `source-h-${pad(index, 5)}`;
}

/**
 * The timestamp of message `index` (from 1) of `count` history messages. The
 * newest one has `EPOCH_MS`, which is older than every live message.
 */
export function historyTimestamp(index: number, count: number): bigint {
  return EPOCH_MS - BigInt(count - index) * HISTORY_STEP_MS;
}

export class Clock {
  readonly #state: ClockState;

  constructor(state: ClockState) {
    this.#state = state;
  }

  /** The next id `<room>-p-<n>` of a product message. */
  nextProductId(room: RoomId): { id: string; counter: number } {
    const counter = (this.#state.product[room] ?? 0) + 1;
    this.#state.product[room] = counter;
    return { id: `${room}-p-${pad(counter, 4)}`, counter };
  }

  /** The next id `source-u-<n>` of a user message of `source`. */
  nextUserId(): string {
    this.#state.user += 1;
    return `source-u-${pad(this.#state.user, 4)}`;
  }

  /** The timestamp of the next message: `EPOCH_MS + n * 1000`. */
  nextTimestamp(): bigint {
    this.#state.created += 1;
    return this.now();
  }

  /**
   * The timestamp of the newest message the product created. An edit gets
   * this time, so an edit does not change the time of the next message.
   */
  now(): bigint {
    return EPOCH_MS + BigInt(this.#state.created) * LIVE_STEP_MS;
  }
}
