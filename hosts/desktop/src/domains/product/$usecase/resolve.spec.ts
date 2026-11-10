import { errAsync, okAsync } from 'neverthrow';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { dotNsGateway } from '../dotns/gateway';
import { MANIFEST_TEXT_RECORD_KEY } from '../product/manifest/constants';
import { type AppExecutable, type WidgetExecutable } from '../product/manifest/types';
import { type PersistedProduct, productDb } from '../product/repository';

import { resolveProductUseCase } from './resolve';

function makeRecord(overrides: Partial<PersistedProduct> = {}): PersistedProduct {
  return {
    baseName: 'app.dot',
    displayName: 'App',
    description: '',
    icon: { cid: 'abc', format: 'png' },
    executables: {},
    pinned: false,
    createdAt: 1000,
    updatedAt: 1000,
    ...overrides,
  };
}

// Drive `fetchProductFromChain` down its manifest branch by putting a real root
// manifest on the `manifest` text record. The real `parseRootManifest` and
// `assembleProduct` then produce the product, so what these tests assert is what a
// chain carrying this record actually yields.
function chainServesRootManifest(displayName: string) {
  vi.mocked(dotNsGateway.readResolver).mockResolvedValue('0xresolver');
  vi.mocked(dotNsGateway.readOwner).mockResolvedValue(null);
  // No executable subname carries a contenthash, so the product has no executables.
  vi.mocked(dotNsGateway.readContentHashAt).mockResolvedValue(null);
  vi.mocked(dotNsGateway.readText).mockImplementation((_env, _resolver, _node, key) =>
    Promise.resolve(
      key === MANIFEST_TEXT_RECORD_KEY
        ? JSON.stringify({ $v: 1, displayName, description: '', icon: { cid: 'abc', format: 'png' } })
        : null,
    ),
  );
}

// Both leaves are plain objects, spied in place: the gateway so no read reaches the
// chain, the repository so none reaches Dexie. The use case and the manifest
// service in between are the real code under test.
beforeEach(() => {
  vi.clearAllMocks();
  vi.spyOn(productDb, 'getAll');
  vi.spyOn(productDb, 'getByBaseName');
  vi.spyOn(productDb, 'update');
  vi.spyOn(dotNsGateway, 'readResolver');
  vi.spyOn(dotNsGateway, 'readOwner');
  vi.spyOn(dotNsGateway, 'readText');
  vi.spyOn(dotNsGateway, 'readContentHashAt');
  vi.spyOn(dotNsGateway, 'readLegacyContentHash');
  vi.mocked(productDb.update).mockReturnValue(okAsync('app.dot'));
});

describe('resolveProduct', () => {
  it('returns the committed row (as a Product) without a chain call', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord({ displayName: 'Stored' })));

    const result = await resolveProductUseCase.resolveProduct('app.dot');

    expect(result).toEqual(expect.objectContaining({ baseName: 'app.dot', displayName: 'Stored' }));
    // pure read — no persistence metadata leaks out
    expect(result).not.toHaveProperty('pinned');
    expect(dotNsGateway.readResolver).not.toHaveBeenCalled();
  });

  it('falls back to chain resolution when no row is committed', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(null));
    chainServesRootManifest('FromChain');

    const result = await resolveProductUseCase.resolveProduct('app.dot');

    expect(result).toEqual(expect.objectContaining({ baseName: 'app.dot', displayName: 'FromChain' }));
  });

  it('never writes the DB', async () => {
    vi.mocked(productDb.getByBaseName).mockReturnValue(okAsync(makeRecord()));

    await resolveProductUseCase.resolveProduct('app.dot');

    expect(productDb.update).not.toHaveBeenCalled();
  });
});

describe('reconcileUnpinnedProducts', () => {
  it('re-resolves an unpinned row and persists the diff', async () => {
    vi.mocked(productDb.getAll).mockReturnValue(okAsync([makeRecord({ displayName: 'Old Name' })]));
    chainServesRootManifest('New Name');

    await resolveProductUseCase.reconcileUnpinnedProducts();

    expect(productDb.update).toHaveBeenCalledWith('app.dot', expect.objectContaining({ displayName: 'New Name' }));
  });

  it('does not write when chain data matches the stored row', async () => {
    vi.mocked(productDb.getAll).mockReturnValue(okAsync([makeRecord({ displayName: 'Same' })]));
    chainServesRootManifest('Same');

    await resolveProductUseCase.reconcileUnpinnedProducts();

    expect(productDb.update).not.toHaveBeenCalled();
  });

  it('skips pinned rows entirely — no chain call, no write', async () => {
    vi.mocked(productDb.getAll).mockReturnValue(okAsync([makeRecord({ pinned: true })]));
    chainServesRootManifest('New Name');

    await resolveProductUseCase.reconcileUnpinnedProducts();

    expect(dotNsGateway.readResolver).not.toHaveBeenCalled();
    expect(productDb.update).not.toHaveBeenCalled();
  });

  it('reconciles only the unpinned rows in a mixed set', async () => {
    vi.mocked(productDb.getAll).mockReturnValue(
      okAsync([makeRecord({ baseName: 'pinned.dot', pinned: true }), makeRecord({ baseName: 'fresh.dot', displayName: 'Old' })]),
    );
    chainServesRootManifest('New');

    await resolveProductUseCase.reconcileUnpinnedProducts();

    expect(productDb.update).toHaveBeenCalledTimes(1);
    expect(productDb.update).toHaveBeenCalledWith('fresh.dot', expect.objectContaining({ displayName: 'New' }));
  });

  it('does nothing when the product list cannot be read', async () => {
    vi.mocked(productDb.getAll).mockReturnValue(errAsync(new Error('db')));

    await resolveProductUseCase.reconcileUnpinnedProducts();

    expect(productDb.update).not.toHaveBeenCalled();
  });

  it('continues past a row whose chain re-resolve throws', async () => {
    vi.mocked(productDb.getAll).mockReturnValue(okAsync([makeRecord({ displayName: 'Old' })]));
    vi.mocked(dotNsGateway.readResolver).mockRejectedValue(new Error('rpc'));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});

    await expect(resolveProductUseCase.reconcileUnpinnedProducts()).resolves.toBeUndefined();
    expect(productDb.update).not.toHaveBeenCalled();
    warn.mockRestore();
  });
});

describe('resolveFreshExecutable', () => {
  const frozenApp: AppExecutable = { kind: 'app', identifier: 'app.dot', contenthash: '0xold', appVersion: [2, 1, 0] };
  const frozenWidget: WidgetExecutable = {
    kind: 'widget',
    identifier: 'widget.app.dot',
    contenthash: '0xold',
    appVersion: [1, 0, 0],
    dimensions: { height: [400] },
  };

  it('builds a fresh executable from a parseable manifest', async () => {
    vi.mocked(dotNsGateway.readResolver).mockResolvedValue('0xresolver');
    vi.mocked(dotNsGateway.readContentHashAt).mockResolvedValue('0xnew');
    vi.mocked(dotNsGateway.readText).mockResolvedValue(JSON.stringify({ $v: 1, kind: 'app', appVersion: [2, 1, 1] }));

    const result = await resolveProductUseCase.resolveFreshExecutable('app.dot', frozenApp);

    // The identifier is the executable's own subname (`dotNsService.subnameOf`), which
    // for the manifest branch is always `<kind>.<base>` — the legacy branch below is
    // the one that lives at the bare base.
    expect(result).toEqual({ kind: 'app', identifier: 'app.app.dot', appVersion: [2, 1, 1], contenthash: '0xnew' });
  });

  it('synthesizes an app executable via legacyApp when the manifest is unparseable', async () => {
    vi.mocked(dotNsGateway.readResolver).mockResolvedValue('0xresolver');
    vi.mocked(dotNsGateway.readContentHashAt).mockResolvedValue('0xnew');
    vi.mocked(dotNsGateway.readText).mockResolvedValue('garbage');

    const result = await resolveProductUseCase.resolveFreshExecutable('app.dot', frozenApp);

    // A legacy app carries no declared version.
    expect(result).toEqual({ kind: 'app', identifier: 'app.dot', appVersion: [0, 0, 0], contenthash: '0xnew' });
  });

  it('returns null for a widget whose manifest will not parse (needs manifest-only fields)', async () => {
    vi.mocked(dotNsGateway.readResolver).mockResolvedValue('0xresolver');
    vi.mocked(dotNsGateway.readContentHashAt).mockResolvedValue('0xnew');
    vi.mocked(dotNsGateway.readText).mockResolvedValue('garbage');

    const result = await resolveProductUseCase.resolveFreshExecutable('app.dot', frozenWidget);

    expect(result).toBeNull();
  });

  it('does not lose the contenthash drift signal when the manifest text read fails', async () => {
    vi.mocked(dotNsGateway.readResolver).mockResolvedValue('0xresolver');
    vi.mocked(dotNsGateway.readContentHashAt).mockResolvedValue('0xnew');
    // A text-record read/decode failure must degrade to "no manifest", not reject —
    // the fresh contenthash still has to come back so drift detection sees it.
    vi.mocked(dotNsGateway.readText).mockRejectedValue(new Error('decode'));

    const result = await resolveProductUseCase.resolveFreshExecutable('app.dot', frozenApp);

    expect(result).toEqual({ kind: 'app', identifier: 'app.dot', appVersion: [0, 0, 0], contenthash: '0xnew' });
  });

  it('falls back to the legacy contenthash when the node carries no registry resolver', async () => {
    vi.mocked(dotNsGateway.readResolver).mockResolvedValue(null);
    vi.mocked(dotNsGateway.readLegacyContentHash).mockResolvedValue('0xlegacy');

    const result = await resolveProductUseCase.resolveFreshExecutable('app.dot', frozenApp);

    expect(result).toEqual({ kind: 'app', identifier: 'app.dot', appVersion: [0, 0, 0], contenthash: '0xlegacy' });
  });

  it('returns null when no contenthash exists on chain at all', async () => {
    vi.mocked(dotNsGateway.readResolver).mockResolvedValue(null);
    vi.mocked(dotNsGateway.readLegacyContentHash).mockResolvedValue(null);

    const result = await resolveProductUseCase.resolveFreshExecutable('app.dot', frozenApp);

    expect(result).toBeNull();
  });
});
