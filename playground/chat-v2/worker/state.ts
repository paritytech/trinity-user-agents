/**
 * The state the product keeps in its local storage, so that a worker restart
 * continues where the last run stopped.
 */
import type { ChatReactionDetail, ChatRoomConfig, ChatRoomPreview } from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import { emptyClockState, type ClockState, type RoomId } from "./clock";
import { defaultPreview, hostStoreConfig, productSourceConfig, sourcePreview } from "./fixtures";
import { emptyStoreSnapshot, type StoreSnapshot } from "./store";

export type MenuMode = "none" | "custom" | "extend" | "hide-edit";
export type ReactionsMode = "on" | "na" | "off";

/** The current setting of each command, for one room. */
export interface RoomSettings {
  config: ChatRoomConfig;
  preview: ChatRoomPreview;
  menu: MenuMode;
  reactions: ReactionsMode;
  /** The guests `/participants` added after `p1` to `p4`. */
  guests: number;
  activity: boolean;
  echo: boolean;
  /** The product messages of the room, oldest first, for `last`. */
  productMessages: string[];
  /** The messages `/deeplink` posted. */
  deeplinkMessages: string[];
  /** The newest echo messages, so a `Seen` of one makes no new echo. */
  echoMessages: string[];
  /** The reactions on the messages of a host-store room. */
  reactionBook: Record<string, ChatReactionDetail[]>;
}

/** The one-shot failures of `/fail`. */
export interface FailFlags {
  reject: boolean;
  timeout: boolean;
  crash: boolean;
  unavailable: boolean;
}

export interface ProductState {
  version: 2;
  clock: ClockState;
  rooms: Record<RoomId, RoomSettings>;
  source: StoreSnapshot;
  /** `true` once `host-empty` exists. */
  hostEmptyCreated: boolean;
  /**
   * `true` after `/room create` and until `/room delete`. A state that an
   * older Worker saved has no value, which reads as `false`.
   */
  throwawayCreated?: boolean;
  /**
   * The key of `assets/sample.png`, once a submit succeeded. A failed submit
   * keeps no key, so the next `/post rich` submits again. A state that an
   * older Worker saved can hold the missing-preimage key, and the product
   * submits again for it too.
   */
  preimageKey?: HexString;
  slowMs: number;
  fail: FailFlags;
}

export function roomSettings(room: RoomId): RoomSettings {
  const source = room === "source";
  return {
    config: source ? productSourceConfig() : hostStoreConfig(),
    preview: source ? sourcePreview() : defaultPreview(),
    menu: "none",
    reactions: source ? "on" : "off",
    guests: 0,
    activity: false,
    echo: false,
    productMessages: [],
    deeplinkMessages: [],
    echoMessages: [],
    reactionBook: {},
  };
}

export function initialState(): ProductState {
  return {
    version: 2,
    clock: emptyClockState(),
    rooms: { host: roomSettings("host"), source: roomSettings("source"), "host-empty": roomSettings("host-empty") },
    source: emptyStoreSnapshot(),
    hostEmptyCreated: false,
    throwawayCreated: false,
    slowMs: 0,
    fail: { reject: false, timeout: false, crash: false, unavailable: false },
  };
}

const BIGINT_KEY = "$bigint";

/** JSON with `bigint` values kept as `{"$bigint": "<decimal>"}`. */
export function encodeState(state: ProductState): string {
  return JSON.stringify(state, (_key, value: unknown) =>
    typeof value === "bigint" ? { [BIGINT_KEY]: value.toString() } : value,
  );
}

export function decodeState(text: string): ProductState {
  const value: unknown = JSON.parse(text, (_key, field: unknown) => {
    if (field !== null && typeof field === "object" && !Array.isArray(field)) {
      const entries = Object.entries(field);
      if (entries.length === 1 && entries[0][0] === BIGINT_KEY && typeof entries[0][1] === "string") {
        return BigInt(entries[0][1]);
      }
    }
    return field;
  });
  if (value === null || typeof value !== "object" || !("version" in value) || value.version !== 2) {
    throw new Error("unknown state version");
  }
  return value as ProductState;
}

/** The product local storage, as the Worker uses it. */
export interface ProductStorage {
  read(key: string): Promise<string | undefined>;
  write(key: string, value: string): Promise<void>;
}

/** A storage in memory, for tests and for a host without local storage. */
export function memoryStorage(initial: Record<string, string> = {}): ProductStorage & { entries: Map<string, string> } {
  const entries = new Map(Object.entries(initial));
  return {
    entries,
    async read(key) {
      return entries.get(key);
    },
    async write(key, value) {
      entries.set(key, value);
    },
  };
}
