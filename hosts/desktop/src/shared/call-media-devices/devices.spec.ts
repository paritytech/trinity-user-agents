import { afterEach, describe, expect, it, vi } from 'vitest';

import { listCallDevices } from './devices';

const info = (kind: MediaDeviceKind, deviceId: string, label = ''): MediaDeviceInfo => ({
  kind,
  deviceId,
  label,
  groupId: '',
  toJSON: () => ({}),
});

afterEach(() => vi.unstubAllGlobals());

describe('listCallDevices', () => {
  it('splits devices by kind and drops empty-deviceId phantoms', async () => {
    const enumerateDevices = vi
      .fn()
      .mockResolvedValue([
        info('videoinput', 'cam-1', 'FaceTime'),
        info('videoinput', '', 'phantom'),
        info('audioinput', 'mic-1', 'Built-in Mic'),
        info('audiooutput', 'out-1', 'Speakers'),
        info('audiooutput', 'out-2', 'Headphones'),
      ]);
    vi.stubGlobal('navigator', { mediaDevices: { enumerateDevices } });

    const devices = await listCallDevices();

    expect(devices.cameras.map(d => d.deviceId)).toEqual(['cam-1']);
    expect(devices.microphones.map(d => d.deviceId)).toEqual(['mic-1']);
    expect(devices.outputs.map(d => d.deviceId)).toEqual(['out-1', 'out-2']);
  });

  it('returns empty lists when mediaDevices is unavailable', async () => {
    vi.stubGlobal('navigator', {});
    const devices = await listCallDevices();
    expect(devices).toEqual({ cameras: [], microphones: [], outputs: [] });
  });
});
