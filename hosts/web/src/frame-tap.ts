import { TRUAPI_CODEC_VERSION } from "@parity/truapi";
import type { InAppDebugger } from "@parity/truapi-debugger";

/** Which way a frame crossed, from the product's side. */
export type FrameDirection = "in" | "out";

/** Observes one product's frames as the page relays them. */
export type FrameTap = (direction: FrameDirection, frame: Uint8Array) => void;

/**
 * Feed a product's relayed frames to the in-page inspector.
 *
 * The identity carries the encoding core's own wire-schema hash. The inspector
 * decodes a frame only when that hash matches the wire table it was built
 * with, so a core and a page bundle that disagree show grouped, undecoded
 * traces instead of wrong method names.
 */
export function createFrameTap(
  inspector: InAppDebugger,
  productId: string,
  coreWireSchemaHash: string | undefined,
): FrameTap {
  const identity = { codec: TRUAPI_CODEC_VERSION, schema: coreWireSchemaHash };
  // The relay hands the same bytes on to the core, which may transfer them, so
  // the inspector keeps its own copy.
  return (direction, frame) =>
    inspector.handleFrame(productId, direction, frame.slice(), identity);
}
