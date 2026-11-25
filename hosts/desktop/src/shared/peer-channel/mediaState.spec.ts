import { toHex } from 'polkadot-api/utils';
import { describe, expect, it } from 'vitest';

import { MediaStateSignalCodec } from './mediaState';

describe('MediaStateSignalCodec', () => {
  it('round-trips cameraEnabled', () => {
    const decoded = MediaStateSignalCodec.dec(MediaStateSignalCodec.enc({ tag: 'cameraEnabled', value: true }));
    expect(decoded).toEqual({ tag: 'cameraEnabled', value: true });
  });

  it('round-trips microphoneEnabled', () => {
    const decoded = MediaStateSignalCodec.dec(MediaStateSignalCodec.enc({ tag: 'microphoneEnabled', value: false }));
    expect(decoded).toEqual({ tag: 'microphoneEnabled', value: false });
  });

  // Wire contract with Android MediaStateSignal (CameraEnabled=0, MicrophoneEnabled=1;
  // bool true=0x01, false=0x00). cameraEnabled(true) = 0x00 0x01; microphoneEnabled(false) = 0x01 0x00.
  it('byte layout matches Android wire', () => {
    expect(toHex(MediaStateSignalCodec.enc({ tag: 'cameraEnabled', value: true }))).toBe('0x0001');
    expect(toHex(MediaStateSignalCodec.enc({ tag: 'microphoneEnabled', value: false }))).toBe('0x0100');
  });
});
