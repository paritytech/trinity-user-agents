import { errAsync, okAsync } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { declinedUpdatesRepository } from '../product/declined-updates/repository';
import { liveExecutableResource } from '../product/manifest/resource';
import { type LiveExecutable } from '../product/manifest/types';
import { type PersistedProduct, productDb } from '../product/repository';

import { updatesUseCase } from './updates';

function makeRecord(overrides: Partial<PersistedProduct> = {}): PersistedProduct {
  return {
    baseName: 'app.dot',
    displayName: 'App',
    description: '',
    icon: { cid: 'abc', format: 'png' },
    executables: { app: { kind: 'app', identifier: 'app.dot', contenthash: '0xold', appVersion: [1, 0, 0] } },
    pinned: true,
    createdAt: 1000,
    updatedAt: 1000,
    ...overrides,
  };
}

function makeLive(overrides: Partial<LiveExecutable> = {}): LiveExecutable {
  return { contenthash: '0xnew', version: [1, 0, 1], ...overrides };
}

// The three "returns null" cases below never stub the live read: the builder mock
// already answers `null`, which is exactly the "no update" shape they need.
beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(productDb, 'getByBaseName');
  vi.spyOn(declinedUpdatesRepository, 'isDeclined');
  vi.mocked(declinedUpdatesRepository.isDeclined).mockResolvedValue(false);
});

describe('checkModalityUpdate', () => {
  it('returns null when the product is not pinned', async () => {
    const readLive = vi.fn(() => makeLive());
    liveExecutableResource.instead(readLive);
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord({ pinned: false })));
    expect(await updatesUseCase.checkModalityUpdate({ baseName: 'app.dot', kind: 'app' })).toBeNull();
    expect(readLive).not.toHaveBeenCalled();
  });

  it('returns null when the kind is absent on the frozen product', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord({ executables: {} })));
    expect(await updatesUseCase.checkModalityUpdate({ baseName: 'app.dot', kind: 'app' })).toBeNull();
  });

  it('returns null when the DB read errors', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(errAsync(new Error('db')));
    expect(await updatesUseCase.checkModalityUpdate({ baseName: 'app.dot', kind: 'app' })).toBeNull();
  });

  it('returns null when the live resolution matches the frozen contenthash (no drift)', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord()));
    liveExecutableResource.instead(() => makeLive({ contenthash: '0xold' }));
    expect(await updatesUseCase.checkModalityUpdate({ baseName: 'app.dot', kind: 'app' })).toBeNull();
  });

  it('returns null when drifted but the version is already declined', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord()));
    liveExecutableResource.instead(() => makeLive());
    vi.mocked(declinedUpdatesRepository.isDeclined).mockResolvedValue(true);
    expect(await updatesUseCase.checkModalityUpdate({ baseName: 'app.dot', kind: 'app' })).toBeNull();
  });

  it('returns the fresh {contenthash, version} when pinned, drifted, and undeclined', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord()));
    liveExecutableResource.instead(() => makeLive());
    const result = await updatesUseCase.checkModalityUpdate({ baseName: 'app.dot', kind: 'app' });
    expect(result).toEqual({ contenthash: '0xnew', version: [1, 0, 1] });
    expect(declinedUpdatesRepository.isDeclined).toHaveBeenCalledWith('app.dot', 'app', '0xnew');
  });

  it('normalizes a raw open identifier to the canonical base name before lookup', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord()));
    liveExecutableResource.instead(() => makeLive());
    const result = await updatesUseCase.checkModalityUpdate({ baseName: 'App', kind: 'app' });
    expect(result).toEqual({ contenthash: '0xnew', version: [1, 0, 1] });
    expect(productDb.getByBaseName).toHaveBeenCalledWith('app.dot');
    expect(declinedUpdatesRepository.isDeclined).toHaveBeenCalledWith('app.dot', 'app', '0xnew');
  });
});

describe('onProductModalityOpenedSideEffect', () => {
  it('fans out {productId, kind} to registered handlers', async () => {
    // No `.inject` on a bare identifier — that's `feature.inject`, which needs a
    // whole feature/gate/scope. `registerHandler`/`removeHandler` are the raw
    // effector events it wraps, and are the established direct-identifier test
    // pattern in this codebase (see createPipeline.test.ts).
    const handler = vi.fn();
    const registered = { available: () => true, body: handler };
    updatesUseCase.onProductModalityOpenedSideEffect.registerHandler(registered);

    await updatesUseCase.onProductModalityOpenedSideEffect.apply({ productId: 'app.dot', kind: 'widget' });
    expect(handler).toHaveBeenCalledWith({ productId: 'app.dot', kind: 'widget' });

    updatesUseCase.onProductModalityOpenedSideEffect.removeHandler(registered);
  });
});
