import { fromHex, toHex } from 'polkadot-api/utils';
import { describe, expect, it } from 'vitest';

import {
  DataChannelMessageCodec,
  MediaStateSignalCodec,
  PeerConnectionSignalCodec,
  WEBRTC_RENEGOTIATION_USE_CASE_ID,
} from './index';

// Golden SCALE bytes computed by an independent from-scratch encoder (NOT scale-ts),
// so this asserts the desktop codecs match the agreed wire spec. Both mobile apps
// target the same SCALE spec; a real iOS/Android capture must still be diffed against
// these goldens at the Phase-6 interop stage before claiming live interop.
const EXPECTED = {
  cameraEnabledTrue: '0x0001',
  microphoneEnabledFalse: '0x0100',
  offer: '0x0020763d302d74657374',
  candidates: '0x02041863616e642d6100000000010430',
  renegotiationEnvelope: '0x987765627274635f72656e65676f74696174696f6e5f696e7465726e616c5f7573655f63617365280020763d302d74657374',
};

describe('cross-platform wire parity', () => {
  it('MediaStateSignal matches mobile bytes', () => {
    expect(toHex(MediaStateSignalCodec.enc({ tag: 'cameraEnabled', value: true }))).toBe(EXPECTED.cameraEnabledTrue);
    expect(toHex(MediaStateSignalCodec.enc({ tag: 'microphoneEnabled', value: false }))).toBe(EXPECTED.microphoneEnabledFalse);
  });

  it('PeerConnectionSignal.offer matches mobile bytes', () => {
    expect(toHex(PeerConnectionSignalCodec.enc({ tag: 'offer', value: 'v=0-test' }))).toBe(EXPECTED.offer);
    // and decodes the mobile bytes back to the same value
    expect(PeerConnectionSignalCodec.dec(fromHex(EXPECTED.offer))).toEqual({ tag: 'offer', value: 'v=0-test' });
  });

  it('PeerConnectionSignal.candidates matches mobile bytes', () => {
    const value = [{ sdp: 'cand-a', sdpMLineIndex: 0, sdpMid: '0' }];
    expect(toHex(PeerConnectionSignalCodec.enc({ tag: 'candidates', value }))).toBe(EXPECTED.candidates);
    // and decodes the mobile bytes back to the same value
    expect(PeerConnectionSignalCodec.dec(fromHex(EXPECTED.candidates))).toEqual({ tag: 'candidates', value });
  });

  it('DataChannelMessage envelope matches mobile bytes', () => {
    const data = PeerConnectionSignalCodec.enc({ tag: 'offer', value: 'v=0-test' });
    const enc = DataChannelMessageCodec.enc({ id: WEBRTC_RENEGOTIATION_USE_CASE_ID, data });
    expect(toHex(enc)).toBe(EXPECTED.renegotiationEnvelope);
    // and decodes the mobile bytes back to the same value
    const decoded = DataChannelMessageCodec.dec(fromHex(EXPECTED.renegotiationEnvelope));
    expect(decoded.id).toBe(WEBRTC_RENEGOTIATION_USE_CASE_ID);
    expect(decoded.data).toEqual(data);
  });
});
