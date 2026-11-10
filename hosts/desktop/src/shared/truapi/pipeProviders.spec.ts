import { describe, expect, it, vi } from 'vitest';

import { pipeProviders } from './pipeProviders';

function fakeProvider() {
  const listeners: ((message: Uint8Array) => void)[] = [];
  const closeListeners: ((error: Error) => void)[] = [];
  const sent: Uint8Array[] = [];

  return {
    sent,
    dispose: vi.fn(),
    postMessage(message: Uint8Array) {
      sent.push(message);
    },
    subscribe(callback: (message: Uint8Array) => void) {
      listeners.push(callback);

      return () => {
        listeners.splice(listeners.indexOf(callback), 1);
      };
    },
    subscribeClose(callback: (error: Error) => void) {
      closeListeners.push(callback);

      return () => {
        closeListeners.splice(closeListeners.indexOf(callback), 1);
      };
    },
    emit(message: Uint8Array) {
      for (const listener of [...listeners]) listener(message);
    },
    emitClose(error: Error) {
      for (const listener of [...closeListeners]) listener(error);
    },
  };
}

describe('pipeProviders', () => {
  it('forwards frames in both directions', () => {
    const product = fakeProvider();
    const core = fakeProvider();
    pipeProviders(product, core);

    product.emit(new Uint8Array([1]));
    core.emit(new Uint8Array([2]));

    expect(core.sent).toEqual([new Uint8Array([1])]);
    expect(product.sent).toEqual([new Uint8Array([2])]);
  });

  it('disposes the core when the product side closes', () => {
    const product = fakeProvider();
    const core = fakeProvider();
    pipeProviders(product, core);

    product.emitClose(new Error('gone'));

    expect(core.dispose).toHaveBeenCalledOnce();
  });

  it('disposes the product when the core side closes', () => {
    const product = fakeProvider();
    const core = fakeProvider();
    pipeProviders(product, core);

    core.emitClose(new Error('gone'));

    expect(product.dispose).toHaveBeenCalledOnce();
  });

  it('stops forwarding after teardown and is idempotent', () => {
    const product = fakeProvider();
    const core = fakeProvider();
    const dispose = pipeProviders(product, core);

    dispose();
    dispose();
    product.emit(new Uint8Array([1]));

    expect(core.sent).toEqual([]);
  });

  // A provider without the optional `subscribeClose` is still a valid WireProvider.
  it('pipes a provider that exposes no close channel', () => {
    const product = fakeProvider();
    const core = fakeProvider();
    const { subscribeClose: _omitted, ...withoutClose } = core;
    const dispose = pipeProviders(product, withoutClose);

    product.emit(new Uint8Array([1]));

    expect(withoutClose.sent).toEqual([new Uint8Array([1])]);
    expect(() => dispose()).not.toThrow();
  });
});
