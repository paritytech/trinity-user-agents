import { GestureEvent, type RendererNode } from "./generated/types.js";

/** Encode the payload carried by a renderer gesture action. */
export function encodeGestureEvent(event: GestureEvent): Uint8Array {
  return GestureEvent.enc(event);
}

/** Decode the payload of a declared tap, start, move or end action. */
export function decodeGestureEvent(payload: Uint8Array): GestureEvent {
  return GestureEvent.dec(payload);
}

/**
 * A `render` stream item that moves the children of the `Gesture` keyed `key`
 * by `x` and `y` logical pixels, until the next tree replaces it. The item
 * carries hundredths of a logical pixel, so a slow drag still moves the
 * children on every frame.
 *
 * Send it in place of a whole tree while a pan moves: the host applies it
 * straight to that node's offset, with no tree to decode, map or diff.
 */
export function setOffset(key: string, x: number, y = 0): RendererNode {
  return { tag: "SetOffset", value: { key, x: Math.round(x * 100), y: Math.round(y * 100) } };
}
