/**
 * Multiplexes multiple use-case streams onto one open data channel using the
 * `{ id, data }` envelope (see `dataChannelEnvelope.ts`). Transport-only: the
 * `data` bytes are opaque here — callers encode/decode their own signal type
 * (`PeerConnectionSignal`, `MediaStateSignal`). Parity with iOS
 * `MultiplexedDataChannel.swift`. The `DataChannelPort` seam keeps this unit
 * testable without a real `RTCDataChannel`.
 */

import { DataChannelMessageCodec } from './dataChannelEnvelope';

export const WEBRTC_RENEGOTIATION_USE_CASE_ID = 'webrtc_renegotiation_internal_use_case';

export type DataChannelPort = {
  send: (data: Uint8Array) => void;
  onMessage: (handler: (data: Uint8Array) => void) => () => void;
};

export type DataChannelSignaler = {
  send: (useCaseId: string, payload: Uint8Array) => void;
  subscribe: (useCaseId: string, handler: (payload: Uint8Array) => void) => () => void;
};

export function createDataChannelSignaler(port: DataChannelPort): DataChannelSignaler {
  const subscribers = new Map<string, ((payload: Uint8Array) => void)[]>();

  port.onMessage(raw => {
    const { id, data } = DataChannelMessageCodec.dec(raw);
    const handlers = subscribers.get(id);
    if (!handlers) return;
    for (const handler of handlers) handler(data);
  });

  function send(useCaseId: string, payload: Uint8Array) {
    port.send(DataChannelMessageCodec.enc({ id: useCaseId, data: payload }));
  }

  function subscribe(useCaseId: string, handler: (payload: Uint8Array) => void) {
    const handlers = subscribers.get(useCaseId) ?? [];
    handlers.push(handler);
    subscribers.set(useCaseId, handlers);
    return () => {
      const current = subscribers.get(useCaseId);
      if (!current) return;
      const i = current.indexOf(handler);
      if (i >= 0) current.splice(i, 1);
    };
  }

  return { send, subscribe };
}
