import { err, ok } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { type WidgetExecutable } from '../product/manifest/types';
import { productDb } from '../product/repository';
import { chainResolveResource } from '../product/resource';

import { commitmentUseCase } from './commitment';
import { offlineCacheUseCase } from './offlineCache';
import { resolveProductUseCase } from './resolve';

// Every collaborator is a plain object, spied in place with no implementation: the
// repository so nothing reaches Dexie, the sibling use cases so a chain read or an
// archive prefetch never runs. Each case states the value it needs.
vi.spyOn(productDb, 'getByBaseName');
vi.spyOn(productDb, 'update');
vi.spyOn(productDb, 'updateExecutable');
vi.spyOn(productDb, 'upsert');
vi.spyOn(offlineCacheUseCase, 'prefetchArchives').mockResolvedValue();
vi.spyOn(offlineCacheUseCase, 'evictArchives').mockResolvedValue();
const fetchProductFromChain = vi.spyOn(resolveProductUseCase, 'fetchProductFromChain');
const resolveFreshExecutable = vi.spyOn(resolveProductUseCase, 'resolveFreshExecutable');

function makeProduct(overrides: Partial<{ displayName: string }> = {}) {
  return {
    baseName: 'app.dot',
    displayName: 'App',
    description: '',
    icon: { cid: 'abc', format: 'png' as const },
    executables: {},
    ...overrides,
  };
}

function makeRecord(overrides: Partial<{ pinned: boolean }> = {}) {
  return {
    ...makeProduct(),
    pinned: false,
    createdAt: 1000,
    updatedAt: 1000,
    ...overrides,
  };
}

// The chain-resolve cache is a resource, so the eviction is observed on the resource
// itself rather than through a stand-in for the module that owns it.
const invalidateChainResolve = vi.spyOn(chainResolveResource, 'invalidate');

beforeEach(() => {
  vi.clearAllMocks();
});

describe('commitProductByIdentifier', () => {
  it('is idempotent — returns existing row without chain call when row already exists', async () => {
    const row = makeRecord();
    vi.mocked(productDb.getByBaseName).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(row)) as any,
    );

    const result = await commitmentUseCase.commitProductByIdentifier('app.dot');

    expect(result).toBe(row);
    expect(fetchProductFromChain).not.toHaveBeenCalled();
    expect(productDb.upsert).not.toHaveBeenCalled();
  });

  it('resolves chain + upserts with pinned:false + drops the chain cache when row absent', async () => {
    const fresh = makeProduct();
    const saved = makeRecord({ pinned: false });

    vi.mocked(productDb.getByBaseName).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(null)) as any,
    );
    vi.mocked(fetchProductFromChain).mockResolvedValue(fresh);
    vi.mocked(productDb.upsert).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(saved)) as any,
    );

    const result = await commitmentUseCase.commitProductByIdentifier('app.dot');

    expect(fetchProductFromChain).toHaveBeenCalledWith('app.dot');
    expect(productDb.upsert).toHaveBeenCalledWith(fresh, { pinned: false });
    expect(invalidateChainResolve).toHaveBeenCalledWith({
      environment: expect.objectContaining({ id: expect.any(String) }),
      identifier: 'app.dot',
      tld: '.dot',
    });
    expect(result).toBe(saved);
  });

  it('returns null when chain returns nothing', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(null)) as any,
    );
    vi.mocked(fetchProductFromChain).mockResolvedValue(null);

    const result = await commitmentUseCase.commitProductByIdentifier('app.dot');

    expect(result).toBeNull();
    expect(productDb.upsert).not.toHaveBeenCalled();
  });

  it('returns null when upsert fails', async () => {
    const fresh = makeProduct();

    vi.mocked(productDb.getByBaseName).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(null)) as any,
    );
    vi.mocked(fetchProductFromChain).mockResolvedValue(fresh);
    vi.mocked(productDb.upsert).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(err(new Error('db error'))) as any,
    );

    const result = await commitmentUseCase.commitProductByIdentifier('app.dot');

    expect(result).toBeNull();
  });
});

describe('commitResolvedProduct', () => {
  it('persists the given Product without a chain call', async () => {
    const product = makeProduct();
    const saved = makeRecord({ pinned: false });

    vi.mocked(productDb.getByBaseName).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(null)) as any,
    );
    vi.mocked(productDb.upsert).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(saved)) as any,
    );

    const result = await commitmentUseCase.commitResolvedProduct(product);

    expect(fetchProductFromChain).not.toHaveBeenCalled();
    expect(productDb.upsert).toHaveBeenCalledWith(product, { pinned: false });
    expect(result).toBe(saved);
  });

  it('is idempotent — returns the existing row without re-persisting', async () => {
    const product = makeProduct();
    const row = makeRecord();

    vi.mocked(productDb.getByBaseName).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(row)) as any,
    );

    const result = await commitmentUseCase.commitResolvedProduct(product);

    expect(result).toBe(row);
    expect(fetchProductFromChain).not.toHaveBeenCalled();
    expect(productDb.upsert).not.toHaveBeenCalled();
  });
});

describe('pinProduct', () => {
  it('ALWAYS re-resolves chain (even when row exists) and upserts with pinned:true', async () => {
    const fresh = makeProduct({ displayName: 'Fresh' });
    const saved = makeRecord({ pinned: true });

    // Existing row in DB — should NOT short-circuit chain call
    vi.mocked(productDb.getByBaseName).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(makeRecord())) as any,
    );
    vi.mocked(fetchProductFromChain).mockResolvedValue(fresh);
    vi.mocked(productDb.upsert).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(saved)) as any,
    );

    const result = await commitmentUseCase.pinProduct('app.dot');

    expect(fetchProductFromChain).toHaveBeenCalledWith('app.dot');
    expect(productDb.upsert).toHaveBeenCalledWith(fresh, { pinned: true });
    expect(result).toBe(saved);
  });

  it('drops the chain-resolve cache entry', async () => {
    const fresh = makeProduct();
    const saved = makeRecord({ pinned: true });

    vi.mocked(fetchProductFromChain).mockResolvedValue(fresh);
    vi.mocked(productDb.upsert).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(saved)) as any,
    );

    await commitmentUseCase.pinProduct('app.dot');

    expect(invalidateChainResolve).toHaveBeenCalledWith({
      environment: expect.objectContaining({ id: expect.any(String) }),
      identifier: 'app.dot',
      tld: '.dot',
    });
  });

  it('returns null when chain returns nothing', async () => {
    vi.mocked(fetchProductFromChain).mockResolvedValue(null);

    const result = await commitmentUseCase.pinProduct('app.dot');

    expect(result).toBeNull();
    expect(productDb.upsert).not.toHaveBeenCalled();
  });

  it('kicks off archive prefetch after persisting the pinned row', async () => {
    const fresh = makeProduct({ displayName: 'Fresh' });
    const saved = makeRecord({ pinned: true });

    vi.mocked(fetchProductFromChain).mockResolvedValue(fresh);
    vi.mocked(productDb.upsert).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok(saved)) as any,
    );
    const prefetch = vi.spyOn(offlineCacheUseCase, 'prefetchArchives').mockResolvedValue();

    await commitmentUseCase.pinProduct('app.dot');

    expect(prefetch).toHaveBeenCalledTimes(1);
    expect(prefetch).toHaveBeenCalledWith(saved);
  });
});

describe('pinExecutable', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  const widgetOld: WidgetExecutable = {
    kind: 'widget',
    identifier: 'widget.app.dot',
    contenthash: '0xold',
    appVersion: [1, 0, 0],
    dimensions: { height: [400] },
  };

  it('returns null when the row is not pinned', async () => {
    vi.mocked(productDb.getByBaseName).mockResolvedValue(
      ok({ ...makeRecord({ pinned: false }), executables: { widget: widgetOld } }),
    );
    const result = await commitmentUseCase.pinExecutable({ identifier: 'app.dot', kind: 'widget' });
    expect(result).toBeNull();
    expect(resolveFreshExecutable).not.toHaveBeenCalled();
  });

  it('returns null when the pinned row has no such frozen kind', async () => {
    vi.mocked(productDb.getByBaseName).mockResolvedValue(ok({ ...makeRecord({ pinned: true }), executables: {} }));
    const result = await commitmentUseCase.pinExecutable({ identifier: 'app.dot', kind: 'widget' });
    expect(result).toBeNull();
    expect(resolveFreshExecutable).not.toHaveBeenCalled();
    expect(productDb.updateExecutable).not.toHaveBeenCalled();
  });

  it('returns null (no write) when no fresh executable can be built', async () => {
    vi.mocked(productDb.getByBaseName).mockResolvedValue(
      ok({ ...makeRecord({ pinned: true }), executables: { widget: widgetOld } }),
    );
    vi.mocked(resolveFreshExecutable).mockResolvedValue(null);
    const result = await commitmentUseCase.pinExecutable({ identifier: 'app.dot', kind: 'widget' });
    expect(result).toBeNull();
    expect(productDb.updateExecutable).not.toHaveBeenCalled();
  });

  it('re-freezes exactly the target kind against the frozen executable and prefetches', async () => {
    const frozen = { ...makeRecord({ pinned: true }), executables: { app: undefined, widget: widgetOld } };
    const freshWidget: WidgetExecutable = { ...widgetOld, contenthash: '0xwidgetNew', appVersion: [1, 1, 1] };
    const updatedRow = { ...frozen, executables: { widget: freshWidget }, updatedAt: 2000 };
    vi.mocked(productDb.getByBaseName).mockResolvedValue(ok(frozen));
    vi.mocked(resolveFreshExecutable).mockResolvedValue(freshWidget);
    vi.mocked(productDb.updateExecutable).mockResolvedValue(ok(updatedRow));

    const result = await commitmentUseCase.pinExecutable({ identifier: 'app.dot', kind: 'widget' });

    // Detection & application share the primitive: it is called with the FROZEN executable.
    expect(resolveFreshExecutable).toHaveBeenCalledWith('app.dot', widgetOld);
    // The transactional per-kind write is the one that persists (no whole-map read-modify-write).
    expect(productDb.updateExecutable).toHaveBeenCalledWith('app.dot', 'widget', freshWidget);
    expect(result).toBe(updatedRow);
    expect(offlineCacheUseCase.prefetchArchives).toHaveBeenCalledWith(updatedRow);
  });
});

describe('unpinProduct', () => {
  it('flips pinned:false and returns true', async () => {
    vi.mocked(productDb.update).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(ok('app.dot')) as any,
    );

    const result = await commitmentUseCase.unpinProduct('app.dot');

    expect(productDb.update).toHaveBeenCalledWith('app.dot', expect.objectContaining({ pinned: false }));
    expect(result).toBe(true);
  });

  it('returns false when update fails', async () => {
    vi.mocked(productDb.update).mockReturnValue(
      // eslint-disable-next-line @typescript-eslint/consistent-type-assertions, @typescript-eslint/no-explicit-any
      Promise.resolve(err(new Error('db error'))) as any,
    );

    const result = await commitmentUseCase.unpinProduct('app.dot');

    expect(result).toBe(false);
  });
});
