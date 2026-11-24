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

/** A mask shape, as the generated client spells it. */
export type MaskShape = Extract<RendererNode, { tag: "ExtendMask" }>["value"]["shapes"][number];

const hundredths = (points: number) => Math.round(points * 100);

/** A filled circle for a mask, centred at `x`, `y` with `radius`, all in logical pixels. */
export function maskCircle(x: number, y: number, radius: number): MaskShape {
  return { tag: "Circle", value: { center: { x: hundredths(x), y: hundredths(y) }, radius: hundredths(radius) } };
}

/**
 * A line for a mask through `points` in order, `width` wide, with round caps and joins; one point is a dot. Logical
 * pixels in, hundredths on the wire.
 */
export function maskStroke(width: number, points: Iterable<{ x: number; y: number }>): MaskShape {
  return {
    tag: "Stroke",
    value: { width: hundredths(width), points: Array.from(points, (p) => ({ x: hundredths(p.x), y: hundredths(p.y) })) },
  };
}

/**
 * A `render` stream item that adds `shapes` to the `Mask` keyed `key`, until the next tree replaces them with its own.
 *
 * Send it in place of a whole tree while a finger draws: the host adds the shapes straight away, with no tree to
 * decode, map or diff. With no shapes it still answers a paced move, which is all a mask that follows the pan needs.
 */
export function extendMask(key: string, shapes: Iterable<MaskShape> = []): RendererNode {
  return { tag: "ExtendMask", value: { key, shapes: Array.from(shapes) } };
}
