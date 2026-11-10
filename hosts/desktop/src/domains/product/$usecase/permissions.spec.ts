import 'fake-indexeddb/auto';

import { type PermissionAuthorizationRequest } from '@parity/truapi-host';
import { firstValueFrom } from 'rxjs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { appDatabase } from '@/shared/database';
import { permissionsService } from '../permissions/service';

import { permissionsUseCase } from './permissions';

// Annotated rather than inferred: a bare object literal widens `tag` to `string`,
// which is not assignable to the discriminated union, and the spec is typechecked.
const CAMERA: PermissionAuthorizationRequest = { tag: 'Device', value: 'Camera' };

const adapter = {
  getStatus: vi.fn(),
  getStatuses: vi.fn(),
  setStatus: vi.fn(),
};

describe('permissionsUseCase', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    permissionsUseCase._resetPermissionLookups();
    permissionsUseCase.setPermissionsAdapter(adapter);
  });

  afterEach(async () => {
    await appDatabase.table('coreStorage').clear();
  });

  it('reads the whole catalogue in one call', async () => {
    adapter.getStatuses.mockResolvedValue(permissionsService.catalogueRequests().map(() => 'Authorized'));

    const entries = await firstValueFrom(permissionsUseCase.watchProductPermissions({ productId: 'one.dot' }));

    expect(adapter.getStatuses).toHaveBeenCalledTimes(1);
    expect(entries.length).toBeGreaterThan(0);
    expect(entries.every(entry => entry.status === 'granted')).toBe(true);
  });

  it('fails closed when the core read rejects', async () => {
    adapter.getStatuses.mockRejectedValue(new Error('worker gone'));

    const entries = await firstValueFrom(permissionsUseCase.watchProductPermissions({ productId: 'one.dot' }));

    expect(entries.every(entry => entry.status === 'ask')).toBe(true);
  });

  it('fails closed when no adapter has been injected', async () => {
    permissionsUseCase.setPermissionsAdapter(null);

    await expect(permissionsUseCase.getPermissionStatus({ productId: 'one.dot', request: CAMERA })).resolves.toBe('ask');
  });

  it('fails closed rather than hanging when the adapter never answers', async () => {
    vi.useFakeTimers();
    adapter.getStatus.mockReturnValue(new Promise(() => {}));

    const pending = permissionsUseCase.getPermissionStatus({ productId: 'hangs.dot', request: CAMERA });
    await vi.advanceTimersByTimeAsync(5_000);

    await expect(pending).resolves.toBe('ask');
    vi.useRealTimers();
  });

  it('de-duplicates concurrent identical lookups without retaining the result', async () => {
    adapter.getStatus.mockResolvedValue('Authorized');

    await Promise.all([
      permissionsUseCase.getPermissionStatus({ productId: 'dedupe.dot', request: CAMERA }),
      permissionsUseCase.getPermissionStatus({ productId: 'dedupe.dot', request: CAMERA }),
    ]);
    expect(adapter.getStatus).toHaveBeenCalledTimes(1);

    await permissionsUseCase.getPermissionStatus({ productId: 'dedupe.dot', request: CAMERA });
    expect(adapter.getStatus).toHaveBeenCalledTimes(2);
  });
});
