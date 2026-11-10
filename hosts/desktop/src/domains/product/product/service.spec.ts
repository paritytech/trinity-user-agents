import { describe, expect, it } from 'vitest';

import { type AppExecutable, type LiveExecutable } from './manifest/types';
import { productService } from './service';
import { type Product } from './types';

function makeProduct(overrides: Partial<Product> = {}): Product {
  return {
    baseName: 'hackm3.dot',
    displayName: 'Hack Me',
    description: '',
    icon: { cid: 'abc', format: 'png' },
    executables: {},
    ...overrides,
  };
}

describe('matchesQuery', () => {
  it('matches the display name case-insensitively', () => {
    expect(productService.matchesQuery(makeProduct({ displayName: 'Acme Wallet' }), 'acme')).toBe(true);
  });

  it('matches the base name', () => {
    expect(productService.matchesQuery(makeProduct({ baseName: 'hackm3.dot' }), 'HACKM3')).toBe(true);
  });

  it('rejects a query matching neither field', () => {
    expect(productService.matchesQuery(makeProduct(), 'unrelated')).toBe(false);
  });
});

describe('hasExecutableDrift', () => {
  const frozen: AppExecutable = { kind: 'app', identifier: 'app.dot', contenthash: '0xold', appVersion: [1, 0, 0] };

  it('is drift when the live contenthash differs', () => {
    const live: LiveExecutable = { contenthash: '0xnew', version: [1, 0, 1] };
    expect(productService.hasExecutableDrift(frozen, live)).toBe(true);
  });

  it('is not drift when the live contenthash matches', () => {
    const live: LiveExecutable = { contenthash: '0xold', version: [1, 0, 0] };
    expect(productService.hasExecutableDrift(frozen, live)).toBe(false);
  });

  it('is not drift when there is no live resolution (null)', () => {
    expect(productService.hasExecutableDrift(frozen, null)).toBe(false);
  });
});
