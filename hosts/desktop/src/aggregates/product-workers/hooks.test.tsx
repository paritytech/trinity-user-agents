// @vitest-environment happy-dom

import { act, cleanup, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { type ProductWorkerInstance } from '@/domains/product';
import { truapiRuntimeUseCase } from '@/aggregates/truapi-runtime';

import { useProductWorkerInstance, useWorkerDemand } from './hooks';
import { productWorkerRegistry } from './state/registry';

// The hooks no longer own a worker's lifetime — they declare demand for one and read
// back whatever the demand watcher registered. Building, registering and disposing is
// `productWorkersUseCase`'s job and is covered in its own spec.
//
// Spied in place rather than mocked: `acquireWorker` awaits a runtime that no test
// here boots, so without a double every call would hang on `whenRuntimeReady`.
const acquire = vi.spyOn(truapiRuntimeUseCase, 'acquireWorker').mockResolvedValue(undefined);
const release = vi.spyOn(truapiRuntimeUseCase, 'releaseWorker').mockResolvedValue(undefined);

function fakeInstance(productId: string): ProductWorkerInstance {
  return {
    productId,
    contenthash: 'cid-1',
    sandbox: {} as ProductWorkerInstance['sandbox'],
    coreProvider: {} as ProductWorkerInstance['coreProvider'],
    disposed: false,
    dispose: vi.fn(),
  };
}

const Probe = ({ productId }: { productId: string }) => {
  useWorkerDemand(productId);

  return null;
};

beforeEach(() => {
  vi.clearAllMocks();
  for (const id of Object.keys(productWorkerRegistry.instances$.get())) {
    const inst = productWorkerRegistry.get(id);
    if (inst) productWorkerRegistry.unregister(inst);
  }
});

afterEach(() => {
  cleanup();
});

describe('useWorkerDemand', () => {
  it('holds one reference for the mount and drops it on unmount', async () => {
    const { unmount } = render(<Probe productId="a.dot" />);

    await waitFor(() => expect(acquire).toHaveBeenCalledWith('a.dot'));
    expect(release).not.toHaveBeenCalled();

    unmount();

    expect(release).toHaveBeenCalledExactlyOnceWith('a.dot');
  });

  // A redeploy changes the product's bytes, not which product is wanted. Releasing
  // and re-acquiring would drop demand to zero and bounce a worker the user is using.
  it('does not re-acquire while the product id is unchanged', async () => {
    const { rerender } = render(<Probe productId="a.dot" />);
    await waitFor(() => expect(acquire).toHaveBeenCalledTimes(1));

    rerender(<Probe productId="a.dot" />);

    expect(acquire).toHaveBeenCalledTimes(1);
    expect(release).not.toHaveBeenCalled();
  });
});

describe('useProductWorkerInstance', () => {
  it('reports the registered instance and follows registration changes', async () => {
    const seen: (ProductWorkerInstance | null)[] = [];
    const Reader = () => {
      seen.push(useProductWorkerInstance('a.dot'));

      return null;
    };

    render(<Reader />);
    expect(seen.at(-1)).toBeNull();

    const inst = fakeInstance('a.dot');
    // The registry write is what re-renders the reader, so it is the call that has to
    // happen inside `act`.
    act(() => productWorkerRegistry.register(inst));

    await waitFor(() => expect(seen.at(-1)).toBe(inst));

    act(() => productWorkerRegistry.unregister(inst));

    await waitFor(() => expect(seen.at(-1)).toBeNull());
  });
});
