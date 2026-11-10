// @vitest-environment happy-dom

import { afterEach, describe, expect, it, vi } from 'vitest';

import { delay } from '@/shared/utils';

import { inputRound, requestRound, resetRound } from './round';

afterEach(() => {
  resetRound();
});

describe('requestRound', () => {
  it('starts no round when the context set is empty', async () => {
    requestRound('anything', [], 'user');
    await delay(400);

    expect(inputRound.get()).toEqual({ kind: 'idle' });
  });

  it('starts no round for an empty field', async () => {
    requestRound('   ', ['wallet'], 'user');
    await delay(400);

    expect(inputRound.get()).toEqual({ kind: 'idle' });
  });

  it('resolves a deeplink to a navigation without querying anything', async () => {
    requestRound('polkadot://example.dot/wallet.dot/send', ['wallet'], 'user');
    await delay(400);

    expect(inputRound.get().kind).toBe('navigation');
  });

  it('still resolves a deeplink when the screen contributes no products', async () => {
    // A navigation carries its own context, so an empty context set must not
    // swallow it — RFC-0027 § Escaping the context.
    requestRound('polkadot://example.dot/wallet.dot/send', [], 'user');
    await delay(400);

    expect(inputRound.get().kind).toBe('navigation');
  });

  it('collapses a burst shorter than the debounce into one round', async () => {
    const seen: string[] = [];
    const sub = inputRound.value$.subscribe(view => seen.push(view.kind));

    requestRound('a', ['wallet'], 'user');
    requestRound('ab', ['wallet'], 'user');
    requestRound('abc', ['wallet'], 'user');
    await delay(2000);
    sub.unsubscribe();

    expect(inputRound.get().kind).toBe('query');
    // One debounce fired, so no round ever started for 'a' or 'ab'.
    expect(seen.filter(kind => kind === 'idle')).toHaveLength(1);
  });

  it('merges candidates as workers answer', async () => {
    requestRound('alice', ['wallet', 'notes', 'governance'], 'user');

    await vi.waitFor(
      () => {
        const view = inputRound.get();
        if (view.kind !== 'query') throw new Error('not a query yet');
        expect(view.asked).toBe(3);
        expect(view.pending).toBe(false);
      },
      { timeout: 4000, interval: 100 },
    );
  });
});
