// @vitest-environment happy-dom

import { renderHook, waitFor } from '@testing-library/react';
import { of } from 'rxjs';
import { beforeEach, describe, expect, it } from 'vitest';

import {
  type ExecutableKind,
  type LiveExecutable,
  type PersistedProduct,
  liveExecutableResource,
  productsResource,
} from '@/domains/product';

import { useAvailableUpdates, useNewerVersionAvailable } from './useNewerVersionAvailable';

const hexAa = '0xaa';
const hexBb = '0xbb';

type FrozenKinds = Partial<Record<ExecutableKind, { contenthash: string; appVersion?: number[] }>>;

function seedProduct(pinned: boolean, executables: FrozenKinds) {
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions -- test fixture, not production code
  const record = {
    baseName: 'a.dot',
    displayName: 'A',
    description: '',
    icon: { cid: '', format: 'png' },
    executables,
    pinned,
    createdAt: 1000,
    updatedAt: 1000,
  } as unknown as PersistedProduct;

  productsResource.instead(() => of([record]));
}

// The chain-side answer per kind, keyed the way the resource is.
function seedLive(map: Partial<Record<ExecutableKind, LiveExecutable | null>>) {
  liveExecutableResource.instead(({ kind }) => map[kind] ?? null);
}

describe('useAvailableUpdates', () => {
  beforeEach(() => {
    seedLive({});
  });

  it('returns [] when the product is not pinned', async () => {
    seedProduct(false, { worker: { contenthash: hexAa } });
    seedLive({ worker: { contenthash: hexBb, version: [0, 1, 1] } });

    const { result } = renderHook(() => useAvailableUpdates('a.dot'));

    await waitFor(() => expect(result.current).toEqual([]));
  });

  it('returns [] when every present kind matches chain', async () => {
    seedProduct(true, { app: { contenthash: hexAa }, worker: { contenthash: hexAa } });
    seedLive({
      app: { contenthash: hexAa, version: [2, 1, 0] },
      worker: { contenthash: hexAa, version: [0, 1, 0] },
    });

    const { result } = renderHook(() => useAvailableUpdates('a.dot'));

    await waitFor(() => expect(result.current).toEqual([]));
  });

  it('returns the drifted kinds with their frozen and fresh versions', async () => {
    seedProduct(true, {
      app: { contenthash: hexAa, appVersion: [2, 1, 0] },
      widget: { contenthash: hexAa, appVersion: [1, 1, 0] },
    });
    seedLive({
      app: { contenthash: hexBb, version: [2, 1, 1] },
      widget: { contenthash: hexAa, version: [1, 1, 0] },
    });

    const { result } = renderHook(() => useAvailableUpdates('a.dot'));

    await waitFor(() => expect(result.current).toEqual([{ kind: 'app', fromVersion: [2, 1, 0], toVersion: [2, 1, 1] }]));
  });
});

describe('useNewerVersionAvailable', () => {
  it('is true when at least one kind drifted', async () => {
    seedProduct(true, { worker: { contenthash: hexAa } });
    seedLive({ worker: { contenthash: hexBb, version: [0, 1, 1] } });

    const { result } = renderHook(() => useNewerVersionAvailable('a.dot'));

    await waitFor(() => expect(result.current).toBe(true));
  });

  it('is false when nothing drifted', async () => {
    seedProduct(true, { worker: { contenthash: hexAa } });
    seedLive({ worker: { contenthash: hexAa, version: [0, 1, 0] } });

    const { result } = renderHook(() => useNewerVersionAvailable('a.dot'));

    await waitFor(() => expect(result.current).toBe(false));
  });
});
