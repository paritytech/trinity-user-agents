/**
 * Ingest: turn the host tap's wire envelopes into {@link ObservedFrame}s.
 *
 * The Rust host tap (`truapi`'s `DebugSink`) emits one envelope per
 * frame - `{ channelId, dir, frame: bytes }`, raw SCALE, opaque to the core.
 * The debugger decodes here: {@link decodeWireMessage} recovers the correlation
 * `requestId` and the wire discriminant, which is everything the trace engine
 * needs to group an op. This is the layer PG's design puts "in the debugger, not
 * the core".
 *
 * @module
 */

import { decodeWireMessage } from "@parity/truapi";
import type { ObservedFrame, TransportObserver } from "./observed-frame.js";
import { frameIdOf, resolveRole, type WireMethodInfo } from "./wire-debugger.js";

/**
 * Version of the host→debugger wire envelope (`{ channelId, dir, frame }`).
 * Bumped when the envelope shape changes. Producers (the Rust `WsDebugSink`, the
 * web host's debugger link) stamp it alongside a codec identity so the debugger
 * can refuse to decode a frame against a wire contract that isn't its own -
 * frame ids are `u8` discriminants that get reassigned as the API evolves, so an
 * unversioned envelope from an older host would resolve to the wrong method and
 * the wrong value.
 */
export const WIRE_ENVELOPE_VERSION = 1;

/**
 * Default cap on `channelId` / `requestId` length, above which the id is
 * replaced by a digest ({@link normalizeId}). Shared so the debugger server's
 * channel registry normalizes to the same bound as ingest and the two keys stay
 * equal (the UI filters by the normalized key).
 */
export const DEFAULT_MAX_ID_CHARS = 256;

/**
 * FNV-1a over `text`'s UTF-16 code units, in 32 bits. Not cryptographic: this
 * only has to keep two *distinct* ids distinct, which a shared prefix does not.
 */
function fnv1a32(text: string, seed: number): number {
  let hash = seed >>> 0;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash >>> 0;
}

/** Two independently-seeded FNV-1a passes, as 16 hex chars. */
function digest(text: string): string {
  const lo = fnv1a32(text, 0x811c9dc5).toString(16).padStart(8, "0");
  const hi = fnv1a32(text, 0x9dc5811c).toString(16).padStart(8, "0");
  return `${lo}${hi}`;
}

/**
 * Bound an id's retained length: returned unchanged when it is within
 * `maxChars`, otherwise replaced by `…<digest>:<length>`.
 *
 * A digest, not a truncation, for two reasons.
 *
 *  - Retention. `String.prototype.slice` yields a view that keeps its *parent*
 *   alive in both JSC and V8, so truncating a 250k-char id retains the whole
 *   250k chars while accounting for 256 - and `retainBytes: false` is no
 *   mitigation, because the ids are retained on every {@link ObservedFrame}
 *   regardless. The digest is computed arithmetically, so nothing references the
 *   input.
 *  - Identity. Two distinct ids sharing a `maxChars` prefix truncate to the same
 *   key and merge into one trace, fabricating a `roundTripMs` between two
 *   unrelated ops and clearing a genuinely `orphaned` badge. Distinct ids digest
 *   to distinct keys.
 *
 * Rejecting the frame instead would take the op dark, which is the opposite of
 * the ingest's own rule for input it cannot use (an undecodable frame becomes a
 * `"malformed"` sentinel, never a drop), and it would discard legitimate frames
 * from any host whose ids are merely long. The digest keeps the op observable
 * and correlatable while bounding what is retained.
 *
 * The length suffix is diagnostic: it says how long the id the host sent
 * actually was, which is the fact an operator needs to see.
 */
export function normalizeId(
  id: string,
  maxChars: number = DEFAULT_MAX_ID_CHARS,
): string {
  if (id.length <= maxChars) return id;
  return `…${digest(id)}:${String(id.length)}`;
}

/**
 * One wire frame as it crosses the host tap, matching the Rust
 * `DebugEvent::Frame { channel_id, dir, bytes }`. `frame` is the untouched
 * `ProtocolMessage` bytes; the debugger owns all decoding.
 */
export interface DebugFrameEnvelope {
  /** Product channel the frame belongs to, e.g. `"myapp.dot"`. */
  channelId: string;
  /**
   * Product-vantage: `out` left the product, `in` arrived at it. The Rust host
   * tap names directions host-vantage internally and flips to this convention
   * on the wire (`FrameDirection::wire_str`), so both ends agree here.
   */
  dir: "in" | "out";
  /** Raw SCALE `ProtocolMessage` bytes. */
  frame: Uint8Array;
  /**
   * Whether this envelope's producer affirmatively vouched for the debugger's wire
   * contract. Set by the mount that parsed the identity fields; carried onto every
   * frame so decode is gated per frame rather than per channel.
   */
  identityConfirmed?: boolean;
  /**
   * Epoch ms at which the *producer* saw the frame cross the tap, stamped by the
   * host link at emit time.
   *
   * The debugger's own clock cannot stand in for this. A host tap buffers a
   * backlog while the debugger is absent and flushes it in one loop on connect,
   * so every frame of a session that ran before the debugger started would be
   * stamped with the same flush instant: durations collapse to 0ms and ops
   * minutes apart land inside the retry-storm window. The producer is the only
   * party that knows when a frame actually crossed.
   *
   * Optional because a host may not stamp it (a pre-identity or foreign tap);
   * such frames fall back to the ingest clock and are marked as such - see
   * {@link ObservedFrame.timestampFromProducer}.
   */
  observedAt?: number;
  /**
   * The producer replayed this frame from its backlog rather than streaming it
   * live, so its arrival order and arrival time are the link's, not the
   * session's. Piggybacked on the envelope the same way `dropped` is.
   */
  buffered?: boolean;
}

// Both fields below are produced *only* here, and `ObservedFrame` is the contract
// every consumer reads, so they are declared onto it rather than pushing every
// consumer through an ingest-specific subtype. Fold them into
// `observed-frame.ts` proper when that file is next touched.
declare module "./observed-frame.js" {
  interface ObservedFrame {
    /**
     * The producer replayed this frame from its backlog (the debugger was absent
     * or slow) instead of streaming it live. Present only when true.
     *
     * Provenance, not a verdict on `timestamp`: a buffered frame that also
     * carries {@link ObservedFrame.timestampFromProducer} has a real observation
     * time and its timings are sound. A buffered frame *without* it has only the
     * flush instant, and every duration derived from it - `roundTripMs`, the
     * retry-storm window - is meaningless.
     */
    buffered?: true;
    /**
     * `timestamp` is the producer's own observation time rather than the moment
     * ingest decoded the frame. Present only when true.
     */
    timestampFromProducer?: true;
  }
}

/**
 * An `observedAt` fit to be used as a timestamp, or `undefined`.
 *
 * Anything able to reach the tap can put anything in this field, and it feeds
 * trace ordering and every duration, so a non-finite or non-positive value falls
 * back to the ingest clock rather than poisoning the trace list.
 */
function producerTimestamp(observedAt: number | undefined): number | undefined {
  if (typeof observedAt !== "number") return undefined;
  // `isSafeInteger`, not merely finite: `1e308` is a finite positive number and
  // was accepted as an epoch-ms timestamp, which made `durationMs` overflow to
  // `Infinity` and serialize as JSON `null` on /stats - a hole in the payload a
  // client parses back. An epoch-ms value is a safe integer by construction.
  if (!Number.isSafeInteger(observedAt) || observedAt <= 0) return undefined;
  return observedAt;
}

/** Options for {@link createDebugIngest}. */
export interface DebugIngestOptions {
  /**
   * Retain each frame's raw SCALE bytes on the {@link ObservedFrame}. Off by
   * default: byte length is always recorded, but the bytes themselves are the
   * dev-only opt-in that level-2 decode needs. `/traces` never serializes them
   * either way; retaining them only makes the drill-down decoder able to run.
   */
  retainBytes?: boolean;
  /**
   * Reverse map from wire `frameId` to method info (build one with
   * {@link createMethodNameMap}). When set, each frame's lifecycle `role` is
   * resolved here from the frame id's wire-table `kind`, so *every* consumer -
   * the default console sink, the `forward` hook, and the trace engine - sees the
   * real role. Without it, `role` is left `"unknown"` and only the view adapter
   * recovers it.
   */
  methodNames?: ReadonlyMap<number, WireMethodInfo>;
  /**
   * Length above which a `channelId` / `requestId` is replaced by a digest
   * ({@link normalizeId}). Anything able to reach the host tap could otherwise
   * send 200k-char ids, one copy per frame; real ids are short (`myapp.dot`,
   * `p:1`). Default 256.
   */
  maxIdChars?: number;
}

/**
 * Ingest that decodes each {@link DebugFrameEnvelope} and forwards the resulting
 * {@link ObservedFrame} to `sink` (typically a {@link WireDebugger}'s `observe`).
 *
 * `role` is a pure function of the frame's wire discriminant and its own
 * `messageType` byte (see {@link resolveRole}): the `(trait, method)` pair
 * addresses a method regardless of which leg a frame carries, so `role`
 * combines the method's static `kind` from `methodNames` with the frame's own
 * `messageType`, rather than being reconstructed from correlation state.
 * Resolving it at ingest is what makes it true for *every* consumer -
 * the default `console.debug` sink, the `forward` hook, and the trace engine -
 * instead of only for the view adapter, which resolves one layer further down
 * (`wireTraceToView`) and would leave the other two reading `"unknown"`.
 *
 * `role` falls back to `"unknown"` in exactly two cases: no `methodNames` map was
 * given, or the id is off-table (a frame from a newer host). An undecodable frame
 * is surfaced as a `"malformed"` sentinel rather than dropped, so the trace
 * records the failure instead of going dark.
 *
 * Raw payload bytes are attached only when `retainBytes` is set - the dev-only
 * byte-exposure opt-in that the level-2 decoder consumes; otherwise a frame
 * carries its byte length and no payload.
 *
 * `timestamp` is the producer's `observedAt` whenever the tap stamped a usable
 * one, and the ingest clock otherwise. Which of the two it is, and whether the
 * frame was replayed from the tap's backlog, are recorded on the frame
 * ({@link ObservedFrame.timestampFromProducer}, {@link ObservedFrame.buffered}),
 * because a flushed backlog arrives in a single loop: read as observation times,
 * those instants collapse every duration to 0ms and pull ops minutes apart into
 * one retry-storm window.
 */
export function createDebugIngest(
  sink: TransportObserver,
  options: DebugIngestOptions = {},
): (envelope: DebugFrameEnvelope) => void {
  const retainBytes = options.retainBytes ?? false;
  const methodNames = options.methodNames;
  const maxIdChars = options.maxIdChars ?? DEFAULT_MAX_ID_CHARS;
  return (envelope) => {
    const channelId = normalizeId(envelope.channelId, maxIdChars);
    // Prefer the producer's observation time; the ingest clock is a fallback, and
    // one that is wrong by the whole duration of the session for a flushed
    // backlog. `provenance` is what lets a consumer tell the two apart instead of
    // reading every timestamp as an observation time.
    const producerAt = producerTimestamp(envelope.observedAt);
    const timestamp = producerAt ?? Date.now();
    const provenance = {
      ...(envelope.buffered === true ? { buffered: true as const } : {}),
      ...(producerAt !== undefined ? { timestampFromProducer: true as const } : {}),
      // Per-frame, deliberately: see ObservedFrame.identityConfirmed.
      ...(envelope.identityConfirmed === true
        ? { identityConfirmed: true as const }
        : {}),
    };
    const decoded = decodeWireMessage(envelope.frame);
    if (decoded.isErr()) {
      sink({
        channelId,
        direction: envelope.dir,
        requestId: "malformed",
        frameId: -1,
        messageType: -1,
        role: "malformed",
        byteLength: envelope.frame.length,
        timestamp,
        ...provenance,
      });
      return;
    }
    const { requestId, payload } = decoded.value;
    const frameId = frameIdOf(payload.traitId, payload.methodId);
    const frame: ObservedFrame = {
      channelId,
      direction: envelope.dir,
      requestId: normalizeId(requestId, maxIdChars),
      frameId,
      messageType: payload.messageType,
      // Resolve the lifecycle role from the method's wire-table kind plus the
      // frame's own `messageType` byte (see resolveRole). "unknown" when no
      // map was given, or the id is off-table (a frame from a newer host).
      role: resolveRole(payload.messageType, methodNames?.get(frameId)?.kind),
      byteLength: payload.value.length,
      timestamp,
      ...provenance,
      ...(retainBytes ? { bytes: payload.value } : {}),
    };
    sink(frame);
  };
}
