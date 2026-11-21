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

/**
 * A `render` stream item that clears `cells` (`row * columns + column`) of the
 * `CoverGrid` keyed `key`, until the next tree replaces them with its own
 * `cleared` bitset.
 *
 * Send it in place of a whole tree while a finger scratches: the host cuts the
 * cells out of the cover straight away, with no tree to decode, map or diff.
 */
export function clearCells(key: string, cells: Iterable<number>): RendererNode {
  return { tag: "ClearCells", value: { key, cells: Array.from(cells) } };
}

/**
 * The `cleared` bitset of a `CoverGrid` holding `cells`, for a tree: one bit per cell, lowest bit first, spelled as
 * the generated client spells bytes (a `0x` hex string).
 */
export function coverBits(cells: Iterable<number>, columns: number, rows: number): `0x${string}` {
  const bits = new Uint8Array(Math.ceil((columns * rows) / 8));
  for (const cell of cells) {
    if (cell >= 0 && cell < columns * rows) bits[cell >> 3] |= 1 << (cell & 7);
  }
  return `0x${Array.from(bits, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}
