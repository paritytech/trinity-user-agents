import { type TrUApiProductProvider } from '@parity/truapi-host';
import { describe, expect, it, vi } from 'vitest';

import { type Sandbox } from '@/shared/sandbox';

import { createProductWorker } from './instance';

function makeFakeProvider() {
  const listeners: ((message: Uint8Array) => void)[] = [];
  const postMessage = vi.fn();
  const dispose = vi.fn();

  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
  const provider = {
    postMessage,
    subscribe: vi.fn((listener: (message: Uint8Array) => void) => {
      listeners.push(listener);

      return () => {
        listeners.splice(listeners.indexOf(listener), 1);
      };
    }),
    dispose,
  } as unknown as TrUApiProductProvider;

  const emit = (message: Uint8Array) => {
    for (const listener of [...listeners]) listener(message);
  };

  return { provider, postMessage, dispose, emit };
}

function makeFakeSandbox(overrides: Partial<{ disposeImpl: () => void }> = {}) {
  const wire = makeFakeProvider();
  const sandboxDispose = vi.fn(overrides.disposeImpl ?? (() => {}));
  let runRejectFn: ((e: unknown) => void) | null = null;
  const run = vi.fn(
    () =>
      new Promise<void>((_res, rej) => {
        runRejectFn = rej;
      }),
  );

  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
  const sandbox = { provider: wire.provider, dispose: sandboxDispose, run } as unknown as Sandbox;

  return { sandbox, wire, sandboxDispose, run, rejectRun: (e: unknown) => runRejectFn?.(e) };
}

const enc = (src: string) => new TextEncoder().encode(src);
// Worker code lives in the archive; the factory resolves the entrypoint from it.
const archive = (src = '') => ({ files: { 'index.js': enc(src) }, entrypoint: 'index.js' });

describe('createProductWorker', () => {
  it('builds an instance with the expected identifying fields', async () => {
    const fake = makeFakeSandbox();
    const core = makeFakeProvider();

    const inst = await createProductWorker({
      productId: 'a.dot',
      contenthash: 'cid-1',
      ...archive(),
      coreProvider: core.provider,
      createSandbox: vi.fn(async () => fake.sandbox),
    });

    expect(inst.productId).toBe('a.dot');
    expect(inst.contenthash).toBe('cid-1');
    expect(inst.sandbox).toBe(fake.sandbox);
    expect(inst.disposed).toBe(false);
  });

  it('forwards frames both ways between the sandbox and the core', async () => {
    const fake = makeFakeSandbox();
    const core = makeFakeProvider();

    await createProductWorker({
      productId: 'a.dot',
      contenthash: 'cid-1',
      ...archive(),
      coreProvider: core.provider,
      createSandbox: vi.fn(async () => fake.sandbox),
    });

    fake.wire.emit(enc('to-core'));
    expect(core.postMessage).toHaveBeenCalledWith(enc('to-core'));

    core.emit(enc('to-product'));
    expect(fake.wire.postMessage).toHaveBeenCalledWith(enc('to-product'));
  });

  it('starts run() but does not await it', async () => {
    const fake = makeFakeSandbox();
    const core = makeFakeProvider();

    const inst = await createProductWorker({
      productId: 'a.dot',
      contenthash: 'cid-1',
      ...archive('CODE'),
      coreProvider: core.provider,
      createSandbox: vi.fn(async () => fake.sandbox),
    });

    expect(fake.run).toHaveBeenCalledTimes(1);
    expect(fake.run).toHaveBeenCalledWith(enc('CODE'), { name: 'index.js' });
    expect(inst.disposed).toBe(false);
  });

  it('dispose() stops forwarding before tearing either end down', async () => {
    const fake = makeFakeSandbox();
    const core = makeFakeProvider();
    const order: string[] = [];
    core.dispose.mockImplementation(() => order.push('core.dispose'));
    fake.sandboxDispose.mockImplementation(() => order.push('sandbox.dispose'));

    const inst = await createProductWorker({
      productId: 'a.dot',
      contenthash: 'cid-1',
      ...archive(),
      coreProvider: core.provider,
      createSandbox: vi.fn(async () => fake.sandbox),
    });

    inst.dispose();

    expect(order).toEqual(['core.dispose', 'sandbox.dispose']);
    expect(inst.disposed).toBe(true);

    // A frame arriving after dispose must not reach the other end.
    fake.wire.emit(enc('late'));
    expect(core.postMessage).not.toHaveBeenCalled();
  });

  it('dispose() is idempotent', async () => {
    const fake = makeFakeSandbox();
    const core = makeFakeProvider();

    const inst = await createProductWorker({
      productId: 'a.dot',
      contenthash: 'cid-1',
      ...archive(),
      coreProvider: core.provider,
      createSandbox: vi.fn(async () => fake.sandbox),
    });

    inst.dispose();
    inst.dispose();
    inst.dispose();

    expect(core.dispose).toHaveBeenCalledTimes(1);
    expect(fake.sandboxDispose).toHaveBeenCalledTimes(1);
  });

  it('dispose() swallows sandbox.dispose() errors', async () => {
    const fake = makeFakeSandbox({
      disposeImpl: () => {
        throw new Error('quickjs abort');
      },
    });
    const core = makeFakeProvider();

    const inst = await createProductWorker({
      productId: 'a.dot',
      contenthash: 'cid-1',
      ...archive(),
      coreProvider: core.provider,
      createSandbox: vi.fn(async () => fake.sandbox),
    });

    expect(() => inst.dispose()).not.toThrow();
    expect(inst.disposed).toBe(true);
  });

  it('a late run() rejection after dispose does not throw out of the factory', async () => {
    const fake = makeFakeSandbox();
    const core = makeFakeProvider();

    const inst = await createProductWorker({
      productId: 'a.dot',
      contenthash: 'cid-1',
      ...archive(),
      coreProvider: core.provider,
      createSandbox: vi.fn(async () => fake.sandbox),
    });

    inst.dispose();
    fake.rejectRun(new Error('use-after-free'));
    await Promise.resolve();
    expect(inst.disposed).toBe(true);
  });
});
