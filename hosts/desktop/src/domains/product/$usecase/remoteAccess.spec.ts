import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('../permissions/broker', () => ({
  requestExternalUrlAccess: vi.fn(),
}));

import { requestExternalUrlAccess } from '../permissions/broker';

import { permissionsUseCase } from './permissions';
import { remoteAccessUseCase } from './remoteAccess';

const { resolveRemoteUrlAccess, setRemoteAccessPromptPolicy } = remoteAccessUseCase;

const call = () => resolveRemoteUrlAccess({ productId: 'p.dot', url: 'https://cdn.example.com/x.png', executionKind: 'App' });

beforeEach(() => {
  setRemoteAccessPromptPolicy(true);
  vi.mocked(requestExternalUrlAccess).mockReset();
  // The core's answer, spied in place: `restoreAllMocks` below puts the real method
  // back after every case, so the spy is re-established here rather than once.
  vi.spyOn(permissionsUseCase, 'getPermissionStatus').mockResolvedValue('ask');
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('resolveRemoteUrlAccess', () => {
  it('returns a stored "granted" without prompting', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('granted');
    await expect(call()).resolves.toBe('granted');
    expect(requestExternalUrlAccess).not.toHaveBeenCalled();
  });

  it('returns a stored "denied" without prompting', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('denied');
    await expect(call()).resolves.toBe('denied');
    expect(requestExternalUrlAccess).not.toHaveBeenCalled();
  });

  it('prompts when the core has no decision', async () => {
    vi.mocked(permissionsUseCase.getPermissionStatus).mockResolvedValue('ask');
    vi.mocked(requestExternalUrlAccess).mockResolvedValue('granted');
    await expect(call()).resolves.toBe('granted');
    expect(requestExternalUrlAccess).toHaveBeenCalledWith({
      productId: 'p.dot',
      url: 'https://cdn.example.com/x.png',
      executionKind: 'App',
    });
  });

  it('prompts when there is no matching stored pattern (policy on)', async () => {
    vi.mocked(requestExternalUrlAccess).mockResolvedValue('denied');
    await expect(call()).resolves.toBe('denied');
    expect(requestExternalUrlAccess).toHaveBeenCalledOnce();
  });

  it('denies unmatched silently without prompting when policy is off', async () => {
    setRemoteAccessPromptPolicy(false);
    await expect(call()).resolves.toBe('denied');
    expect(requestExternalUrlAccess).not.toHaveBeenCalled();
  });
});
