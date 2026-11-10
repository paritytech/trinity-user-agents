import { toHex } from 'polkadot-api/utils';
import { describe, expect, it } from 'vitest';

import { PeerConnectionSignalCodec } from './peerConnectionSignal';

describe('PeerConnectionSignalCodec', () => {
  it('round-trips an offer', () => {
    const decoded = PeerConnectionSignalCodec.dec(PeerConnectionSignalCodec.enc({ tag: 'offer', value: 'v=0...' }));
    expect(decoded).toEqual({ tag: 'offer', value: 'v=0...' });
  });

  it('round-trips an answer', () => {
    const decoded = PeerConnectionSignalCodec.dec(PeerConnectionSignalCodec.enc({ tag: 'answer', value: 'v=0-ans' }));
    expect(decoded).toEqual({ tag: 'answer', value: 'v=0-ans' });
  });

  it('round-trips candidates (with and without sdpMid)', () => {
    const value = [
      { sdp: 'candidate:1 1 udp 2 1.2.3.4 5 typ host', sdpMLineIndex: 0, sdpMid: '0' },
      { sdp: 'candidate:2 1 udp 2 1.2.3.4 6 typ host', sdpMLineIndex: 1, sdpMid: undefined },
    ];
    const decoded = PeerConnectionSignalCodec.dec(PeerConnectionSignalCodec.enc({ tag: 'candidates', value }));
    expect(decoded).toEqual({ tag: 'candidates', value });
  });

  // Enum tag ordinals are the wire contract with iOS/Android.
  it('encodes variant tags as 0=offer, 1=answer, 2=candidates', () => {
    expect(toHex(PeerConnectionSignalCodec.enc({ tag: 'offer', value: '' })).slice(0, 4)).toBe('0x00');
    expect(toHex(PeerConnectionSignalCodec.enc({ tag: 'answer', value: '' })).slice(0, 4)).toBe('0x01');
    expect(toHex(PeerConnectionSignalCodec.enc({ tag: 'candidates', value: [] })).slice(0, 4)).toBe('0x02');
  });
});
