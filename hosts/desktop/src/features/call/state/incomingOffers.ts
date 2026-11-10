/**
 * Bridges the incoming-offer SDP from `IncomingCallDetector` (which sees the
 * offer chat message) to `BridgeEndpointBinding` (which owns the MessagePort and
 * must ship `provideIncomingCall` to the call window). The detector stashes the
 * already-encoded MinimalSetup bytes keyed by the offer's chat `messageId`
 * (== the call window's `offerId`); the binding reads them when the port arrives
 * and clears the entry.
 */

const pendingIncomingOffers = new Map<string, Uint8Array>();

export function stashIncomingOffer(offerId: string, sdp: Uint8Array): void {
  pendingIncomingOffers.set(offerId, sdp);
}

export function takeIncomingOffer(offerId: string): Uint8Array | undefined {
  const sdp = pendingIncomingOffers.get(offerId);
  pendingIncomingOffers.delete(offerId);
  return sdp;
}
