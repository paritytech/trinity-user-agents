import type { RoomId } from "./clock";

/** The DotNS name the build used when it set none. */
export const DEFAULT_PRODUCT = "truapi-chat-v2.paseo";

/** The DotNS name of the product, from the build. */
export const PRODUCT: string =
  typeof __CHAT_V2_PRODUCT__ === "string" ? __CHAT_V2_PRODUCT__ : DEFAULT_PRODUCT;

/** The link to a room, as the RFC addresses it. */
export function roomLink(roomId: RoomId, product: string = PRODUCT): string {
  return `polkadot://${product}/-/chat/${roomId}`;
}

/** The link that opens the App with its "Opened from a deeplink" banner. */
export function appDeeplink(product: string = PRODUCT): string {
  return `polkadot://${product}/?from=deeplink`;
}
