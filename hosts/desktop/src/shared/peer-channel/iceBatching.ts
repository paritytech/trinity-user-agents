/**
 * ICE-candidate batching policy, shared by every WebRTC consumer.
 *
 * Mobile parity — both clients implement this once, in the connection layer they
 * share across calls, device-sync and the video game:
 *   Android `PeerChannelConnection.startSendingLocalIceCandidates()` — `chunked(4, 500.milliseconds)`
 *   iOS     `DataConnectionContext` — `defaultLimitPerFlush = 4`, `defaultFlushDelay = 0.5`
 * Sending one candidate per signal (what this replaces) costs one statement-store
 * submit each, and each submit re-posts the sender's whole unacked batch.
 *
 * Empty batches are deliberately NOT filtered. The first emission is what bounds
 * how long an offer/answer waits for its initial candidates, so a peer that
 * gathers nothing must still receive an empty batch rather than hang. Consumers
 * skip empties at the trickle site.
 */

import { type OperatorFunction, bufferTime } from 'rxjs';

/** Candidates per batch before it is flushed early. Android/iOS both use 4. */
export const ICE_BATCH_SIZE = 4;

/** Longest a partial batch waits before being flushed. Android/iOS both use 500ms. */
export const ICE_BATCH_WINDOW_MS = 500;

/**
 * Batches a stream of gathered local candidates: emits when {@link ICE_BATCH_SIZE}
 * candidates have accumulated OR {@link ICE_BATCH_WINDOW_MS} elapses, whichever
 * comes first, then opens the next window.
 */
export function bufferIceCandidates(): OperatorFunction<RTCIceCandidate, RTCIceCandidate[]> {
  return bufferTime(ICE_BATCH_WINDOW_MS, null, ICE_BATCH_SIZE);
}
