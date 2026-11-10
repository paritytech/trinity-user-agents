import { beforeEach, describe, expect, it, vi } from 'vitest';

import { contactRepository } from '@/domains/contact';
import { deviceSyncRepository } from '@/domains/device-sync';

import { sessionUseCase } from './session';

const clearAllP2PChatStorage = vi.fn(async () => undefined);
const clearAllProductChatStorage = vi.fn(async () => undefined);
// Repositories are plain objects, so their wipes are spied in place; nothing reaches Dexie.
const contactClearAll = vi.spyOn(contactRepository, 'clearAll').mockResolvedValue();
const deviceSyncClearAll = vi.spyOn(deviceSyncRepository, 'clearAll').mockResolvedValue();

// The two chat wipes are bare function exports, which nothing can spy on in place —
// the module has to be replaced until they take the storage they clear as a parameter.
vi.mock(import('@/domains/chat'), async importOriginal => {
  const actual = await importOriginal();

  return {
    ...actual,
    clearAllP2PChatStorage: () => clearAllP2PChatStorage(),

    clearAllProductChatStorage: () => clearAllProductChatStorage(),
  };
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('sessionUseCase.runV2Logout', () => {
  it('wipes every application-owned per-user store', async () => {
    await sessionUseCase.runV2Logout();

    expect(contactClearAll).toHaveBeenCalledTimes(1);
    expect(deviceSyncClearAll).toHaveBeenCalledTimes(1);
    expect(clearAllP2PChatStorage).toHaveBeenCalledTimes(1);
    expect(clearAllProductChatStorage).toHaveBeenCalledTimes(1);
  });

  // A blocked IndexedDB connection must not strand the user in an authenticated
  // shell: the wipe is best-effort, the logout always completes.
  it('completes even when a repository wipe fails', async () => {
    contactClearAll.mockRejectedValueOnce(new Error('IndexedDB blocked'));

    await expect(sessionUseCase.runV2Logout()).resolves.toBeUndefined();

    expect(clearAllProductChatStorage).toHaveBeenCalledTimes(1);
  });
});
