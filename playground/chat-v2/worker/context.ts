import type { TrUApiClient } from "@parity/truapi";
import type { HexString } from "@parity/truapi/scale";
import type { Clock, RoomId } from "./clock";
import type { Feed } from "./feed";
import type { Assets } from "./fixtures";
import type { ProductState, RoomSettings } from "./state";
import type { SourceStore } from "./store";

/** The parts of the client the Worker uses. Tests give a fake of it. */
export type SampleClient = Pick<TrUApiClient, "chat" | "renderer" | "localStorage" | "preimage">;

export type ChatSurface = SampleClient["chat"];

/** What a command needs to run. */
export interface ProductContext {
  chat: ChatSurface;
  assets: Assets;
  /** The DotNS name of the product, for the links. */
  product: string;
  state: ProductState;
  clock: Clock;
  store: SourceStore;
  feed: Feed;
  settings(room: RoomId): RoomSettings;
  /** The key `/post rich` uses for its preimage image. */
  preimageKey(): Promise<HexString>;
}
