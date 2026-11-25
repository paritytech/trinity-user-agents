import { describe, expect, it, vi } from 'vitest';

import { type DataChannelPort, createDataChannelSignaler } from './dataChannelSignaler';

function createFakePort(): DataChannelPort & { emit: (data: Uint8Array) => void; sent: Uint8Array[] } {
  const handlers: ((data: Uint8Array) => void)[] = [];
  const sent: Uint8Array[] = [];
  return {
    sent,
    send: data => {
      sent.push(data);
    },
    onMessage: handler => {
      handlers.push(handler);
      return () => {
        const i = handlers.indexOf(handler);
        if (i >= 0) handlers.splice(i, 1);
      };
    },
    emit: data => {
      for (const h of handlers) h(data);
    },
  };
}

describe('createDataChannelSignaler', () => {
  it('routes an inbound payload only to the matching use-case subscriber', () => {
    const port = createFakePort();
    const signaler = createDataChannelSignaler(port);
    const renegotiation = vi.fn();
    const mediaState = vi.fn();
    signaler.subscribe('webrtc_renegotiation_internal_use_case', renegotiation);
    signaler.subscribe('webrtc_media_state_use_case', mediaState);

    // Loop a send back through the port to simulate the remote peer.
    signaler.send('webrtc_renegotiation_internal_use_case', new Uint8Array([9, 8, 7]));
    port.emit(port.sent[0]!);

    expect(renegotiation).toHaveBeenCalledTimes(1);
    expect(Array.from(renegotiation.mock.calls[0]![0])).toEqual([9, 8, 7]);
    expect(mediaState).not.toHaveBeenCalled();
  });

  it('stops delivering after unsubscribe', () => {
    const port = createFakePort();
    const signaler = createDataChannelSignaler(port);
    const handler = vi.fn();
    const unsubscribe = signaler.subscribe('id-a', handler);

    signaler.send('id-a', new Uint8Array([1]));
    port.emit(port.sent[0]!);
    unsubscribe();
    signaler.send('id-a', new Uint8Array([2]));
    port.emit(port.sent[1]!);

    expect(handler).toHaveBeenCalledTimes(1);
  });

  it('ignores payloads for a use-case id with no subscriber', () => {
    const port = createFakePort();
    const signaler = createDataChannelSignaler(port);
    signaler.send('unknown', new Uint8Array([1]));
    expect(() => port.emit(port.sent[0]!)).not.toThrow();
  });
});
