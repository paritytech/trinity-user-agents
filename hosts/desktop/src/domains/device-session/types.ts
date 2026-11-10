import { type Observable } from 'rxjs';
import { type CodecType } from 'scale-ts';

import { type SyncSignalingEnvelopeCodec } from '@/shared/peer-channel';

export type SyncSignalingEnvelope = CodecType<typeof SyncSignalingEnvelopeCodec>;

export type DeviceSessionDeps = {
  ourDeviceEncPriv: Uint8Array;
  ourStatementAccountId: Uint8Array;
  peerDeviceEncPub: Uint8Array;
  peerStatementAccountId: Uint8Array;
  post: (topic: Uint8Array, data: Uint8Array, channel: Uint8Array) => Promise<void>;
  subscribe: (topic: Uint8Array) => Observable<{ topic: Uint8Array; data: Uint8Array }>;
};

export type DeviceSessionChannel = {
  send: (envelope: SyncSignalingEnvelope) => Promise<void>;
  /**
   * Posts several signals as ONE statement, in the given order.
   *
   * The peer's reconnect handling consumes a `Reconnected` together with the
   * envelopes that follow it *in the same request* (iOS `reconnection(in:)`
   * returns `(offerId, following)`), so a `Reconnected` and the Offer that
   * supersedes it must not be split across two posts — a peer that reads the
   * store between them resets against an empty `following` and then meets the
   * Offer with a connection it has just torn down.
   */
  sendAll: (envelopes: SyncSignalingEnvelope[]) => Promise<void>;
  messages$: Observable<SyncSignalingEnvelope>;
  close: () => void;
};
