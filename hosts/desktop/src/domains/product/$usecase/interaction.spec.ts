import { NEVER, firstValueFrom, of, throwError } from 'rxjs';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { dotNsTldResource } from '../dotns/resource';
import { productsResource } from '../product/resource';

import { type InteractedProduct, interactionUseCase } from './interaction';
import { permissionsUseCase } from './permissions';

function committedNames(result: InteractedProduct[]) {
  return result.filter(entry => entry.kind === 'committed').map(entry => entry.product.baseName);
}

function permissionOnlyIds(result: InteractedProduct[]) {
  return result.filter(entry => entry.kind === 'permissionOnly').map(entry => entry.productId);
}

function makeRecord(baseName: string) {
  return {
    baseName,
    displayName: baseName,
    description: '',
    icon: { cid: 'abc', format: 'png' as const },
    executables: {},
    pinned: false,
    createdAt: 1000,
    updatedAt: 1000,
  };
}

// `productsResource` needs no stub for the empty case: its builder mock already
// serves `[]`. Cases that need rows state them with `instead`, which is undone
// after each test.
describe('interactionUseCase.watchInteractedProducts', () => {
  beforeEach(() => {
    // A non-`.dot` network throughout, so a helper that stopped taking the TLD from
    // the use case would fail here instead of passing on the resource's own fallback.
    dotNsTldResource.instead(() => '.paseo');
    // The permission slot stream comes from the core-storage table; spied in place so
    // no case has to seed Dexie rows to describe it.
    vi.spyOn(permissionsUseCase, 'watchProductsWithPermissions').mockReturnValue(of([]));
  });

  it('returns committed products with no permission-only ids when stores are empty', async () => {
    productsResource.instead(() => of([makeRecord('committed.paseo')]));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(committedNames(result)).toEqual(['committed.paseo']);
    expect(permissionOnlyIds(result)).toEqual([]);
  });

  it('orders committed products before permission-only entries', async () => {
    productsResource.instead(() => of([makeRecord('committed.paseo')]));
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['browsed.paseo']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(result.map(entry => entry.kind)).toEqual(['committed', 'permissionOnly']);
  });

  it('lists a product with a permissions row but no committed row as permission-only', async () => {
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['browsed.paseo']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(committedNames(result)).toEqual([]);
    expect(permissionOnlyIds(result)).toEqual(['browsed.paseo']);
  });

  it('lists an alias requester with no committed row as permission-only', async () => {
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['aliased.paseo']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(permissionOnlyIds(result)).toEqual(['aliased.paseo']);
  });

  it('does not duplicate a committed product that also has permission rows', async () => {
    productsResource.instead(() => of([makeRecord('committed.paseo')]));
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['committed.paseo']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(committedNames(result)).toEqual(['committed.paseo']);
    expect(permissionOnlyIds(result)).toEqual([]);
  });

  it('returns permission-only ids sorted', async () => {
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['zebra.paseo', 'alpha.paseo']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(permissionOnlyIds(result)).toEqual(['alpha.paseo', 'zebra.paseo']);
  });

  it('lists a product holding a permission slot once', async () => {
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['browsed.paseo']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(permissionOnlyIds(result)).toEqual(['browsed.paseo']);
  });

  it('dedups a permission row stored under a raw unnormalized id against its committed product', async () => {
    // Permission rows store the raw webview identifier; committed baseName is
    // always baseNameOf()-normalized (lowercase, suffixed with the network TLD).
    // This case is the sentinel for the TLD actually reaching the normalization.
    productsResource.instead(() => of([makeRecord('localhost:5173.paseo')]));
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['localhost:5173']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(permissionOnlyIds(result)).toEqual([]);
  });

  it('dedups raw-id variants of the same uncommitted product into one entry', async () => {
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(of(['Browsed.paseo', 'browsed.paseo']));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(permissionOnlyIds(result)).toEqual(['Browsed.paseo']);
  });

  it('keeps emitting committed products when the permission stream errors', async () => {
    productsResource.instead(() => of([makeRecord('committed.paseo')]));
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(throwError(() => new Error('unreachable core')));

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(committedNames(result)).toEqual(['committed.paseo']);
    expect(permissionOnlyIds(result)).toEqual([]);
  });

  it('emits committed products before the permission stream produces a first value', async () => {
    productsResource.instead(() => of([makeRecord('committed.paseo')]));
    vi.mocked(permissionsUseCase.watchProductsWithPermissions).mockReturnValue(NEVER);

    const result = await firstValueFrom(interactionUseCase.watchInteractedProducts());

    expect(committedNames(result)).toEqual(['committed.paseo']);
    expect(permissionOnlyIds(result)).toEqual([]);
  });
});
