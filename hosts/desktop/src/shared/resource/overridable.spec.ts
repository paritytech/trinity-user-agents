import { Subject, firstValueFrom, of } from 'rxjs';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { resetExecutionEnvironment, setExecutionEnvironment } from '@/shared/execution-environment';

import { createQueryResource } from './createQueryResource';
import { createStreamResource } from './createStreamResource';

const realRequest = vi.fn((params: { id: number }) => Promise.resolve(`real-${params.id}`));

const queryResource = createQueryResource<{ id: number }>({ key: ({ id }) => `q-${id}` })
  .request<string>(realRequest)
  .cache<Record<string, string>>({
    initial: {},
    staleAfter: Number.POSITIVE_INFINITY,
    map: (cache, value, { id }) => ({ ...cache, [`q-${id}`]: value }),
  })
  .build();

function read(id: number) {
  return firstValueFrom(queryResource.read$({ id }));
}

describe('resource.instead', () => {
  it('serves the override instead of the real request', async () => {
    queryResource.instead(({ id }) => `fake-${id}`);

    await expect(read(1)).resolves.toBe('fake-1');
    expect(realRequest).not.toHaveBeenCalled();
  });

  // The failure mode this exists to prevent: without invalidating on override,
  // a resource that has already read something serves the OLD value and the
  // override silently does nothing. `staleAfter: Infinity` is common here, so
  // this would be the default experience rather than an edge case.
  it('takes effect even when the real request already cached a value', async () => {
    await expect(read(2)).resolves.toBe('real-2');

    queryResource.instead(({ id }) => `fake-${id}`);

    await expect(read(2)).resolves.toBe('fake-2');
  });

  it('supports a per-case answer, including a rejection', async () => {
    queryResource.instead(() => Promise.reject(new Error('boom')));

    await expect(read(3)).rejects.toThrow('boom');
  });

  it('restores the real request on reset, and drops what the override cached', async () => {
    queryResource.instead(({ id }) => `fake-${id}`);
    await expect(read(4)).resolves.toBe('fake-4');

    resetExecutionEnvironment();

    await expect(read(4)).resolves.toBe('real-4');
  });

  // Resources are module singletons and a cached entry outlives the test that
  // produced it, so a reset empties every cache, not only the overridden ones.
  it('empties the cache of a resource that was never overridden', async () => {
    await expect(read(5)).resolves.toBe('real-5');
    expect(queryResource.snapshot()).not.toEqual({});

    resetExecutionEnvironment();

    expect(queryResource.snapshot()).toEqual({});
  });
});

describe('resource.instead — stream resources', () => {
  it('swaps the subscription factory', async () => {
    const upstream = new Subject<string>();
    const streamResource = createStreamResource<{ id: number }>({ key: ({ id }) => `s-${id}` })
      .subscribe<string>(() => upstream.asObservable())
      .cache<Record<string, string>>({
        initial: {},
        map: (cache, value, { id }) => ({ ...cache, [`s-${id}`]: value }),
      })
      .build();

    const fake = new Subject<string>();
    streamResource.instead(() => fake.asObservable());

    const first = firstValueFrom(streamResource.read$({ id: 1 }));
    fake.next('from-override');

    await expect(first).resolves.toBe('from-override');
  });
});

const realMockedRequest = vi.fn((params: { id: number }) => Promise.resolve(`real-${params.id}`));

const mockedResource = createQueryResource<{ id: number }>({ key: ({ id }) => `m-${id}` })
  .request<string>(realMockedRequest)
  .mock(({ id }) => `mock-${id}`)
  .cache<Record<string, string>>({
    initial: {},
    staleAfter: Number.POSITIVE_INFINITY,
    map: (cache, value, { id }) => ({ ...cache, [`m-${id}`]: value }),
  })
  .build();

function readMocked(id: number) {
  return firstValueFrom(mockedResource.read$({ id }));
}

// `vitest.setup.js` puts the whole suite in the test execution environment, which
// is what serves every builder mock; a case that needs the real request switches
// to 'runtime' and the global `afterEach` switches back.
describe('resource.mock', () => {
  // `vitest.config.ts` sets no `clearMocks`, so the spy's count would otherwise
  // accumulate across this file and make the assertions order-dependent.
  afterEach(() => {
    realMockedRequest.mockClear();
  });

  it('serves the builder mock under the test environment', async () => {
    await expect(readMocked(1)).resolves.toBe('mock-1');
    expect(realMockedRequest).toHaveBeenCalledTimes(0);
  });

  it('serves the real request under the runtime environment', async () => {
    setExecutionEnvironment('runtime');

    await expect(readMocked(2)).resolves.toBe('real-2');
  });

  // The priority the whole seam rests on: a per-case answer must beat the
  // builder's blanket one, otherwise a spec cannot express its own scenario.
  it('lets instead win over the mock', async () => {
    mockedResource.instead(({ id }) => `fake-${id}`);

    await expect(readMocked(3)).resolves.toBe('fake-3');
  });

  it('falls back to the mock once the instead override is reset', async () => {
    mockedResource.instead(({ id }) => `fake-${id}`);
    await expect(readMocked(4)).resolves.toBe('fake-4');

    resetExecutionEnvironment();

    await expect(readMocked(4)).resolves.toBe('mock-4');
  });

  // Why the test environment can be on globally from day one: it is a no-op for
  // every resource that has not declared a mock, so no existing spec changes.
  it('leaves a resource that declared no mock on its real request', async () => {
    await expect(read(6)).resolves.toBe('real-6');
  });
});

describe('resource.mock — stream resources', () => {
  it('serves the mock factory under the test environment', async () => {
    const streamResource = createStreamResource<{ id: number }>({ key: ({ id }) => `sm-${id}` })
      .subscribe<string>(() => new Subject<string>().asObservable())
      .mock(({ id }) => of(`mock-${id}`))
      .cache<Record<string, string>>({
        initial: {},
        map: (cache, value, { id }) => ({ ...cache, [`sm-${id}`]: value }),
      })
      .build();

    await expect(firstValueFrom(streamResource.read$({ id: 1 }))).resolves.toBe('mock-1');
  });
});
