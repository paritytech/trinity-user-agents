import { freezeAndDelete } from './freeze.js';

export function installWebRtcPolicy(win: object): void {
  const descriptor = Object.getOwnPropertyDescriptor;
  const prototypeOf: (value: object) => object | null = Object.getPrototypeOf;
  // Calling consent authorizes the trusted host, never product-owned SDP/ICE.
  for (const name of [
    'RTCPeerConnection',
    'webkitRTCPeerConnection',
    'mozRTCPeerConnection',
  ]) {
    let owner: object | null = win;
    while (owner) {
      if (owner === win || descriptor(owner, name)) freezeAndDelete(owner, name);
      owner = prototypeOf(owner);
    }
  }
}
