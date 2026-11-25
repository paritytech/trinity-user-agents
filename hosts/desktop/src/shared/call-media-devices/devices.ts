import { type CallDevices } from './types';

const EMPTY: CallDevices = { cameras: [], microphones: [], outputs: [] };

// Enumerate media devices and split them by kind. Phantom entries with an empty
// deviceId (no permission yet, or a placeholder) are dropped — they cannot be
// selected as a getUserMedia/setSinkId target.
export async function listCallDevices(): Promise<CallDevices> {
  const mediaDevices = navigator.mediaDevices;
  if (!mediaDevices || typeof mediaDevices.enumerateDevices !== 'function') return EMPTY;

  const all = await mediaDevices.enumerateDevices();
  const usable = all.filter(device => device.deviceId !== '');

  return {
    cameras: usable.filter(device => device.kind === 'videoinput'),
    microphones: usable.filter(device => device.kind === 'audioinput'),
    outputs: usable.filter(device => device.kind === 'audiooutput'),
  };
}
