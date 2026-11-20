import { GestureEvent } from "./generated/types.js";

/** Encode the payload carried by a renderer gesture action. */
export function encodeGestureEvent(event: GestureEvent): Uint8Array {
  return GestureEvent.enc(event);
}

/** Decode the payload of a declared tap, start, move or end action. */
export function decodeGestureEvent(payload: Uint8Array): GestureEvent {
  return GestureEvent.dec(payload);
}
