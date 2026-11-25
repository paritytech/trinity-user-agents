/**
 * Schemas + validators for the cross-renderer MessagePort bridge. The MessagePort
 * is a trust boundary — a malformed message must be dropped (return null), never
 * thrown, so one bad frame cannot crash the peer renderer.
 *
 * The schemas are the source of truth: the `WindowToMainMessage` /
 * `MainToWindowMessage` types are derived via `v.InferOutput`. SDP/candidate
 * payloads are already-SCALE-encoded bytes — validated only as `Uint8Array`
 * here (the bridge is byte-opaque). `purpose` is pinned to the wire's accepted
 * values rather than importing the domain `CallPurpose`, so widening the wire
 * contract is a deliberate edit here.
 */

import * as v from 'valibot';

// `v.custom<Uint8Array>` rather than `v.instance(Uint8Array)`: the latter infers
// `Uint8Array<ArrayBufferLike>`, which mismatches the plain `Uint8Array` the rest
// of the code uses (TS 5.7+ made Uint8Array generic over its buffer).
const bytes = v.custom<Uint8Array>(value => value instanceof Uint8Array);
const purpose = v.picklist(['audio', 'video']);

export const WindowToMainMessageSchema = v.variant('kind', [
  v.object({ kind: v.literal('publishOffer'), offerId: v.string(), purpose, sdp: bytes }),
  v.object({ kind: v.literal('publishAnswer'), offerId: v.string(), sdp: bytes }),
  v.object({ kind: v.literal('publishCandidates'), offerId: v.string(), candidates: bytes }),
  v.object({ kind: v.literal('publishClosed'), offerId: v.string() }),
]);

export const MainToWindowMessageSchema = v.variant('kind', [
  v.object({
    kind: v.literal('provideIncomingCall'),
    offerId: v.string(),
    purpose,
    sdp: bytes,
    candidates: bytes,
    peerName: v.string(),
  }),
  v.object({ kind: v.literal('deliverAnswer'), offerId: v.string(), sdp: bytes }),
  v.object({ kind: v.literal('deliverCandidates'), offerId: v.string(), candidates: bytes }),
  v.object({ kind: v.literal('deliverClosed'), offerId: v.string() }),
  // Resolved on a sibling device — the window must close without emitting `closed`.
  v.object({ kind: v.literal('dismissAnsweredElsewhere'), offerId: v.string() }),
]);

export type WindowToMainMessage = v.InferOutput<typeof WindowToMainMessageSchema>;
export type MainToWindowMessage = v.InferOutput<typeof MainToWindowMessageSchema>;

export function parseWindowToMain(value: unknown): WindowToMainMessage | null {
  const result = v.safeParse(WindowToMainMessageSchema, value);
  return result.success ? result.output : null;
}

export function parseMainToWindow(value: unknown): MainToWindowMessage | null {
  const result = v.safeParse(MainToWindowMessageSchema, value);
  return result.success ? result.output : null;
}
