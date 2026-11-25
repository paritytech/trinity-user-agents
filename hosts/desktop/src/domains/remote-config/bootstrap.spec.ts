import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { REMOTE_CONFIG_REFRESH_ATTEMPTS } from './constants';

const firebase = vi.hoisted(() => ({
  fetchAndActivate: vi.fn(() => Promise.resolve(true)),
}));

vi.mock('firebase/app', () => ({ initializeApp: () => ({}) }));
vi.mock('firebase/remote-config', () => ({
  getRemoteConfig: () => ({ settings: { minimumFetchIntervalMillis: 0, fetchTimeoutMillis: 0 } }),
  fetchAndActivate: firebase.fetchAndActivate,
  setCustomSignals: () => Promise.resolve(),
}));

const { bootstrapRemoteConfig, refreshRemoteConfig, remoteConfigReady } = await import('./bootstrap');

// The singleton is module-level and initialises once, so every case shares it.
// Bootstrap fires its own fetch/activate — wait for it to settle so its call
// never lands in a case's call count.
beforeAll(async () => {
  bootstrapRemoteConfig({ apiKey: 'key', projectId: 'project', appId: 'app' });
  await remoteConfigReady;
});

beforeEach(() => {
  firebase.fetchAndActivate.mockReset();
  firebase.fetchAndActivate.mockResolvedValue(true);
});

describe('refreshRemoteConfig', () => {
  it('returns on the first successful attempt', async () => {
    await expect(refreshRemoteConfig()).resolves.toBe(true);

    expect(firebase.fetchAndActivate).toHaveBeenCalledTimes(1);
  });

  // The boot-time self-heal in `src/bootstrap.ts` hangs off this single call, so
  // one dropped request must not be terminal.
  it('retries a transient failure and succeeds', async () => {
    firebase.fetchAndActivate.mockRejectedValueOnce(new Error('network')).mockResolvedValueOnce(true);

    await expect(refreshRemoteConfig()).resolves.toBe(true);

    expect(firebase.fetchAndActivate).toHaveBeenCalledTimes(2);
  });

  it('gives up after the configured attempts and reports failure without throwing', async () => {
    firebase.fetchAndActivate.mockRejectedValue(new Error('offline'));

    await expect(refreshRemoteConfig()).resolves.toBe(false);

    expect(firebase.fetchAndActivate).toHaveBeenCalledTimes(REMOTE_CONFIG_REFRESH_ATTEMPTS);
  });
});
