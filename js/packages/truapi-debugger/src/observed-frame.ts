/**
 * The frame model the debugger works in.
 *
 * A host tap streams raw wire frames as `{ channelId, dir, frame: bytes }`
 * envelopes; {@link createDebugIngest} decodes each one into an
 * {@link ObservedFrame} - correlation id, wire discriminant, byte length, and
 * (dev-only) the raw bytes - which the trace and host engines consume. The core
 * never decodes; decoding happens here, in the debugger.
 *
 * @module
 */

/**
 * Direction of an observed wire frame relative to the product: `out` left the
 * product, `in` arrived at it.
 */
export type FrameDirection = "out" | "in";

/**
 * Role of an observed frame within the request/subscription lifecycle, derived
 * from its wire discriminant against the method's frame ids.
 */
/**
 * Roles that OPEN an op. Lives here, in the leaf module, because both the
 * retention engine and the view layer need it: the engine protects the opener
 * from eviction and the storm detector keys on its frame id, and both of those
 * were previously written as `frames[0]` on the assumption the opener is the
 * first frame observed. It is not - both mounts start mid-session, so the first
 * frame seen for an id is often a closer for a request that predates the tap.
 */
export const OPENING_ROLES: ReadonlySet<FrameRole> = new Set<FrameRole>([
  "request",
  "start",
]);

/**
 * Index of the frame that opened this op, or `-1` when no opener was observed.
 * Takes anything carrying a {@link FrameRole}, so both the raw
 * {@link ObservedFrame} sequence and the view layer's projections can use it
 * (the op began before the tap attached). Callers that need a frame to anchor on
 * regardless should fall back to `0`, never assume `0` IS the opener.
 */
export function openerIndexOf(
  frames: readonly { readonly role: FrameRole }[],
): number {
  return frames.findIndex((f) => OPENING_ROLES.has(f.role));
}

export type FrameRole =
  | "request"
  | "response"
  | "cancel"
  | "start"
  | "stop"
  | "receive"
  | "interrupt"
  | "handshake"
  | "malformed"
  | "unknown";

/**
 * A single decoded wire frame. Carries the correlation `requestId`, the wire
 * discriminant, a best-effort lifecycle `role`, and the encoded byte length.
 * The raw `bytes` are present only when byte exposure is enabled - a dev-only
 * opt-in, since the raw wire can carry key material.
 */
export interface ObservedFrame {
  /**
   * Product channel the frame crossed, e.g. `"myapp.dot"`. Carried from the
   * host tap envelope. Because `requestId` is minted per transport (each host
   * mints `p:1`, `p:2`, …), it is unique only *within* a channel; grouping and
   * lookups key on `(channelId, requestId)` so two hosts' ops never merge.
   */
  channelId: string;
  /** Whether the frame was sent by the product (`out`) or received by it (`in`). */
  direction: FrameDirection;
  /** Correlation id shared by every frame of one request/subscription, within a channel. */
  requestId: string;
  /** Wire-table numeric discriminant of the frame's payload. */
  frameId: number;
  /**
   * The wire's own leg marker (`Payload.messageType`): `Request`/`Start` = 0,
   * `Response`/`Receive` = 1, `Interrupt` = 2, `Stop` = 3, `Cancel` = 4. `-1` for a frame
   * that failed to decode at all (the `"malformed"` sentinel).
   */
  messageType: number;
  /** Best-effort lifecycle role inferred from the frame id and `messageType`. */
  role: FrameRole;
  /** Encoded SCALE payload length in bytes. */
  byteLength: number;
  /** Epoch ms at which the frame was observed. */
  timestamp: number;
  /** The raw SCALE payload bytes, present only when byte exposure is enabled. */
  bytes?: Uint8Array;
  /**
   * Whether the producer of THIS frame affirmatively vouched for the wire contract
   * the debugger decodes with (matching envelope version, codec version and wire
   * schema hash).
   *
   * Identity has to travel with the frame, not with its channel. A per-channel
   * verdict is a latch: one attested frame flipped the channel to trusted and every
   * unattested frame already retained under that `channelId` became decodable
   * retroactively. `channelId` is the productId, so a stale host and a fresh host
   * serving the same product share it, and an embedding host that learns its core's
   * schema hash asynchronously tees unattested frames before it knows it.
   *
   * Absent means "not vouched for": group it, list it, never decode it.
   */
  identityConfirmed?: boolean;
}

/**
 * Emit-only consumer of observed frames. The trace engine's
 * {@link WireDebugger.observe} is one; a host relay is another.
 */
export type TransportObserver = (frame: ObservedFrame) => void;
