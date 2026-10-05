import { expect, it } from 'bun:test';

import { installWebRtcPolicy } from './webrtc.js';

it('denies standard and vendor RTC construction through inherited global slots', () => {
  class NativePeerConnection {}
  const prototype = {
    RTCPeerConnection: NativePeerConnection,
    webkitRTCPeerConnection: NativePeerConnection,
    mozRTCPeerConnection: NativePeerConnection,
  };
  const win: typeof prototype = Object.create(prototype);
  installWebRtcPolicy(win);
  for (const name of ['RTCPeerConnection', 'webkitRTCPeerConnection', 'mozRTCPeerConnection'] as const) {
    expect(() => new win[name]()).toThrow(TypeError);
    expect(() => new prototype[name]()).toThrow(TypeError);
  }
});

it('cannot restore a constructor by assignment or property redefinition', () => {
  class NativePeerConnection {}
  const win = { RTCPeerConnection: NativePeerConnection };
  installWebRtcPolicy(win);
  win.RTCPeerConnection = NativePeerConnection;
  Reflect.defineProperty(win, 'RTCPeerConnection', { value: NativePeerConnection });
  expect(() => new win.RTCPeerConnection()).toThrow(TypeError);
});
