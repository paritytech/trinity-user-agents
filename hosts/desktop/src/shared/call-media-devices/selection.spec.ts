import { beforeEach, describe, expect, it, vi } from 'vitest';

import { deviceIdForKind, loadDeviceSelection, saveDeviceSelection, withDeviceForKind } from './selection';

// Hermetic in-memory localStorage — avoids depending on the host environment's
// Web Storage (node's experimental global shadows happy-dom's and lacks a
// working `.clear`), so the persistence logic is what's under test, not the env.
const createMemoryStorage = (): Storage => {
  const store = new Map<string, string>();
  return {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: key => store.get(key) ?? null,
    setItem: (key, value) => void store.set(key, String(value)),
    removeItem: key => void store.delete(key),
    key: index => [...store.keys()][index] ?? null,
  };
};

beforeEach(() => vi.stubGlobal('localStorage', createMemoryStorage()));

describe('device selection persistence', () => {
  it('defaults to all-null when nothing is stored', () => {
    expect(loadDeviceSelection()).toEqual({ cameraId: null, microphoneId: null, outputId: null });
  });

  it('round-trips a saved selection', () => {
    saveDeviceSelection({ cameraId: 'cam-1', microphoneId: null, outputId: 'out-2' });
    expect(loadDeviceSelection()).toEqual({ cameraId: 'cam-1', microphoneId: null, outputId: 'out-2' });
  });

  it('falls back to all-null on corrupt JSON', () => {
    localStorage.setItem('call:device-selection', '{not json');
    expect(loadDeviceSelection()).toEqual({ cameraId: null, microphoneId: null, outputId: null });
  });

  it('reads/writes a kind via the helpers', () => {
    const base = { cameraId: null, microphoneId: null, outputId: null };
    const next = withDeviceForKind(base, 'microphone', 'mic-9');
    expect(next.microphoneId).toBe('mic-9');
    expect(deviceIdForKind(next, 'microphone')).toBe('mic-9');
    expect(deviceIdForKind(next, 'camera')).toBeNull();
  });
});
